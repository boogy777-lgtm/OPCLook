//! CLSID resolution policy of the OPC DA backend (D1): platform-neutral.
//!
//! The WinAPI calls (remote OPCEnum, local registry) plug in through
//! [`ClsidLookup`]; a GUID travels as `u128` (`GUID::to_u128`/`from_u128`).
#![cfg_attr(not(windows), allow(dead_code))]

use super::model::BackendMsg;
use std::collections::HashMap;

/// Whether `host` designates this computer (empty, `localhost`, `.`).
pub fn is_local_host(host: &str) -> bool {
    let host = host.trim();
    host.is_empty() || host.eq_ignore_ascii_case("localhost") || host == "."
}

/// The sources a CLSID can come from. A GUID is carried as `u128`
/// (`GUID::to_u128`/`from_u128`) so that this module stays WinAPI-free.
pub trait ClsidLookup {
    /// `IOPCServerList::CLSIDFromProgID` on the OPCEnum of the remote `host`.
    fn remote_enum(&mut self, host: &str, prog_id: &str) -> Result<u128, BackendMsg>;
    /// A CLSID remembered from an earlier discovery of this host.
    fn discovered(&self, host: &str, prog_id: &str) -> Option<u128>;
    /// `CLSIDFromProgID` against the local registry.
    fn local_registry(&mut self, prog_id: &str) -> Result<u128, BackendMsg>;
}

/// Resolve the CLSID of `prog_id` on `host`.
///
/// Local host: the local registry. Remote host: the ProgID is usually not
/// registered on this machine, so the remote OPCEnum is asked first, then the
/// CLSID seen during discovery, and the local registry is the last resort (a
/// server whose registration was copied to this machine).
pub fn resolve_clsid<L: ClsidLookup>(
    host: &str,
    prog_id: &str,
    lookup: &mut L,
) -> Result<u128, BackendMsg> {
    if is_local_host(host) {
        return lookup.local_registry(prog_id);
    }
    let host = host.trim();
    let remote_error = match lookup.remote_enum(host, prog_id) {
        Ok(clsid) => return Ok(clsid),
        Err(e) => e,
    };
    if let Some(clsid) = lookup.discovered(host, prog_id) {
        return Ok(clsid);
    }
    match lookup.local_registry(prog_id) {
        Ok(clsid) => Ok(clsid),
        Err(local_error) => Err(BackendMsg::DaClsidResolveFailed {
            prog_id: prog_id.to_string(),
            host: host.to_string(),
            remote: Box::new(remote_error),
            local: Box::new(local_error),
        }),
    }
}

/// CLSIDs seen while enumerating servers, keyed by (host, ProgID). The model
/// carries no CLSID, so the connect step recovers it from here.
#[derive(Default)]
pub struct DiscoveredClsids {
    map: HashMap<(String, String), u128>,
}

impl DiscoveredClsids {
    fn key(host: &str, prog_id: &str) -> (String, String) {
        let host = if is_local_host(host) {
            String::new()
        } else {
            host.trim().to_lowercase()
        };
        (host, prog_id.trim().to_lowercase())
    }

    pub fn remember(&mut self, host: &str, prog_id: &str, clsid: u128) {
        self.map.insert(Self::key(host, prog_id), clsid);
    }

    pub fn get(&self, host: &str, prog_id: &str) -> Option<u128> {
        self.map.get(&Self::key(host, prog_id)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- D1 ---------------------------------------------------------------

    #[derive(Default)]
    struct FakeLookup {
        remote: Option<Result<u128, BackendMsg>>,
        discovered: Option<u128>,
        local: Option<Result<u128, BackendMsg>>,
        calls: Vec<&'static str>,
    }

    impl ClsidLookup for FakeLookup {
        fn remote_enum(&mut self, _host: &str, _prog_id: &str) -> Result<u128, BackendMsg> {
            self.calls.push("remote");
            self.remote
                .clone()
                .unwrap_or_else(|| Err(BackendMsg::raw("no remote")))
        }
        fn discovered(&self, _host: &str, _prog_id: &str) -> Option<u128> {
            self.discovered
        }
        fn local_registry(&mut self, _prog_id: &str) -> Result<u128, BackendMsg> {
            self.calls.push("local");
            self.local
                .clone()
                .unwrap_or_else(|| Err(BackendMsg::raw("no local")))
        }
    }

    #[test]
    fn remote_server_clsid_comes_from_the_remote_opcenum_not_the_local_registry() {
        // Before D1 only the local registry was consulted, so a ProgID that is
        // not registered on this machine could not be connected to.
        let mut lookup = FakeLookup {
            remote: Some(Ok(0xAA)),
            local: Some(Err(BackendMsg::raw("not registered"))),
            ..FakeLookup::default()
        };
        assert_eq!(
            resolve_clsid("plc-1", "Vendor.Server.1", &mut lookup),
            Ok(0xAA)
        );
        assert_eq!(lookup.calls, vec!["remote"]);
    }

    #[test]
    fn remote_falls_back_to_discovery_then_local_registry() {
        let mut lookup = FakeLookup {
            discovered: Some(0xBB),
            local: Some(Ok(0xCC)),
            ..FakeLookup::default()
        };
        assert_eq!(resolve_clsid("plc-1", "S", &mut lookup), Ok(0xBB));

        let mut lookup = FakeLookup {
            local: Some(Ok(0xCC)),
            ..FakeLookup::default()
        };
        assert_eq!(resolve_clsid("plc-1", "S", &mut lookup), Ok(0xCC));
        assert_eq!(lookup.calls, vec!["remote", "local"]);
    }

    #[test]
    fn remote_failure_reports_both_causes() {
        let mut lookup = FakeLookup {
            remote: Some(Err(BackendMsg::raw("RPC unavailable"))),
            local: Some(Err(BackendMsg::raw("not registered"))),
            ..FakeLookup::default()
        };
        let err = resolve_clsid("plc-1", "S", &mut lookup)
            .unwrap_err()
            .to_string();
        assert!(err.contains("RPC unavailable"), "{err}");
        assert!(err.contains("not registered"), "{err}");
        assert!(err.contains("plc-1"), "{err}");
    }

    #[test]
    fn local_hosts_use_only_the_local_registry() {
        for host in ["", " ", "localhost", "LOCALHOST", ".", " . "] {
            let mut lookup = FakeLookup {
                remote: Some(Ok(1)),
                local: Some(Ok(2)),
                ..FakeLookup::default()
            };
            assert_eq!(
                resolve_clsid(host, "S", &mut lookup),
                Ok(2),
                "host {host:?}"
            );
            assert_eq!(lookup.calls, vec!["local"]);
            assert!(is_local_host(host));
        }
        assert!(!is_local_host("plc-1"));
        assert!(!is_local_host("127.0.0.1"));
    }

    #[test]
    fn discovered_clsids_are_keyed_case_insensitively_and_local_hosts_merge() {
        let mut cache = DiscoveredClsids::default();
        cache.remember("PLC-1", "Vendor.Server.1", 7);
        assert_eq!(cache.get(" plc-1 ", "vendor.server.1"), Some(7));
        assert_eq!(cache.get("plc-2", "vendor.server.1"), None);
        cache.remember("localhost", "Local.Server", 9);
        assert_eq!(cache.get("", "local.server"), Some(9));
    }
}
