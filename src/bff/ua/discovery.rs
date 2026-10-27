use super::session::pki_dir;
use crate::bff::mdns::{self, MdnService};
use crate::bff::model::{BackendMsg, UaEndpointInfo, UaNetworkInfo, UaServerInfo};
use crate::bff::net::{BoundConnectorBuilder, NetworkBinding, endpoint_url_for};
use crate::constants::{DISCOVERY_GUARD_TIMEOUT, DISCOVERY_URLS, MDNS_WINDOW, UA_SEARCH_TIMEOUT};
use opcua::client::ClientBuilder;
use opcua::types::{ServerOnNetwork, StatusCode};
use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

/// What a discovery step found plus its diagnostics. The notes are journal lines
/// (`EventKind::Log`): a release build has no console, so a step that failed or
/// was skipped must say so here instead of printing to stderr.
pub(super) struct Searched<T> {
    pub found: Vec<T>,
    pub notes: Vec<BackendMsg>,
}

/// Searches for OPC UA servers on `host:port` and returns their descriptions
/// plus endpoint details. Uses `find_servers` on a one-shot connection (which
/// needs no session); if the discovery endpoint does not advertise servers it
/// falls back to the endpoint descriptions of the discovery endpoint itself.
///
/// For a host that is not this machine, `FindServersOnNetwork` is asked at the
/// same `host:port` as well: that is how a remote LDS lists the servers
/// registered with it. Both answers are merged.
pub(super) async fn search_servers(
    host: &str,
    port: u16,
    binding: Option<&NetworkBinding>,
) -> Result<Searched<UaServerInfo>, BackendMsg> {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return Err(BackendMsg::HostRequired);
    }
    if port == 0 {
        return Err(BackendMsg::PortInvalid);
    }

    // Resolve and try ALL addresses, IPv4 first. Many hosts ("localhost")
    // resolve to ::1 first while the server actually listens on 127.0.0.1, so
    // guessing just the first address would miss it.
    let mut addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| BackendMsg::ResolveAddressFailed {
            host: host.to_string(),
            detail: e.to_string(),
        })?
        .collect();
    addresses.sort_by_key(|a| a.is_ipv6());
    addresses.dedup();
    if addresses.is_empty() {
        return Err(BackendMsg::AddressNoIp {
            host: host.to_string(),
        });
    }

    let client = build_discovery_client()?;

    let mut failures = Vec::new();
    for addr in &addresses {
        // IPv6 literals must be bracketed in endpoint URLs.
        let url = endpoint_url_for(addr.ip(), port);
        // A remote host may be a Local Discovery Server: ask it for the servers
        // registered with it while FindServers runs. This machine's own LDS is
        // covered by the network search, so loopback is not asked twice.
        let ask_lds = !addr.ip().is_loopback();
        let (direct, on_network) = tokio::join!(find_servers_at(&client, &url, binding), async {
            if ask_lds {
                Some(find_on_network_at(&client, &url, binding).await)
            } else {
                None
            }
        });
        let reachable = direct.is_ok();
        let mut servers = match direct {
            Ok(servers) => servers,
            Err(e) => {
                failures.push(BackendMsg::AtUrl {
                    url: url.clone(),
                    cause: Box::new(e),
                });
                Vec::new()
            }
        };
        let mut notes = Vec::new();
        match on_network {
            Some(Ok(records)) => {
                for record in &records {
                    merge_server_info(&mut servers, server_from_record(record));
                }
            }
            // An ordinary server answers "service unsupported": expected.
            Some(Err(LdsError::Unsupported)) | None => {}
            // Only worth a line when the server itself was reachable; otherwise
            // the failure above already says it.
            Some(Err(LdsError::Timeout)) if reachable => {
                notes.push(BackendMsg::LdsTimeout { url: url.clone() });
            }
            Some(Err(LdsError::Failed(e))) if reachable => {
                notes.push(BackendMsg::LdsFailed {
                    url: url.clone(),
                    detail: e,
                });
            }
            Some(Err(_)) => {}
        }
        if servers.is_empty() {
            if reachable {
                failures.push(BackendMsg::AtUrl {
                    url: url.clone(),
                    cause: Box::new(BackendMsg::ServersNotFoundShort),
                });
            }
            continue;
        }
        return Ok(Searched {
            found: servers,
            notes,
        });
    }

    Err(BackendMsg::UaNoServers { failures })
}

/// Why a `FindServersOnNetwork` request gave no records.
#[derive(Debug, PartialEq, Eq)]
enum LdsError {
    /// The peer answered, but is not a discovery server (`BadServiceUnsupported`).
    Unsupported,
    /// The peer did not answer within the time budget.
    Timeout,
    /// The request failed (for example nothing listens there); the text says why.
    Failed(String),
}

/// `FindServersOnNetwork` on `url`: the servers a (remote) LDS has registered.
async fn find_on_network_at(
    client: &opcua::client::Client,
    url: &str,
    binding: Option<&NetworkBinding>,
) -> Result<Vec<ServerOnNetwork>, LdsError> {
    let request = if let Some(binding) = binding {
        let connector = BoundConnectorBuilder::new(url, binding.clone());
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.find_servers_on_network(connector, 0, 0, None),
        )
        .await
    } else {
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.find_servers_on_network(url, 0, 0, None),
        )
        .await
    };
    match request {
        Ok(Ok(response)) => Ok(response.servers.unwrap_or_default()),
        Ok(Err(e)) if e.status() == StatusCode::BadServiceUnsupported => Err(LdsError::Unsupported),
        Ok(Err(e)) if e.status() == StatusCode::BadTimeout => Err(LdsError::Timeout),
        Ok(Err(e)) => Err(LdsError::Failed(e.to_string())),
        Err(_) => Err(LdsError::Timeout),
    }
}

/// One `ServerOnNetwork` record as a server entry. The record carries no
/// ApplicationUri, so the URI stays empty (the entry is matched by URL).
fn server_from_record(record: &ServerOnNetwork) -> UaServerInfo {
    let url = record.discovery_url.to_string();
    let url = url.trim();
    let endpoints = if url.to_ascii_lowercase().starts_with("opc.tcp://") {
        vec![UaEndpointInfo {
            url: url.to_string(),
            security_policy: String::new(),
            security_mode: String::new(),
        }]
    } else {
        Vec::new()
    };
    UaServerInfo {
        application_name: record.server_name.to_string(),
        application_uri: String::new(),
        endpoints,
    }
}

/// Merge one server into a list: the same application URI, or failing that a
/// shared endpoint URL, is the same server; endpoints are unioned.
pub(super) fn merge_server_info(servers: &mut Vec<UaServerInfo>, incoming: UaServerInfo) {
    let uri = incoming.application_uri.trim().to_ascii_lowercase();
    let position = servers
        .iter()
        .position(|existing| {
            !uri.is_empty() && existing.application_uri.trim().to_ascii_lowercase() == uri
        })
        .or_else(|| {
            servers.iter().position(|existing| {
                existing
                    .endpoints
                    .iter()
                    .any(|a| incoming.endpoints.iter().any(|b| same_url(&a.url, &b.url)))
            })
        });
    match position.and_then(|i| servers.get_mut(i)) {
        Some(existing) => {
            if existing.application_name.trim().is_empty() {
                existing.application_name = incoming.application_name;
            }
            if existing.application_uri.trim().is_empty() {
                existing.application_uri = incoming.application_uri;
            }
            for endpoint in incoming.endpoints {
                let incoming_is_bare =
                    endpoint.security_policy.is_empty() && endpoint.security_mode.is_empty();
                // A bare URL (an LDS record) adds nothing to the same URL that is
                // already known, with or without security details.
                if incoming_is_bare
                    && existing
                        .endpoints
                        .iter()
                        .any(|e| same_url(&e.url, &endpoint.url))
                {
                    continue;
                }
                // The same URL with security details supersedes the bare URL
                // an LDS record gave.
                if let Some(bare) = existing.endpoints.iter_mut().find(|e| {
                    same_url(&e.url, &endpoint.url)
                        && e.security_policy.is_empty()
                        && e.security_mode.is_empty()
                }) {
                    if !endpoint.security_policy.is_empty() || !endpoint.security_mode.is_empty() {
                        *bare = endpoint;
                    }
                } else if !existing.endpoints.iter().any(|e| {
                    same_url(&e.url, &endpoint.url)
                        && e.security_policy == endpoint.security_policy
                        && e.security_mode == endpoint.security_mode
                }) {
                    existing.endpoints.push(endpoint);
                }
            }
        }
        None => servers.push(incoming),
    }
}

async fn find_servers_at(
    client: &opcua::client::Client,
    url: &str,
    binding: Option<&NetworkBinding>,
) -> Result<Vec<UaServerInfo>, BackendMsg> {
    use opcua::types::ApplicationDescription;

    let request = if let Some(binding) = binding {
        let connector = BoundConnectorBuilder::new(url, binding.clone());
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.find_servers(connector, None, None),
        )
        .await
    } else {
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.find_servers(url, None, None),
        )
        .await
    };
    let descriptions: Vec<ApplicationDescription> = request
        .map_err(|_| BackendMsg::TimeoutSearch)?
        .map_err(|e| BackendMsg::SearchRejected {
            detail: e.to_string(),
        })?;

    // Fetch actual endpoint descriptions for this discovery endpoint so the
    // user gets a usable opc.tcp:// URL even if FindServers returns nothing.
    let mut discovery_uri = String::new();
    let endpoint_request = if let Some(binding) = binding {
        let connector = BoundConnectorBuilder::new(url, binding.clone());
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.get_server_endpoints_from_url(connector),
        )
        .await
    } else {
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.get_server_endpoints_from_url(url),
        )
        .await
    };
    let endpoint_infos: Vec<UaEndpointInfo> = match endpoint_request {
        Ok(Ok(endpoints)) => {
            if let Some(first) = endpoints.first() {
                discovery_uri = first.server.application_uri.to_string();
            }
            endpoints
                .iter()
                .map(|e| UaEndpointInfo {
                    url: e.endpoint_url.to_string(),
                    security_policy: e.security_policy_uri.to_string(),
                    security_mode: format!("{:?}", e.security_mode),
                })
                .collect()
        }
        _ => Vec::new(),
    };

    // Every described application keeps its own advertised DiscoveryUrls. An
    // LDS answers FindServers with *other* servers; pointing those rows at the
    // LDS URL (the old fallback) made a double-click connect to the LDS itself.
    let mut servers: Vec<UaServerInfo> = descriptions
        .iter()
        .map(|d| {
            let mut endpoints: Vec<UaEndpointInfo> = Vec::new();
            for discovery_url in d.discovery_urls.iter().flatten() {
                let discovery_url = discovery_url.to_string();
                let discovery_url = discovery_url.trim();
                // Only opc.tcp is supported by this client (no https/wss).
                if !discovery_url.to_ascii_lowercase().starts_with("opc.tcp://") {
                    continue;
                }
                if !endpoints.iter().any(|e| e.url == discovery_url) {
                    endpoints.push(UaEndpointInfo {
                        url: discovery_url.to_string(),
                        security_policy: String::new(),
                        security_mode: String::new(),
                    });
                }
            }
            UaServerInfo {
                application_name: d.application_name.text.to_string(),
                application_uri: d.application_uri.to_string(),
                endpoints,
            }
        })
        .collect();

    // Attach the fetched endpoint descriptions (with security) to the server
    // that actually answered at `url`, matched by application URI.
    if !endpoint_infos.is_empty() {
        let target = if discovery_uri.is_empty() {
            None
        } else {
            servers
                .iter_mut()
                .find(|s| s.application_uri == discovery_uri)
        };
        if let Some(app) = target {
            app.endpoints.extend(endpoint_infos.clone());
        } else {
            servers.push(UaServerInfo {
                application_name: if discovery_uri.is_empty() {
                    url.to_string()
                } else {
                    discovery_uri.clone()
                },
                application_uri: discovery_uri.clone(),
                endpoints: endpoint_infos.clone(),
            });
        }
    }

    // The server that answered at `url` is always reachable there, even when
    // it advertised no DiscoveryUrls and GetEndpoints failed.
    if let Some(answering) = servers.iter_mut().find(|s| {
        s.endpoints.is_empty() && (discovery_uri.is_empty() || s.application_uri == discovery_uri)
    }) {
        answering.endpoints.push(UaEndpointInfo {
            url: url.to_string(),
            security_policy: String::new(),
            security_mode: String::new(),
        });
    }

    Ok(servers)
}

/// Discovers OPC UA servers advertised via LDS (`FindServersOnNetwork`) and
/// native mDNS (`_opcua-tcp._tcp.local`) concurrently. mDNS is collected in a
/// strict [`MDNS_WINDOW`], LDS keeps its existing guard, and an LDS failure
/// never hides mDNS results.
///
/// Every mDNS URL is verified with `FindServers`/`GetEndpoints` through the
/// same binding before it is reported. A URL that does not answer as OPC UA is
/// dropped; when verification is impossible the result is dropped as well
/// instead of producing a false positive. Whatever failed or was skipped is
/// returned in [`Searched::notes`] for the journal.
pub(super) async fn search_network_servers(
    binding: Option<&NetworkBinding>,
) -> Result<Searched<UaNetworkInfo>, BackendMsg> {
    let mdns_binding = binding.cloned();
    let mdns_task = tokio::task::spawn_blocking(move || mdns::discover(mdns_binding.as_ref()));

    let client = build_discovery_client();
    let lds = async {
        match &client {
            Ok(client) => {
                let search = search_lds_servers(client, binding, &DISCOVERY_URLS);
                match tokio::time::timeout(DISCOVERY_GUARD_TIMEOUT, search).await {
                    Ok(search) => search,
                    Err(_) => LdsSearch {
                        found: Vec::new(),
                        notes: vec![BackendMsg::LdsSearchTimedOut],
                    },
                }
            }
            Err(error) => LdsSearch {
                found: Vec::new(),
                notes: vec![BackendMsg::LdsUnavailable {
                    cause: Box::new(error.clone()),
                }],
            },
        }
    };
    let mdns_result = async {
        // The worker owns a strict MDNS_WINDOW; this is only a hard guard for a
        // wedged blocking task and adds no UI-visible wait.
        match tokio::time::timeout(MDNS_WINDOW + Duration::from_secs(2), mdns_task).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => mdns::MdnsOutcome {
                services: Vec::new(),
                notes: vec![BackendMsg::MdnsCrashed {
                    detail: error.to_string(),
                }],
            },
            Err(_) => mdns::MdnsOutcome {
                services: Vec::new(),
                notes: vec![BackendMsg::MdnsNotInTime],
            },
        }
    };
    let (lds, mdns_outcome) = tokio::join!(lds, mdns_result);
    let mut found = lds.found;
    let mut notes = lds.notes;
    notes.extend(mdns_outcome.notes);

    match &client {
        Ok(client) => {
            let verified = verify_mdns_services(client, &mdns_outcome.services, binding).await;
            notes.extend(verified.notes);
            for info in verified.found {
                merge_network_info(&mut found, info);
            }
        }
        Err(_) if !mdns_outcome.services.is_empty() => {
            notes.push(BackendMsg::MdnsDiscarded);
        }
        Err(_) => {}
    }

    Ok(Searched { found, notes })
}

/// Result of polling the local discovery URLs.
struct LdsSearch {
    found: Vec<UaNetworkInfo>,
    notes: Vec<BackendMsg>,
}

/// True for `opc.tcp://localhost…`, `127.x.x.x` and `[::1]`: spellings of this
/// machine's own discovery endpoint.
fn is_loopback_url(url: &str) -> bool {
    let rest = url
        .trim()
        .trim_start_matches("opc.tcp://")
        .trim_start_matches("OPC.TCP://");
    let host = rest.split(['/', '?']).next().unwrap_or(rest);
    let host = if let Some(stripped) = host.strip_prefix('[') {
        stripped.split(']').next().unwrap_or(stripped)
    } else {
        host.rsplit_once(':').map_or(host, |(name, _)| name)
    };
    let host = host.to_ascii_lowercase();
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

/// LDS discovery over the standard local endpoints. `localhost` and `127.0.0.1`
/// are two names of ONE endpoint: once one of them answered (even "not an LDS" or
/// a timeout) the other is skipped, so the machine's LDS is not polled twice and
/// a silent host does not cost two timeouts. Only an unreachable first name
/// (refused, unresolvable) lets the alias try.
///
/// Errors are diagnostics only; the caller always receives whatever was found.
async fn search_lds_servers(
    client: &opcua::client::Client,
    binding: Option<&NetworkBinding>,
    urls: &[&str],
) -> LdsSearch {
    poll_lds_urls(urls, |url| async move {
        find_on_network_at(client, &url, binding).await
    })
    .await
}

/// The polling policy of [`search_lds_servers`], independent of the transport so
/// it can be tested with a fake `ask`.
async fn poll_lds_urls<F, Fut>(urls: &[&str], mut ask: F) -> LdsSearch
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Vec<ServerOnNetwork>, LdsError>>,
{
    let mut found: Vec<UaNetworkInfo> = Vec::new();
    let mut notes = Vec::new();
    let mut loopback_answered = false;
    let mut answered = false;
    let mut last_failure: Option<BackendMsg> = None;

    for url in urls {
        let loopback = is_loopback_url(url);
        if loopback && loopback_answered {
            continue;
        }
        match ask((*url).to_string()).await {
            Ok(records) => {
                answered = true;
                loopback_answered |= loopback;
                for record in &records {
                    merge_network_info(&mut found, network_info_from_record(record));
                }
            }
            // Reachable, but not a discovery server: nothing to report.
            Err(LdsError::Unsupported) => {
                answered = true;
                loopback_answered |= loopback;
            }
            Err(LdsError::Timeout) => {
                answered = true;
                loopback_answered |= loopback;
                notes.push(BackendMsg::LdsTimeout {
                    url: (*url).to_string(),
                });
            }
            Err(LdsError::Failed(error)) => {
                last_failure = Some(BackendMsg::LdsFailed {
                    url: (*url).to_string(),
                    detail: error,
                });
            }
        }
    }
    // Nothing listening on the standard port is the normal case on a machine
    // without an LDS: one line, not one per spelling.
    if !answered {
        if let Some(failure) = last_failure {
            notes.push(failure);
        }
    }
    LdsSearch { found, notes }
}

/// A `ServerOnNetwork` record as a network-search row. It carries no
/// ApplicationUri; the URL is never used as a fake one (it would keep this row
/// from merging with the same server found by a host search).
fn network_info_from_record(record: &ServerOnNetwork) -> UaNetworkInfo {
    let discovery_url = record.discovery_url.to_string();
    let application_uri = if discovery_url.is_empty() {
        format!("urn:lds-server-{}", record.record_id)
    } else {
        String::new()
    };
    let mut urls = vec![discovery_url];
    urls.retain(|u| !u.is_empty());
    urls.dedup();
    UaNetworkInfo {
        application_name: record.server_name.to_string(),
        application_uri,
        discovery_urls: urls,
    }
}

/// Verify mDNS results through the same binding. Only URLs that answer
/// `FindServers`/`GetEndpoints` become `UaNetworkInfo` entries.
async fn verify_mdns_services(
    client: &opcua::client::Client,
    services: &[MdnService],
    binding: Option<&NetworkBinding>,
) -> Searched<UaNetworkInfo> {
    let deadline = std::time::Instant::now() + DISCOVERY_GUARD_TIMEOUT;
    let mut verified = Vec::new();
    let mut notes = Vec::new();

    for service in services {
        let mut discovery_urls = Vec::new();
        let mut application_uri = service.application_uri.clone();
        let mut application_name = service.application_name.clone();
        let mut responded = false;

        for url in service.discovery_urls() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, find_servers_at(client, &url, binding)).await {
                Ok(Ok(servers)) if !servers.is_empty() => {
                    responded = true;
                    // The URL may belong to an LDS that also lists other
                    // servers: take only the entry for THIS mDNS service
                    // (by URI, else the one reachable at the URL, else the
                    // first) instead of folding every server into one row.
                    let wanted_uri = application_uri.trim().to_ascii_lowercase();
                    let index = servers
                        .iter()
                        .position(|server| {
                            !wanted_uri.is_empty()
                                && server.application_uri.trim().to_ascii_lowercase() == wanted_uri
                        })
                        .or_else(|| {
                            servers.iter().position(|server| {
                                server.endpoints.iter().any(|e| same_url(&e.url, &url))
                            })
                        })
                        .unwrap_or(0);
                    let server = servers.into_iter().nth(index).expect("index in range");
                    push_unique(&mut discovery_urls, url);
                    if application_uri.is_empty() {
                        application_uri = server.application_uri;
                    }
                    if application_name.is_empty() {
                        application_name = server.application_name;
                    }
                    for endpoint in server.endpoints {
                        push_unique(&mut discovery_urls, endpoint.url);
                    }
                    break;
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => notes.push(BackendMsg::MdnsVerifyFailed {
                    url: url.clone(),
                    cause: Box::new(error),
                }),
                Err(_) => {
                    notes.push(BackendMsg::MdnsVerifyTimeout { url: url.clone() });
                    break;
                }
            }
        }

        if !responded {
            continue;
        }
        if application_uri.is_empty() {
            application_uri = service.instance_name.clone();
        }
        if application_name.is_empty() {
            application_name = application_uri.clone();
        }
        verified.push(UaNetworkInfo {
            application_name,
            application_uri,
            discovery_urls,
        });
    }

    Searched {
        found: verified,
        notes,
    }
}

fn normalized_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn same_url(left: &str, right: &str) -> bool {
    normalized_url(left) == normalized_url(right)
}

fn push_unique(list: &mut Vec<String>, value: String) {
    let value = value.trim().to_string();
    if value.is_empty() {
        return;
    }
    if !list.iter().any(|existing| same_url(existing, &value)) {
        list.push(value);
    }
}

/// Merge one discovery result into the aggregate list. Entries are deduplicated
/// by application URI and, when the URI is absent, by discovery URL; endpoints
/// (URLs) are unioned.
pub(super) fn merge_network_info(found: &mut Vec<UaNetworkInfo>, incoming: UaNetworkInfo) {
    let app_key = incoming.application_uri.trim().to_ascii_lowercase();
    let url_keys: Vec<String> = incoming
        .discovery_urls
        .iter()
        .map(|url| normalized_url(url))
        .collect();

    let mut target = None;
    if !app_key.is_empty() {
        target = found.iter().position(|existing| {
            let existing_key = existing.application_uri.trim().to_ascii_lowercase();
            !existing_key.is_empty() && existing_key == app_key
        });
    }
    if target.is_none() {
        target = found.iter().position(|existing| {
            existing
                .discovery_urls
                .iter()
                .any(|url| url_keys.contains(&normalized_url(url)))
        });
    }

    match target {
        Some(index) => {
            let existing = &mut found[index];
            for url in incoming.discovery_urls {
                push_unique(&mut existing.discovery_urls, url);
            }
            if existing.application_name.trim().is_empty() {
                existing.application_name = incoming.application_name;
            }
            if existing.application_uri.trim().is_empty() {
                existing.application_uri = incoming.application_uri;
            }
        }
        None => {
            if !incoming.discovery_urls.is_empty() || !incoming.application_uri.is_empty() {
                found.push(incoming);
            }
        }
    }
}

/// Builds a `Client` for one-shot discovery (`FindServers`/`FindServersOnNetwork`/
/// `GetEndpoints`). Shares the app PKI store so client/server certs are kept in one place.
///
/// The library request deadline is set to [`UA_SEARCH_TIMEOUT`]; the one-shot
/// helpers below wrap the whole call in the larger [`DISCOVERY_GUARD_TIMEOUT`].
/// This ordering matters: on a slow/absent response the library times the
/// request out itself and its `get_endpoints`/`find_servers` code closes the
/// temporary secure channel and drains the event loop before returning, so the
/// channel is not dropped mid-response. The outer guard is only a hard stop for
/// a wedged connect.
pub(super) fn build_discovery_client() -> Result<opcua::client::Client, BackendMsg> {
    let pki = pki_dir();
    ClientBuilder::new()
        .application_name("OPC DA/UA Test Client")
        .application_uri("urn:opcda-egui")
        .product_uri("urn:opcda-egui")
        .create_sample_keypair(true)
        .trust_server_certs(true)
        .pki_dir(pki)
        .session_retry_limit(0)
        .request_timeout(UA_SEARCH_TIMEOUT)
        .client()
        .map_err(|errors| BackendMsg::Raw(errors.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn record(id: u32, name: &str, url: &str) -> ServerOnNetwork {
        ServerOnNetwork {
            record_id: id,
            server_name: name.into(),
            discovery_url: url.into(),
            server_capabilities: None,
        }
    }

    /// Runs `poll_lds_urls` with scripted answers; returns what it asked.
    fn poll(
        urls: &[&str],
        mut answers: Vec<Result<Vec<ServerOnNetwork>, LdsError>>,
    ) -> (LdsSearch, Vec<String>) {
        answers.reverse();
        let answers = Arc::new(Mutex::new(answers));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (answers_in, asked_in) = (Arc::clone(&answers), Arc::clone(&asked));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test runtime");
        let search = runtime.block_on(poll_lds_urls(urls, move |url| {
            asked_in.lock().unwrap().push(url);
            let next = answers_in.lock().unwrap().pop();
            async move { next.expect("an unexpected extra request") }
        }));
        let asked = asked.lock().unwrap().clone();
        (search, asked)
    }

    const IP: &str = "opc.tcp://127.0.0.1:4840";
    const NAME: &str = "opc.tcp://localhost:4840";

    #[test]
    fn loopback_spellings_are_recognised() {
        for url in [
            "opc.tcp://localhost:4840",
            "opc.tcp://LOCALHOST:4840/",
            "opc.tcp://127.0.0.1:4840",
            "opc.tcp://127.1.2.3:4840/path",
            "opc.tcp://[::1]:4840",
        ] {
            assert!(is_loopback_url(url), "{url}");
        }
        for url in [
            "opc.tcp://192.168.1.10:4840",
            "opc.tcp://plc-1:4840",
            "opc.tcp://[fe80::1]:4840",
            "opc.tcp://1127.0.0.1:4840",
        ] {
            assert!(!is_loopback_url(url), "{url}");
        }
    }

    #[test]
    fn an_answering_loopback_lds_is_not_polled_a_second_time() {
        let (search, asked) = poll(
            &[IP, NAME],
            vec![Ok(vec![record(1, "Srv", "opc.tcp://pc:4841")])],
        );
        assert_eq!(
            asked,
            vec![IP.to_string()],
            "localhost is the same endpoint"
        );
        assert_eq!(search.found.len(), 1);
        assert!(search.notes.is_empty());
    }

    #[test]
    fn a_loopback_that_is_not_an_lds_or_times_out_costs_one_request() {
        let (search, asked) = poll(&[IP, NAME], vec![Err(LdsError::Unsupported)]);
        assert_eq!(asked.len(), 1);
        assert!(search.notes.is_empty(), "not an LDS is the expected case");

        let (search, asked) = poll(&[IP, NAME], vec![Err(LdsError::Timeout)]);
        assert_eq!(
            asked.len(),
            1,
            "a second timeout would only double the wait"
        );
        assert_eq!(search.notes.len(), 1);
        assert!(
            search.notes[0].to_string().contains(IP),
            "{:?}",
            search.notes
        );
    }

    #[test]
    fn an_unreachable_first_spelling_lets_the_alias_try() {
        let (search, asked) = poll(
            &[IP, NAME],
            vec![
                Err(LdsError::Failed("refused".into())),
                Ok(vec![record(2, "Srv", "opc.tcp://pc:4841")]),
            ],
        );
        assert_eq!(asked, vec![IP.to_string(), NAME.to_string()]);
        assert_eq!(search.found.len(), 1);
        assert!(
            search.notes.is_empty(),
            "the alias answered: {:?}",
            search.notes
        );
    }

    #[test]
    fn nothing_listening_is_one_journal_line_not_one_per_spelling() {
        let (search, asked) = poll(
            &[IP, NAME],
            vec![
                Err(LdsError::Failed("refused".into())),
                Err(LdsError::Failed("refused".into())),
            ],
        );
        assert_eq!(asked.len(), 2);
        assert!(search.found.is_empty());
        assert_eq!(search.notes.len(), 1, "{:?}", search.notes);
    }

    #[test]
    fn non_loopback_discovery_urls_are_each_asked() {
        let (_, asked) = poll(
            &["opc.tcp://10.0.0.5:4840", IP, NAME],
            vec![Err(LdsError::Unsupported), Err(LdsError::Unsupported)],
        );
        assert_eq!(
            asked,
            vec!["opc.tcp://10.0.0.5:4840".to_string(), IP.to_string()]
        );
    }

    #[test]
    fn the_default_discovery_urls_start_with_the_numeric_loopback() {
        assert_eq!(DISCOVERY_URLS[0], IP);
        assert!(DISCOVERY_URLS.iter().all(|url| is_loopback_url(url)));
    }

    fn endpoint(url: &str, policy: &str, mode: &str) -> UaEndpointInfo {
        UaEndpointInfo {
            url: url.to_string(),
            security_policy: policy.to_string(),
            security_mode: mode.to_string(),
        }
    }

    #[test]
    fn an_lds_record_and_find_servers_describe_one_server() {
        // FindServers: the application with its URI and the full endpoint list.
        let mut servers = vec![UaServerInfo {
            application_name: "Plant".into(),
            application_uri: "urn:plant".into(),
            endpoints: vec![endpoint("opc.tcp://pc:4841", "None", "None")],
        }];
        // FindServersOnNetwork: the same server by URL, no URI, and one more.
        merge_server_info(
            &mut servers,
            server_from_record(&record(1, "Plant", "opc.tcp://PC:4841/")),
        );
        merge_server_info(
            &mut servers,
            server_from_record(&record(2, "Other", "opc.tcp://pc:4842")),
        );
        assert_eq!(servers.len(), 2, "{servers:?}");
        assert_eq!(servers[0].application_uri, "urn:plant");
        assert_eq!(servers[0].endpoints.len(), 1, "no duplicate endpoint");
        assert_eq!(servers[1].application_name, "Other");
    }

    #[test]
    fn security_details_replace_a_bare_lds_url() {
        let mut servers = vec![server_from_record(&record(1, "Plant", "opc.tcp://pc:4841"))];
        merge_server_info(
            &mut servers,
            UaServerInfo {
                application_name: "Plant".into(),
                application_uri: "urn:plant".into(),
                endpoints: vec![endpoint("opc.tcp://pc:4841", "None", "None")],
            },
        );
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].application_uri, "urn:plant");
        assert_eq!(servers[0].endpoints.len(), 1);
        assert_eq!(servers[0].endpoints[0].security_policy, "None");
    }

    #[test]
    fn records_without_a_discovery_url_get_a_synthetic_uri() {
        let info = network_info_from_record(&record(7, "NoUrl", ""));
        assert_eq!(info.application_uri, "urn:lds-server-7");
        assert!(info.discovery_urls.is_empty());
        let info = network_info_from_record(&record(8, "Srv", "opc.tcp://pc:4841"));
        assert!(
            info.application_uri.is_empty(),
            "the URL is never a fake URI"
        );
    }
}
