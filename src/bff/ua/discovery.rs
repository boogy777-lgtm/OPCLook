use super::session::pki_dir;
use crate::bff::mdns::{self, MdnService};
use crate::bff::model::{UaEndpointInfo, UaNetworkInfo, UaServerInfo};
use crate::bff::net::{BoundConnectorBuilder, NetworkBinding, endpoint_url_for};
use crate::constants::{DISCOVERY_GUARD_TIMEOUT, DISCOVERY_URLS, MDNS_WINDOW, UA_SEARCH_TIMEOUT};
use opcua::client::ClientBuilder;
use std::net::SocketAddr;
use std::time::Duration;

/// Searches for OPC UA servers on `host:port` and returns their descriptions
/// plus endpoint details. Uses `find_servers` on a one-shot connection (which
/// needs no session); if the discovery endpoint does not advertise servers it
/// falls back to the endpoint descriptions of the discovery endpoint itself.
pub(super) async fn search_servers(
    host: &str,
    port: u16,
    binding: Option<&NetworkBinding>,
) -> Result<Vec<UaServerInfo>, String> {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return Err("укажите адрес сервера (хост)".to_string());
    }
    if port == 0 {
        return Err("укажите корректный порт".to_string());
    }

    // Resolve and try ALL addresses, IPv4 first. Many hosts ("localhost")
    // resolve to ::1 first while the server actually listens on 127.0.0.1, so
    // guessing just the first address would miss it.
    let mut addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("не удалось разрешить адрес \"{host}\": {e}"))?
        .collect();
    addresses.sort_by_key(|a| a.is_ipv6());
    addresses.dedup();
    if addresses.is_empty() {
        return Err(format!("адрес \"{host}\" не дал ни одного IP-адреса"));
    }

    let client = build_discovery_client()?;

    let mut failures = Vec::new();
    for addr in &addresses {
        // IPv6 literals must be bracketed in endpoint URLs.
        let url = endpoint_url_for(addr.ip(), port);
        match find_servers_at(&client, &url, binding).await {
            Ok(servers) if !servers.is_empty() => return Ok(servers),
            Ok(_) => failures.push(format!("{url}: серверы не найдены")),
            Err(e) => failures.push(format!("{url}: {e}")),
        }
    }

    let detail = if failures.is_empty() {
        "OPC UA серверы не найдены".to_string()
    } else {
        format!("OPC UA серверы не найдены: {}", failures.join("; "))
    };
    Err(detail)
}

async fn find_servers_at(
    client: &opcua::client::Client,
    url: &str,
    binding: Option<&NetworkBinding>,
) -> Result<Vec<UaServerInfo>, String> {
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
        .map_err(|_| "таймаут поиска серверов".to_string())?
        .map_err(|e| format!("запрос поиска отклонён: {e}"))?;

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
/// instead of producing a false positive.
pub(super) async fn search_network_servers(
    binding: Option<&NetworkBinding>,
) -> Result<Vec<UaNetworkInfo>, String> {
    let mdns_binding = binding.cloned();
    let mdns_task = tokio::task::spawn_blocking(move || mdns::discover(mdns_binding.as_ref()));

    let client = build_discovery_client();
    let lds = async {
        match &client {
            Ok(client) => {
                let search = search_lds_servers(client, binding);
                match tokio::time::timeout(DISCOVERY_GUARD_TIMEOUT, search).await {
                    Ok(found) => found,
                    Err(_) => {
                        eprintln!("LDS search timed out");
                        Vec::new()
                    }
                }
            }
            Err(error) => {
                eprintln!("LDS search unavailable: {error}");
                Vec::new()
            }
        }
    };
    let mdns_result = async {
        // The worker owns a strict MDNS_WINDOW; this is only a hard guard for a
        // wedged blocking task and adds no UI-visible wait.
        match tokio::time::timeout(MDNS_WINDOW + Duration::from_secs(2), mdns_task).await {
            Ok(Ok(services)) => services,
            Ok(Err(error)) => {
                eprintln!("mDNS worker failed: {error}");
                Vec::new()
            }
            Err(_) => {
                eprintln!("mDNS worker timed out");
                Vec::new()
            }
        }
    };
    let (mut found, mdns_services) = tokio::join!(lds, mdns_result);

    match &client {
        Ok(client) => {
            let verified = verify_mdns_services(client, &mdns_services, binding).await;
            for info in verified {
                merge_network_info(&mut found, info);
            }
        }
        Err(_) if !mdns_services.is_empty() => {
            eprintln!("mDNS results discarded: no OPC UA discovery client for verification");
        }
        Err(_) => {}
    }

    Ok(found)
}

/// LDS discovery over the standard local endpoints. Errors are diagnostics
/// only; the caller always receives whatever was found.
async fn search_lds_servers(
    client: &opcua::client::Client,
    binding: Option<&NetworkBinding>,
) -> Vec<UaNetworkInfo> {
    let mut found: Vec<UaNetworkInfo> = Vec::new();

    for url in DISCOVERY_URLS {
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
        let result = request.map(|inner| {
            inner
                .map(|response| response.servers.unwrap_or_default())
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(Ok(servers)) => {
                for record in servers {
                    let discovery_url = record.discovery_url.to_string();
                    // ServerOnNetwork carries no ApplicationUri. Never use the
                    // URL as a fake URI: it would keep this row from merging
                    // with the same server found by a host search (by URL).
                    let app_uri = if discovery_url.is_empty() {
                        format!("urn:lds-server-{}", record.record_id)
                    } else {
                        String::new()
                    };
                    let mut urls = vec![discovery_url];
                    urls.retain(|u| !u.is_empty());
                    urls.dedup();
                    merge_network_info(
                        &mut found,
                        UaNetworkInfo {
                            application_name: record.server_name.to_string(),
                            application_uri: app_uri,
                            discovery_urls: urls,
                        },
                    );
                }
            }
            Ok(Err(error)) => eprintln!("LDS {url}: {error}"),
            Err(_) => eprintln!("LDS {url}: timeout"),
        }
    }

    found
}

/// Verify mDNS results through the same binding. Only URLs that answer
/// `FindServers`/`GetEndpoints` become `UaNetworkInfo` entries.
async fn verify_mdns_services(
    client: &opcua::client::Client,
    services: &[MdnService],
    binding: Option<&NetworkBinding>,
) -> Vec<UaNetworkInfo> {
    let deadline = std::time::Instant::now() + DISCOVERY_GUARD_TIMEOUT;
    let mut verified = Vec::new();

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
                Ok(Err(error)) => eprintln!("mDNS verify {url}: {error}"),
                Err(_) => {
                    eprintln!("mDNS verify {url}: timeout");
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

    verified
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
pub(super) fn build_discovery_client() -> Result<opcua::client::Client, String> {
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
        .map_err(|errors| errors.join("; "))
}
