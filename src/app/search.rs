use super::*;
use crate::bff::model::{CommandKind, LdsTarget, Protocol};
use crate::bff::net::NetworkBinding;
use crate::constants::{DEFAULT_UA_HOST, DEFAULT_UA_PORT};
use crate::i18n::{Tr, fmt, tr};
use crate::net::{NetworkLease, SubnetSpec};

impl App {
    pub fn discover_da(&mut self, host: String) {
        self.dialogs.connect.da_pending = true;
        self.dialogs.connect.error = None;
        let host_suffix = if host.is_empty() {
            String::new()
        } else {
            fmt(self.lang(), Tr::ServerHostSuffix, &[&host])
        };
        self.journal.info_l(Tr::ReqServerList, &[&host_suffix]);
        if let Err(error) = self.send_da_prepared(
            0,
            CommandKind::Discover { host: host.clone() },
            PendingCtx::Host(host.clone()),
        ) {
            let message = error.localized(self.lang());
            let d = &mut self.dialogs.connect;
            d.da_auto_pending = d.da_auto_pending.saturating_sub(1);
            if d.da_auto_pending == 0 {
                d.da_pending = false;
            }
            d.error = Some(message.clone());
            self.journal.error(message);
        }
    }

    /// Start a direct DA host search, invalidating all previous popup
    /// discovery requests. Used by the Search button and host-field Enter.
    pub fn restart_da_discovery(&mut self, host: String) {
        self.clear_pending_requests(0);
        self.release_da_discovery_routes();
        let d = &mut self.dialogs.connect;
        d.da_pending = false;
        d.da_auto_pending = 0;
        d.da_servers.clear();
        d.da_server_hosts.clear();
        d.da_selected = None;
        d.da_selected_host = None;
        self.discover_da(host);
    }

    /// Combined DA discovery: this computer (empty host) plus every known remote
    /// DA host from settings/history (best effort). Results are merged as the
    /// per-host `Servers` events arrive.
    pub fn discover_da_all(&mut self) {
        // A new user search owns a new set of request IDs and result rows.
        // Late replies from the previous search may not overwrite them.
        self.clear_pending_requests(0);
        self.release_da_discovery_routes();
        // Without an explicitly selected adapter only this computer is
        // discovered; remote search/connect is an explicit error instead of a
        // silent fallback to the default route.
        let has_adapter = self.network.lease.is_some();
        let mut hosts: Vec<String> = if has_adapter {
            self.settings
                .recent_endpoints
                .iter()
                .filter(|e| e.protocol == Protocol::OpcDa)
                .map(|e| e.host.trim().to_string())
                .filter(|h| !h.is_empty() && !h.eq_ignore_ascii_case("localhost") && h != ".")
                .collect()
        } else {
            Vec::new()
        };
        hosts.sort();
        hosts.dedup();
        self.dialogs.connect.da_auto_pending = 1 + hosts.len();
        self.dialogs.connect.da_servers.clear();
        self.dialogs.connect.da_server_hosts.clear();
        self.dialogs.connect.da_selected = None;
        self.dialogs.connect.da_selected_host = None;
        self.discover_da(String::new());
        for host in hosts {
            self.discover_da(host);
        }
    }

    /// Search for OPC UA servers on the dialog's host:port fields. An empty
    /// host means "this computer + local network" -> a combined search that
    /// merges mDNS/LDS and the local UA discovery endpoints.
    pub fn search_ua_servers(&mut self) {
        let host = self.dialogs.connect.ua_search_host.trim().to_string();
        if host.is_empty() {
            self.search_ua_all();
            return;
        }
        let port: u16 = match self.dialogs.connect.ua_search_port.trim().parse() {
            Ok(p) if p > 0 => p,
            _ => DEFAULT_UA_PORT, // empty or invalid port -> the UA default
        };
        let Some(bind) = self.search_binding(&host) else {
            self.dialogs.connect.error =
                Some(tr(self.lang(), Tr::SelectAdapterRequired).to_string());
            self.dialogs.connect.ua_searching = false;
            return;
        };
        self.dialogs.connect.ua_searching = true;
        self.dialogs.connect.ua_search_results.clear();
        self.dialogs.connect.ua_result_epoch = self.dialogs.connect.ua_result_epoch.wrapping_add(1);
        self.dialogs.connect.error = None;
        self.journal
            .info_l(Tr::SearchingUaHost, &[&host, &port.to_string()]);
        self.send(
            0,
            Protocol::OpcUa,
            CommandKind::SearchUaServers {
                host,
                port,
                bind: Some(bind),
            },
        );
    }

    /// Combined "empty host" UA search: `FindServersOnNetwork` (mDNS/LDS) plus
    /// ONE `FindServers` on this computer's loopback endpoint (`127.0.0.1` and
    /// `localhost` are the same endpoint, asking both only doubles the wait).
    /// Partial results accumulate and are merged when all sub-searches
    /// complete.
    pub fn search_ua_all(&mut self) {
        let Ok(lds) = self.lds_target_from_dialog() else {
            self.dialogs.connect.ua_searching = false;
            self.dialogs.connect.ua_network_searching = false;
            return;
        };
        let bind = self.current_network_binding();
        // An explicitly named LDS is asked even when no adapter is chosen.
        let network = bind.is_some() || lds.is_some();
        let d = &mut self.dialogs.connect;
        d.ua_searching = true;
        d.ua_network_searching = network;
        d.error = None;
        d.ua_search_results.clear();
        d.ua_result_epoch = d.ua_result_epoch.wrapping_add(1);
        d.ua_network_results.clear();
        d.ua_discovered_endpoints.clear();
        let commands = ua_all_commands(bind, lds);
        d.ua_auto_pending = commands.len();
        for kind in commands {
            self.send(0, Protocol::OpcUa, kind);
        }
    }

    /// The dialog's optional LDS host as a search target. `Ok(None)` when the
    /// field is empty. An unusable text or a missing adapter (for a non-local
    /// host) leaves a localized error in the dialog and returns `Err`. The
    /// text is remembered in the settings.
    fn lds_target_from_dialog(&mut self) -> Result<Option<LdsTarget>, ()> {
        let text = self.dialogs.connect.ua_lds_host.trim().to_string();
        let parsed = match parse_lds_host(&text) {
            Ok(parsed) => parsed,
            Err(()) => {
                self.dialogs.connect.error = Some(tr(self.lang(), Tr::LdsHostInvalid).to_string());
                return Err(());
            }
        };
        let Some((host, port)) = parsed else {
            self.settings.set_ua_lds_host("");
            return Ok(None);
        };
        let Some(bind) = self.search_binding(&host) else {
            self.dialogs.connect.error =
                Some(tr(self.lang(), Tr::SelectAdapterRequired).to_string());
            return Err(());
        };
        self.settings.set_ua_lds_host(&text);
        Ok(Some(LdsTarget { host, port, bind }))
    }

    /// Fold one completed sub-search into the combined UA auto-search. Called
    /// from the event handlers while `ua_auto_pending > 0`.
    pub(super) fn ua_auto_step_done(&mut self, update: impl FnOnce(&mut ConnectDialog)) {
        update(&mut self.dialogs.connect);
        let d = &mut self.dialogs.connect;
        d.ua_auto_pending = d.ua_auto_pending.saturating_sub(1);
        if d.ua_auto_pending == 0 {
            d.ua_searching = false;
            d.ua_network_searching = false;
            merge_ua_results(d);
        }
    }

    /// Request `GetEndpoints` discovery for the current dialog URL so the
    /// security combos can be populated before connecting. Returns false if the
    /// URL is not a usable opc.tcp:// endpoint.
    pub fn discover_ua_endpoints(&mut self) -> bool {
        let lang = self.lang();
        let url = self.dialogs.connect.ua_url.trim().to_string();
        if url.parse::<crate::bff::model::UaUrl>().is_err() {
            self.dialogs.connect.error = Some(tr(lang, Tr::HintUaUrl).to_string());
            return false;
        }
        self.dialogs.connect.error = None;
        // The worker resolves endpoints during Connect; here we just clear the
        // stale combo list so the dialog reflects that discovery is pending
        // (it is refreshed from the search results / a connect attempt).
        self.dialogs.connect.ua_discovered_endpoints.clear();
        true
    }

    /// Validate the dialog's typed policy/mode selection.
    ///
    /// The pair is checked first (e.g. policy None + Sign is invalid), then —
    /// when the dialog already holds discovered endpoints — resolved against
    /// them so an unadvertised explicit choice is rejected before a connection
    /// is attempted. Returns true when the selection is usable; on failure the
    /// localized error is left in the dialog.
    pub fn validate_ua_security(&mut self) -> bool {
        let lang = self.lang();
        let d = &mut self.dialogs.connect;
        if let Err(err) =
            crate::bff::model::validate_security_pair(d.ua_security_policy, d.ua_security_mode)
        {
            d.error = Some(fmt(
                lang,
                Tr::SecurityInvalidCombination,
                &[&err.to_string()],
            ));
            return false;
        }
        // Search results may contain different servers. Validate only the
        // selected URL, never a different server's security capabilities.
        let selected: Vec<_> = d
            .ua_discovered_endpoints
            .iter()
            .filter(|endpoint| endpoint.url == d.ua_url.trim())
            .cloned()
            .collect();
        if !selected.is_empty() {
            if let Err(detail) = crate::bff::ua::resolve_security(
                d.ua_security_policy,
                d.ua_security_mode,
                &selected,
            ) {
                d.error = Some(fmt(lang, Tr::SecurityResolveFailed, &[&detail]));
                return false;
            }
        }
        d.error = None;
        true
    }

    /// Validate the identity selector. Anonymous and X509 ignore the
    /// username/password fields; UserName requires a non-empty user name.
    pub fn validate_ua_identity(&mut self) -> bool {
        let lang = self.lang();
        let d = &mut self.dialogs.connect;
        if d.ua_identity_index == 1 && d.ua_user.trim().is_empty() {
            d.error = Some(tr(lang, Tr::UaUserNameRequired).to_string());
            false
        } else {
            d.error = None;
            true
        }
    }

    /// Search for OPC UA servers advertized via mDNS/LDS (`FindServersOnNetwork`).
    /// With an "LDS host" in the dialog that host's LDS is asked (and mDNS when
    /// an adapter is chosen); without one, this computer's own LDS and mDNS.
    pub fn search_ua_network(&mut self) {
        let Ok(lds) = self.lds_target_from_dialog() else {
            self.dialogs.connect.ua_network_searching = false;
            return;
        };
        let bind = self.current_network_binding();
        if bind.is_none() && lds.is_none() {
            let message = tr(self.lang(), Tr::SelectAdapterRequired).to_string();
            self.dialogs.connect.ua_network_searching = false;
            self.dialogs.connect.error = Some(message);
            return;
        }
        self.dialogs.connect.ua_network_searching = true;
        self.dialogs.connect.ua_result_epoch = self.dialogs.connect.ua_result_epoch.wrapping_add(1);
        self.dialogs.connect.error = None;
        match &lds {
            Some(target) => {
                let place = format!("{}:{}", target.host, target.port);
                self.journal.info_l(Tr::SearchingUaLds, &[&place]);
            }
            None => self.journal.info_l(Tr::SearchingUaNetwork, &[]),
        }
        self.send(
            0,
            Protocol::OpcUa,
            CommandKind::SearchUaNetwork { bind, lds },
        );
    }

    /// Start the N+1 subnet scan for the current lease. An over-large subnet is
    /// not launched: the dialog shows a confirmation prompt instead.
    pub fn start_ua_subnet_scan(&mut self) {
        let lang = self.lang();
        // A new scan supersedes a running one; its events are ignored once the
        // new scan id replaces the old one.
        if self.dialogs.connect.ua_subnet_searching || self.dialogs.connect.ua_scan_id.is_some() {
            self.cancel_ua_subnet_scan();
        }
        let Some(lease) = self.network.lease.clone() else {
            self.dialogs.connect.ua_scan_confirm = None;
            self.dialogs.connect.error = Some(tr(lang, Tr::SelectAdapterRequired).to_string());
            return;
        };
        // A loopback-only lease scans just the loopback addresses; its /8
        // subnet must not turn into millions of targets.
        let subnet = if lease.local_ip().is_loopback() {
            None
        } else {
            Some(lease.subnet)
        };
        if let Some(subnet) = subnet {
            if subnet.needs_confirmation() {
                let count = subnet.usable_count();
                let d = &mut self.dialogs.connect;
                d.ua_subnet_searching = false;
                d.ua_scan_id = None;
                d.ua_scan_confirm = Some(count);
                d.ua_scan_total = 0;
                d.ua_scan_checked = 0;
                d.ua_scan_found = 0;
                d.error = None;
                d.ua_scan_error = Some(fmt(
                    lang,
                    Tr::UaScanConfirmText,
                    &[
                        &count.to_string(),
                        &crate::constants::SCAN_CONFIRM_THRESHOLD.to_string(),
                    ],
                ));
                return;
            }
        }
        self.launch_ua_subnet_scan(&lease, subnet);
    }

    /// Confirm an over-large subnet. The backend never scans above the cap, so
    /// confirmation explicitly narrows the run to loopback + the local address
    /// (the prompt says exactly that).
    pub fn confirm_ua_subnet_scan(&mut self) {
        let Some(_) = self.dialogs.connect.ua_scan_confirm.take() else {
            return;
        };
        let lang = self.lang();
        let Some(lease) = self.network.lease.clone() else {
            self.dialogs.connect.error = Some(tr(lang, Tr::SelectAdapterRequired).to_string());
            return;
        };
        self.launch_ua_subnet_scan(&lease, None);
    }

    fn launch_ua_subnet_scan(&mut self, lease: &NetworkLease, subnet: Option<SubnetSpec>) {
        let scan_id = self.next_scan;
        self.next_scan += 1;
        let binding = lease.network_binding();
        let local_ip = lease.local_ip();
        let d = &mut self.dialogs.connect;
        d.ua_scan_id = Some(scan_id);
        d.ua_subnet_searching = true;
        d.ua_scan_checked = 0;
        d.ua_scan_total = 0;
        d.ua_scan_found = 0;
        d.ua_scan_error = None;
        d.ua_scan_confirm = None;
        d.ua_scan_servers.clear();
        d.ua_search_results.clear();
        d.ua_result_epoch = d.ua_result_epoch.wrapping_add(1);
        d.ua_network_results.clear();
        d.ua_discovered_endpoints.clear();
        d.ua_auto_pending = 0;
        d.error = None;
        self.journal.info_l(Tr::SearchingUaSubnet, &[]);
        self.send(
            0,
            Protocol::OpcUa,
            CommandKind::ScanUaNetwork {
                scan_id,
                bind: binding,
                subnet,
                local_ip,
            },
        );
    }

    /// Stop the running subnet scan, keeping the partial results already
    /// received and dropping the spinner immediately.
    pub fn cancel_ua_subnet_scan(&mut self) {
        let pending_confirm = self.dialogs.connect.ua_scan_confirm.take().is_some();
        let scan_id = self.dialogs.connect.ua_scan_id.take();
        self.dialogs.connect.ua_subnet_searching = false;
        merge_ua_scan_servers(&mut self.dialogs.connect);
        if pending_confirm {
            self.dialogs.connect.ua_scan_error = None;
        }
        if let Some(scan_id) = scan_id {
            self.send(0, Protocol::OpcUa, CommandKind::CancelUaScan { scan_id });
        }
    }
}

/// Merge the running scan's buffered servers into the dialog result list and
/// rebuild the security combo source.
pub(super) fn merge_ua_scan_servers(dialog: &mut ConnectDialog) {
    for server in std::mem::take(&mut dialog.ua_scan_servers) {
        merge_ua_server(dialog, server);
    }
    refresh_ua_combos(dialog);
}

/// Deduplicate one scan result by application URI and, when the URI is missing
/// or distinct, by endpoint URL. Endpoints of a duplicate are unioned.
fn merge_ua_server(dialog: &mut ConnectDialog, server: crate::bff::model::UaServerInfo) {
    let app_uri = server.application_uri.trim();
    let mut existing = if app_uri.is_empty() {
        None
    } else {
        dialog
            .ua_search_results
            .iter()
            .position(|s| s.application_uri.trim().eq_ignore_ascii_case(app_uri))
    };
    if existing.is_none() {
        // URL fallback only when one side has no URI: two servers with
        // different application URIs are different servers even if a stale
        // discovery record points both at the same URL.
        existing = dialog.ua_search_results.iter().position(|s| {
            (app_uri.is_empty() || s.application_uri.trim().is_empty())
                && s.endpoints.iter().any(|existing| {
                    server.endpoints.iter().any(|incoming| {
                        normalized_ua_url(&existing.url) == normalized_ua_url(&incoming.url)
                    })
                })
        });
    }
    match existing {
        Some(index) => {
            let existing = &mut dialog.ua_search_results[index];
            if existing.application_name.trim().is_empty() {
                existing.application_name = server.application_name.clone();
            }
            if existing.application_uri.trim().is_empty() {
                existing.application_uri = server.application_uri.clone();
            }
            for endpoint in server.endpoints {
                if !existing
                    .endpoints
                    .iter()
                    .any(|owned| same_ua_endpoint(owned, &endpoint))
                {
                    existing.endpoints.push(endpoint);
                }
            }
        }
        None => dialog.ua_search_results.push(server),
    }
}

fn normalized_ua_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Same URL can advertise several distinct security configurations.
fn same_ua_endpoint(
    left: &crate::bff::model::UaEndpointInfo,
    right: &crate::bff::model::UaEndpointInfo,
) -> bool {
    normalized_ua_url(&left.url) == normalized_ua_url(&right.url)
        && left.security_policy == right.security_policy
        && left.security_mode == right.security_mode
}

/// Merge a batch of per-host results into the visible list (dedup by
/// application URI, then by endpoint URL; endpoints are unioned).
pub(super) fn merge_ua_servers(
    dialog: &mut ConnectDialog,
    servers: Vec<crate::bff::model::UaServerInfo>,
) {
    for server in servers {
        merge_ua_server(dialog, server);
    }
    refresh_ua_combos(dialog);
}

/// Fold the mDNS/LDS network results into the (already merged) per-host
/// results of a combined UA auto-search.
///
/// Network entries go through the same merge as every other source, so a
/// server already found by a host search still gains the discovery URLs that
/// only LDS/mDNS reported (e.g. its LAN address). An empty application URI
/// stays empty (dedup then falls back to the URL); the display name is never
/// stored as a URI.
pub(super) fn merge_ua_results(dialog: &mut ConnectDialog) {
    use crate::bff::model::UaServerInfo;
    let mut merged: Vec<UaServerInfo> = std::mem::take(&mut dialog.ua_search_results);
    for network in std::mem::take(&mut dialog.ua_network_results) {
        let endpoints = network
            .discovery_urls
            .iter()
            .filter(|url| !url.trim().is_empty())
            .map(|url| crate::bff::model::UaEndpointInfo {
                url: url.clone(),
                security_policy: String::new(),
                security_mode: String::new(),
            })
            .collect::<Vec<_>>();
        if network.application_uri.trim().is_empty() && endpoints.is_empty() {
            continue;
        }
        merged.push(UaServerInfo {
            application_name: network.application_name,
            application_uri: network.application_uri,
            endpoints,
        });
    }
    // Use the same merge mechanism as streaming scan results. This keeps
    // (URL, security policy, security mode) alternatives intact.
    dialog.ua_search_results.clear();
    for server in merged {
        merge_ua_server(dialog, server);
    }
    dialog.ua_network_results.clear();
    dialog.ua_auto_pending = 0;
    refresh_ua_combos(dialog);
}

/// Rebuild the security-combo endpoint list from the current UA search results,
/// deduplicated by (url, policy, mode).
pub(super) fn refresh_ua_combos(dialog: &mut ConnectDialog) {
    let mut seen = std::collections::HashSet::new();
    let mut eps = Vec::new();
    for server in &dialog.ua_search_results {
        for e in &server.endpoints {
            let key = (
                e.url.clone(),
                e.security_policy.clone(),
                e.security_mode.clone(),
            );
            if seen.insert(key) {
                eps.push(e.clone());
            }
        }
    }
    dialog.ua_discovered_endpoints = eps;
}

/// Parse the LDS host field: empty -> `Ok(None)`; otherwise `host[:port]` (an
/// IPv6 literal in brackets, an optional `opc.tcp://` prefix and trailing `/`
/// are tolerated). The port defaults to 4840.
pub(crate) fn parse_lds_host(text: &str) -> Result<Option<(String, u16)>, ()> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    if rest
        .get(..10)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("opc.tcp://"))
    {
        rest = &rest[10..];
    }
    let rest = rest.trim_end_matches('/');
    let parse_port = |port: &str| -> Result<u16, ()> {
        match port.parse::<u16>() {
            Ok(port) if port > 0 => Ok(port),
            _ => Err(()),
        }
    };
    let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
        let (host, tail) = inner.split_once(']').ok_or(())?;
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(());
        }
        match tail.strip_prefix(':') {
            Some(port) => (host.to_string(), parse_port(port)?),
            None if tail.is_empty() => (host.to_string(), DEFAULT_UA_PORT),
            None => return Err(()),
        }
    } else if rest.parse::<std::net::Ipv6Addr>().is_ok() {
        (rest.to_string(), DEFAULT_UA_PORT)
    } else if let Some((host, port)) = rest.rsplit_once(':') {
        (host.to_string(), parse_port(port)?)
    } else {
        (rest.to_string(), DEFAULT_UA_PORT)
    };
    let valid_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'));
    if valid_host {
        Ok(Some((host, port)))
    } else {
        Err(())
    }
}

/// The requests of the combined "empty host" UA search: LDS/mDNS when an
/// adapter is chosen or an LDS host is named, and always exactly ONE
/// `FindServers` on the loopback endpoint (`127.0.0.1`; `localhost` is the same
/// endpoint under another name).
pub(crate) fn ua_all_commands(
    bind: Option<NetworkBinding>,
    lds: Option<LdsTarget>,
) -> Vec<CommandKind> {
    let mut commands = Vec::new();
    if bind.is_some() || lds.is_some() {
        commands.push(CommandKind::SearchUaNetwork { bind, lds });
    }
    commands.push(CommandKind::SearchUaServers {
        host: DEFAULT_UA_HOST.to_string(),
        port: DEFAULT_UA_PORT,
        bind: Some(NetworkBinding::loopback()),
    });
    commands
}
