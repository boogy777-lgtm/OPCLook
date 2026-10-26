//! Native mDNS/DNS-SD discovery of OPC UA servers.
//!
//! Windows provides the DNS-SD API in `dnsapi.dll`:
//!
//! ```text
//! DnsServiceBrowse("_opcua-tcp._tcp.local") -> service instance names
//! DnsServiceResolve(instance)              -> SRV host/port, A/AAAA, TXT
//! ```
//!
//! Both calls are asynchronous and report results through a callback. The
//! callback context here is a numeric registry id, not a pointer to Rust
//! memory. A callback only looks the id up in a process-wide registry and
//! clones an `Arc`; if the search has already unregistered the id, the
//! callback returns without touching any freed memory. This makes the
//! "cancel then free" path safe even if the OS delivers one last callback
//! after `DnsServiceBrowseCancel` / `DnsServiceResolveCancel` returns.
//!
//! The whole discovery is bounded by [`MDNS_WINDOW`] (5 s), and the interface
//! index comes from [`NetworkBinding`] (IPv4 index for an IPv4 source, IPv6
//! index for an IPv6 source, 0/all interfaces for loopback or an unbound
//! search).

use crate::bff::net::{NetworkBinding, endpoint_url_for};
use crate::constants::MDNS_WINDOW;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

/// DNS-SD service type used by OPC UA TCP endpoints.
pub(crate) const OPCUA_SERVICE_TYPE: &str = "_opcua-tcp._tcp.local";

/// Time reserved inside [`MDNS_WINDOW`] for SRV/TXT resolution after browsing.
const MDNS_RESOLVE_RESERVE: Duration = Duration::from_secs(1);

/// One resolved DNS-SD service instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdnService {
    /// Fully qualified instance name (`<name>._opcua-tcp._tcp.local`).
    pub instance_name: String,
    /// Human-readable instance label.
    pub application_name: String,
    /// Application URI from TXT, when advertised.
    pub application_uri: String,
    /// SRV target host (without the trailing root dot).
    pub host: String,
    /// SRV port.
    pub port: u16,
    /// Resolved A/AAAA addresses, IPv4 first.
    pub addresses: Vec<IpAddr>,
    /// Interface on which the service was discovered.
    pub interface_index: u32,
}

impl MdnService {
    /// Discovery URLs for this service. IP literals are preferred; IPv6 is
    /// bracketed. The SRV host is used only when no address was resolved.
    pub fn discovery_urls(&self) -> Vec<String> {
        let mut urls = Vec::new();
        for address in &self.addresses {
            let url = endpoint_url_for(*address, self.port);
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        if urls.is_empty() {
            if let Some(url) = service_url(&self.host, self.port) {
                urls.push(url);
            }
        }
        urls
    }
}

/// Interface index for a DNS-SD request.
///
/// IPv4 source address uses `IfIndex`; IPv6 source uses `Ipv6IfIndex`.
/// Loopback or an unbound search uses 0 (all interfaces).
pub(crate) fn interface_index_for(binding: Option<&NetworkBinding>) -> u32 {
    let Some(binding) = binding else {
        return 0;
    };
    if binding.is_loopback() {
        return 0;
    }
    match binding.local_ip {
        IpAddr::V4(_) => binding.ipv4_if_index.unwrap_or(0),
        IpAddr::V6(_) => binding.ipv6_if_index.unwrap_or(0),
    }
}

/// Run a bounded mDNS discovery. Never panics; failures are logged and
/// reported as an empty list.
pub(crate) fn discover(binding: Option<&NetworkBinding>) -> Vec<MdnService> {
    #[cfg(windows)]
    {
        let interface = interface_index_for(binding);
        match windows_impl::discover(interface) {
            Ok(services) => services,
            Err(error) => {
                eprintln!("mDNS discovery failed: {error}");
                Vec::new()
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = binding;
        eprintln!("mDNS discovery is not supported on this platform");
        Vec::new()
    }
}

/// Strip a DNS trailing dot and URL brackets from a host.
pub(crate) fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_end_matches('.')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string()
}

/// Extract the human-readable label from a DNS-SD instance name.
pub(crate) fn instance_label(instance: &str) -> String {
    let name = instance.trim().trim_end_matches('.');
    match name.split_once("._") {
        Some((label, _)) => label.trim().to_string(),
        None => name.to_string(),
    }
}

/// Build `opc.tcp://host:port`, bracketing IPv6 literals.
pub(crate) fn service_url(host: &str, port: u16) -> Option<String> {
    if port == 0 {
        return None;
    }
    let host = normalize_host(host);
    if host.is_empty() {
        return None;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(endpoint_url_for(ip, port));
    }
    Some(format!("opc.tcp://{host}:{port}"))
}

/// TXT properties relevant to OPC UA discovery.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TxtProperties {
    pub application_uri: Option<String>,
    pub path: Option<String>,
}

/// Parse TXT key/value pairs. Keys are matched case-insensitively, ignoring
/// `-`, `_` and whitespace, so `ApplicationUri`, `application-uri` and
/// `APPLICATION_URI` are equivalent.
pub(crate) fn txt_properties(pairs: &[(String, String)]) -> TxtProperties {
    let mut properties = TxtProperties::default();
    for (key, value) in pairs {
        let normalized: String = key
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        let value = value.trim().to_string();
        match normalized.as_str() {
            "applicationuri" | "appuri" => {
                if !value.is_empty() {
                    properties.application_uri = Some(value);
                }
            }
            "path" => {
                if !value.is_empty() {
                    properties.path = Some(value);
                }
            }
            _ => {}
        }
    }
    properties
}

/// Merge duplicates produced by repeated browse/resolve callbacks.
pub(crate) fn dedupe_services(services: Vec<MdnService>) -> Vec<MdnService> {
    let mut out: Vec<MdnService> = Vec::new();
    'next: for mut service in services {
        service.addresses.sort();
        service.addresses.dedup();
        for existing in &mut out {
            let same_instance = !service.instance_name.is_empty()
                && service
                    .instance_name
                    .eq_ignore_ascii_case(&existing.instance_name);
            let same_application = !service.application_uri.is_empty()
                && service
                    .application_uri
                    .eq_ignore_ascii_case(&existing.application_uri)
                && service.port == existing.port;
            if same_instance || same_application {
                existing.addresses.append(&mut service.addresses);
                existing.addresses.sort();
                existing.addresses.dedup();
                if existing.application_uri.is_empty() {
                    existing.application_uri = service.application_uri;
                }
                if existing.host.is_empty() {
                    existing.host = service.host;
                }
                continue 'next;
            }
        }
        out.push(service);
    }
    out
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Instant;
    use windows::Win32::Foundation::{DNS_REQUEST_PENDING, ERROR_SUCCESS};
    use windows::Win32::NetworkManagement::Dns::{
        DNS_QUERY_REQUEST_VERSION1, DNS_RECORDW, DNS_SERVICE_BROWSE_REQUEST,
        DNS_SERVICE_BROWSE_REQUEST_0, DNS_SERVICE_CANCEL, DNS_SERVICE_INSTANCE,
        DNS_SERVICE_RESOLVE_REQUEST, DNS_TYPE_PTR,
    };
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::{PCSTR, PCWSTR, PWSTR};

    type BrowseFn = unsafe extern "system" fn(
        *const DNS_SERVICE_BROWSE_REQUEST,
        *mut DNS_SERVICE_CANCEL,
    ) -> i32;
    type BrowseCancelFn = unsafe extern "system" fn(*const DNS_SERVICE_CANCEL) -> i32;
    type ResolveFn = unsafe extern "system" fn(
        *const DNS_SERVICE_RESOLVE_REQUEST,
        *mut DNS_SERVICE_CANCEL,
    ) -> i32;
    type ResolveCancelFn = unsafe extern "system" fn(*const DNS_SERVICE_CANCEL) -> i32;

    struct DnsApi {
        browse: BrowseFn,
        browse_cancel: BrowseCancelFn,
        resolve: ResolveFn,
        resolve_cancel: ResolveCancelFn,
    }

    /// Resolve the Windows 10 DNS-SD entry points at runtime so an older OS
    /// without these exports degrades to an empty result instead of failing to
    /// load the application.
    fn dns_api() -> Option<&'static DnsApi> {
        static API: OnceLock<Option<DnsApi>> = OnceLock::new();
        API.get_or_init(load_dns_api).as_ref()
    }

    fn load_dns_api() -> Option<DnsApi> {
        let module_name = wide("dnsapi.dll");
        let module = unsafe { LoadLibraryW(PCWSTR(module_name.as_ptr())) }.ok()?;
        let browse = unsafe { GetProcAddress(module, PCSTR(b"DnsServiceBrowse\0".as_ptr())) }?;
        let browse_cancel =
            unsafe { GetProcAddress(module, PCSTR(b"DnsServiceBrowseCancel\0".as_ptr())) }?;
        let resolve = unsafe { GetProcAddress(module, PCSTR(b"DnsServiceResolve\0".as_ptr())) }?;
        let resolve_cancel =
            unsafe { GetProcAddress(module, PCSTR(b"DnsServiceResolveCancel\0".as_ptr())) }?;
        Some(DnsApi {
            browse: unsafe { std::mem::transmute(browse) },
            browse_cancel: unsafe { std::mem::transmute(browse_cancel) },
            resolve: unsafe { std::mem::transmute(resolve) },
            resolve_cancel: unsafe { std::mem::transmute(resolve_cancel) },
        })
    }

    /// Results copied out of an API callback. No pointer escapes the callback.
    #[derive(Debug)]
    enum CallbackEvent {
        BrowseInstance(String),
        BrowseDone(u32),
        Resolve {
            request: usize,
            service: ResolvedService,
        },
        ResolveDone(u32),
    }

    struct Sink {
        tx: Sender<CallbackEvent>,
    }

    static CALLBACKS: OnceLock<Mutex<HashMap<usize, Arc<Sink>>>> = OnceLock::new();
    static NEXT_CALLBACK: AtomicUsize = AtomicUsize::new(1);

    fn callbacks() -> &'static Mutex<HashMap<usize, Arc<Sink>>> {
        CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn register_callback(tx: Sender<CallbackEvent>) -> usize {
        let id = NEXT_CALLBACK.fetch_add(1, Ordering::Relaxed);
        callbacks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, Arc::new(Sink { tx }));
        id
    }

    fn unregister_callback(id: usize) {
        callbacks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }

    fn callback_sink(id: usize) -> Option<Arc<Sink>> {
        callbacks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&id)
            .cloned()
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn wide_ptr_to_string(value: PWSTR) -> String {
        if value.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        let mut cursor = value.0;
        while unsafe { *cursor } != 0 {
            len += 1;
            cursor = unsafe { cursor.add(1) };
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(value.0, len) })
    }

    #[derive(Clone, Debug)]
    struct ResolvedService {
        instance_name: String,
        host: String,
        port: u16,
        addresses: Vec<IpAddr>,
        properties: Vec<(String, String)>,
        interface_index: u32,
    }

    unsafe extern "system" fn browse_callback(
        status: u32,
        context: *const c_void,
        record: *const DNS_RECORDW,
    ) {
        let Some(sink) = callback_sink(context as usize) else {
            return;
        };
        if status != ERROR_SUCCESS.0 {
            let _ = sink.tx.send(CallbackEvent::BrowseDone(status));
            return;
        }
        let mut current = record;
        while !current.is_null() {
            let entry = unsafe { &*current };
            if entry.wType == DNS_TYPE_PTR.0 {
                let name = unsafe { wide_ptr_to_string(entry.Data.PTR.pNameHost) };
                if !name.is_empty() {
                    let _ = sink.tx.send(CallbackEvent::BrowseInstance(name));
                }
            }
            current = entry.pNext;
        }
    }

    unsafe extern "system" fn resolve_callback(
        status: u32,
        context: *const c_void,
        instance: *const DNS_SERVICE_INSTANCE,
    ) {
        let Some(sink) = callback_sink(context as usize) else {
            return;
        };
        if status != ERROR_SUCCESS.0 || instance.is_null() {
            let _ = sink.tx.send(CallbackEvent::ResolveDone(status));
            return;
        }
        let service = unsafe { parse_service_instance(&*instance) };
        let _ = sink.tx.send(CallbackEvent::Resolve {
            request: context as usize,
            service,
        });
    }

    unsafe fn parse_service_instance(instance: &DNS_SERVICE_INSTANCE) -> ResolvedService {
        let instance_name = unsafe { wide_ptr_to_string(instance.pszInstanceName) };
        let host = unsafe { wide_ptr_to_string(instance.pszHostName) };
        let mut addresses = Vec::new();
        if !instance.ip4Address.is_null() {
            let raw = unsafe { *instance.ip4Address };
            addresses.push(IpAddr::V4(Ipv4Addr::from(u32::from_be(raw))));
        }
        if !instance.ip6Address.is_null() {
            let bytes = unsafe { (*instance.ip6Address).IP6Byte };
            addresses.push(IpAddr::V6(Ipv6Addr::from(bytes)));
        }
        let mut properties = Vec::new();
        if !instance.keys.is_null() && !instance.values.is_null() {
            for index in 0..instance.dwPropertyCount as usize {
                let key = unsafe { wide_ptr_to_string(*instance.keys.add(index)) };
                let value = unsafe { wide_ptr_to_string(*instance.values.add(index)) };
                if !key.is_empty() {
                    properties.push((key, value));
                }
            }
        }
        ResolvedService {
            instance_name,
            host,
            port: instance.wPort,
            addresses,
            properties,
            interface_index: instance.dwInterfaceIndex,
        }
    }

    fn browse(interface: u32, deadline: Instant) -> Result<Vec<String>, String> {
        let Some(api) = dns_api() else {
            return Err("Windows DNS-SD API (DnsServiceBrowse) is unavailable".to_string());
        };
        let (tx, rx) = mpsc::channel();
        let id = register_callback(tx);
        let query = wide(OPCUA_SERVICE_TYPE);
        let request = DNS_SERVICE_BROWSE_REQUEST {
            Version: DNS_QUERY_REQUEST_VERSION1.0,
            InterfaceIndex: interface,
            QueryName: PCWSTR(query.as_ptr()),
            Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 {
                pBrowseCallback: Some(browse_callback),
            },
            pQueryContext: id as *mut c_void,
        };
        let mut cancel = DNS_SERVICE_CANCEL::default();
        let started = unsafe { (api.browse)(&request, &mut cancel) };
        if started != DNS_REQUEST_PENDING && started != 0 {
            unregister_callback(id);
            return Err(format!("DnsServiceBrowse failed with {started}"));
        }

        let mut instances = Vec::new();
        let mut seen = HashSet::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(CallbackEvent::BrowseInstance(name)) => {
                    if seen.insert(name.clone()) {
                        instances.push(name);
                    }
                }
                Ok(CallbackEvent::BrowseDone(status)) => {
                    if status != 0 {
                        break;
                    }
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        let cancelled = unsafe { (api.browse_cancel)(&cancel) };
        if cancelled != 0 {
            eprintln!("mDNS browse cancel failed with {cancelled}");
        }
        // Later callbacks only hold this id; they will not find it and return.
        unregister_callback(id);
        Ok(instances)
    }

    struct PendingResolve {
        id: usize,
        cancel: DNS_SERVICE_CANCEL,
        /// Stable heap allocation referenced by the asynchronous request.
        _query: Box<[u16]>,
    }

    fn resolve(
        instances: &[String],
        interface: u32,
        deadline: Instant,
    ) -> Result<Vec<ResolvedService>, String> {
        let Some(api) = dns_api() else {
            return Err("Windows DNS-SD API (DnsServiceResolve) is unavailable".to_string());
        };
        let (tx, rx) = mpsc::channel();
        let mut pending = Vec::new();
        for instance in instances {
            let id = register_callback(tx.clone());
            let mut query: Box<[u16]> = wide(instance).into_boxed_slice();
            let request = DNS_SERVICE_RESOLVE_REQUEST {
                Version: DNS_QUERY_REQUEST_VERSION1.0,
                InterfaceIndex: interface,
                QueryName: PWSTR(query.as_mut_ptr()),
                pResolveCompletionCallback: Some(resolve_callback),
                pQueryContext: id as *mut c_void,
            };
            let mut cancel = DNS_SERVICE_CANCEL::default();
            let started = unsafe { (api.resolve)(&request, &mut cancel) };
            if started == DNS_REQUEST_PENDING || started == 0 {
                pending.push(PendingResolve {
                    id,
                    cancel,
                    _query: query,
                });
            } else {
                unregister_callback(id);
                eprintln!("mDNS resolve start failed for {instance}: {started}");
            }
        }

        let mut resolved = Vec::new();
        let mut completed = HashSet::new();
        while completed.len() < pending.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(CallbackEvent::Resolve { request, service }) => {
                    if completed.insert(request) {
                        resolved.push(service);
                    }
                }
                Ok(CallbackEvent::ResolveDone(status)) => {
                    if status != 0 {
                        eprintln!("mDNS resolve completed with {status}");
                    }
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        for entry in &pending {
            let cancelled = unsafe { (api.resolve_cancel)(&entry.cancel) };
            if cancelled != 0 {
                eprintln!("mDNS resolve cancel failed with {cancelled}");
            }
            unregister_callback(entry.id);
        }
        Ok(resolved)
    }

    fn service_from_resolved(resolved: ResolvedService) -> Option<MdnService> {
        if resolved.port == 0 {
            return None;
        }
        let host = normalize_host(&resolved.host);
        let mut addresses = resolved.addresses;
        addresses.sort_by_key(|address| address.is_ipv6());
        addresses.dedup();
        if host.is_empty() && addresses.is_empty() {
            return None;
        }
        let txt = txt_properties(&resolved.properties);
        let application_name = {
            let label = instance_label(&resolved.instance_name);
            if label.is_empty() {
                host.clone()
            } else {
                label
            }
        };
        Some(MdnService {
            instance_name: resolved.instance_name,
            application_name,
            application_uri: txt.application_uri.unwrap_or_default(),
            host,
            port: resolved.port,
            addresses,
            interface_index: resolved.interface_index,
        })
    }

    pub(super) fn discover(interface: u32) -> Result<Vec<MdnService>, String> {
        let overall_deadline = Instant::now() + MDNS_WINDOW;
        let browse_deadline = overall_deadline
            .checked_sub(MDNS_RESOLVE_RESERVE)
            .unwrap_or(overall_deadline);
        let instances = browse(interface, browse_deadline)?;
        if instances.is_empty() {
            return Ok(Vec::new());
        }
        let resolved = resolve(&instances, interface, overall_deadline)?;
        let services: Vec<MdnService> = resolved
            .into_iter()
            .filter_map(service_from_resolved)
            .collect();
        Ok(dedupe_services(services))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(address: &str, port: u16, app_uri: &str) -> MdnService {
        MdnService {
            instance_name: format!("{address}._opcua-tcp._tcp.local"),
            application_name: address.to_string(),
            application_uri: app_uri.to_string(),
            host: address.to_string(),
            port,
            addresses: vec![address.parse().unwrap()],
            interface_index: 7,
        }
    }

    #[test]
    fn interface_index_uses_the_source_address_family() {
        let v4 = NetworkBinding::new("192.168.1.20".parse().unwrap(), Some(12), Some(24), None);
        assert_eq!(interface_index_for(Some(&v4)), 12);

        let v6 = NetworkBinding::new("fe80::1".parse().unwrap(), Some(12), Some(24), Some(7));
        assert_eq!(interface_index_for(Some(&v6)), 24);

        let v4_without_index =
            NetworkBinding::new("192.168.1.20".parse().unwrap(), None, Some(24), None);
        assert_eq!(interface_index_for(Some(&v4_without_index)), 0);

        let loopback = NetworkBinding::loopback();
        assert_eq!(interface_index_for(Some(&loopback)), 0);
        assert_eq!(interface_index_for(None), 0);
    }

    #[test]
    fn service_instance_label_and_host_are_normalized() {
        assert_eq!(
            instance_label("My Server._opcua-tcp._tcp.local"),
            "My Server"
        );
        assert_eq!(instance_label("plain-name.local"), "plain-name.local");
        assert_eq!(normalize_host("server.local."), "server.local");
        assert_eq!(normalize_host("[::1]"), "::1");
    }

    #[test]
    fn urls_bracket_ipv6_and_keep_hostnames() {
        assert_eq!(
            service_url("::1", 4840).as_deref(),
            Some("opc.tcp://[::1]:4840")
        );
        assert_eq!(
            service_url("fe80::1", 4840).as_deref(),
            Some("opc.tcp://[fe80::1]:4840")
        );
        assert_eq!(
            service_url("10.0.0.5", 4840).as_deref(),
            Some("opc.tcp://10.0.0.5:4840")
        );
        assert_eq!(
            service_url("server.local.", 4840).as_deref(),
            Some("opc.tcp://server.local:4840")
        );
        assert_eq!(service_url("", 4840), None);
        assert_eq!(service_url("10.0.0.5", 0), None);
    }

    #[test]
    fn resolved_service_prefers_ip_urls_and_brackets_ipv6() {
        let mut item = service("10.0.0.5", 4840, "urn:example");
        item.addresses.push("fe80::5".parse::<IpAddr>().unwrap());
        assert_eq!(
            item.discovery_urls(),
            vec![
                "opc.tcp://10.0.0.5:4840".to_string(),
                "opc.tcp://[fe80::5]:4840".to_string()
            ]
        );

        let host_only = MdnService {
            addresses: Vec::new(),
            host: "server.local".to_string(),
            ..item
        };
        assert_eq!(
            host_only.discovery_urls(),
            vec!["opc.tcp://server.local:4840".to_string()]
        );
    }

    #[test]
    fn txt_application_uri_is_case_insensitive_and_optional() {
        let pairs = vec![
            ("txtvers".to_string(), "1".to_string()),
            (
                "ApplicationUri".to_string(),
                "urn:example:server".to_string(),
            ),
            ("path".to_string(), "/opcua".to_string()),
        ];
        let txt = txt_properties(&pairs);
        assert_eq!(txt.application_uri.as_deref(), Some("urn:example:server"));
        assert_eq!(txt.path.as_deref(), Some("/opcua"));

        let empty = txt_properties(&[("application_uri".to_string(), " ".to_string())]);
        assert_eq!(empty.application_uri, None);
    }

    #[test]
    fn duplicate_services_merge_addresses() {
        let mut first = service("10.0.0.5", 4840, "urn:example");
        first.host = "server.local".to_string();
        let mut second = service("10.0.0.6", 4840, "urn:example");
        second.instance_name = "second._opcua-tcp._tcp.local".to_string();
        second.host = "server.local".to_string();
        let merged = dedupe_services(vec![first, second]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].addresses.len(), 2);
        assert_eq!(merged[0].application_uri, "urn:example");
    }
}
