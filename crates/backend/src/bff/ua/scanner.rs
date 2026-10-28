use super::discovery::build_discovery_client;
use crate::bff::model::{BackendMsg, Event, EventKind, UaEndpointInfo, UaServerInfo};
use crate::bff::net::{BoundConnectorBuilder, NetworkBinding, endpoint_url_for, tcp_probe};
use crate::constants::{
    DEFAULT_UA_PORT, SCAN_ADDRESS_TIMEOUT, SCAN_CONFIRM_THRESHOLD, SCAN_PARALLELISM,
    SCAN_TCP_CONNECT_TIMEOUT, SCAN_UA_HANDSHAKE_TIMEOUT,
};
use crate::net::SubnetSpec;
use opclook_contract::ua_url::UaUrlIdentity;
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tokio::task::{JoinHandle, JoinSet};

/// Error token emitted when the requested subnet is above the configured
/// confirmation cap. The app maps it to a localized message; the scanner never
/// silently truncates the address range.
pub const SCAN_SUBNET_TOO_LARGE: &str = "scan-subnet-too-large";

/// One running subnet scan owned by the UA worker.
pub(super) struct ScanHandle {
    pub(super) cancel: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

pub(super) type ScanRegistry = Arc<Mutex<HashMap<u64, ScanHandle>>>;

/// One address of an N+1 scan plus the binding that must carry its traffic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScanTarget {
    pub(super) ip: IpAddr,
    pub(super) binding: NetworkBinding,
}

/// Build the strictly ordered N+1 target list:
///
/// 1. `127.0.0.1`;
/// 2. `::1` only when the selected local address (or the subnet) is IPv6;
/// 3. the selected adapter's local address, unless it is loopback;
/// 4. every usable host of the selected subnet.
///
/// Duplicates are removed while preserving the first occurrence. No address is
/// ever silently dropped for size; the caller rejects over-large subnets.
pub(super) fn scan_targets(
    binding: &NetworkBinding,
    subnet: Option<SubnetSpec>,
    local_ip: IpAddr,
) -> Vec<ScanTarget> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();

    push_scan_target(
        &mut targets,
        &mut seen,
        IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    );
    let wants_ipv6 = local_ip.is_ipv6()
        || subnet
            .map(|subnet| subnet.network().is_ipv6())
            .unwrap_or(false);
    if wants_ipv6 {
        push_scan_target(
            &mut targets,
            &mut seen,
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        );
    }
    if !local_ip.is_loopback() {
        push_scan_target(&mut targets, &mut seen, local_ip);
    }
    if let Some(subnet) = subnet {
        for ip in subnet.hosts() {
            push_scan_target(&mut targets, &mut seen, ip);
        }
    }
    for target in &mut targets {
        target.binding = if target.ip.is_loopback() {
            NetworkBinding::loopback_for(target.ip)
        } else {
            binding.clone()
        };
    }
    targets
}

fn push_scan_target(targets: &mut Vec<ScanTarget>, seen: &mut HashSet<IpAddr>, ip: IpAddr) {
    if seen.insert(ip) {
        targets.push(ScanTarget {
            ip,
            binding: NetworkBinding::loopback_for(ip),
        });
    }
}

pub(super) fn cancel_scan(scans: &ScanRegistry, scan_id: u64) {
    if let Some(scan) = scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&scan_id)
    {
        scan.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[test]
fn cancellation_survives_poisoned_scan_registry() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let scans: ScanRegistry = Arc::new(Mutex::new(HashMap::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(std::future::pending());
        scans.lock().unwrap().insert(
            42,
            ScanHandle {
                cancel: Arc::clone(&cancel),
                task,
            },
        );
        let copy = Arc::clone(&scans);
        let _ = std::thread::spawn(move || {
            let _guard = copy.lock().unwrap();
            panic!("poison registry");
        })
        .join();
        cancel_scan(&scans, 42);
        assert!(cancel.load(Ordering::Relaxed));
        abort_all_scans(&scans).await;
    });
}

/// Register and spawn one scan without blocking the worker command loop.
pub(super) fn start_scan_task(
    scan_id: u64,
    bind: NetworkBinding,
    subnet: Option<SubnetSpec>,
    local_ip: IpAddr,
    scans: &ScanRegistry,
    tx: &Sender<Event>,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    let task_cancel = Arc::clone(&cancel);
    let registry = Arc::clone(scans);
    let tx = tx.clone();
    let task = tokio::spawn(async move {
        run_scan_job(
            scan_id,
            bind,
            subnet,
            local_ip,
            DEFAULT_UA_PORT,
            task_cancel,
            &tx,
        )
        .await;
        registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&scan_id);
    });
    scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(scan_id, ScanHandle { cancel, task });
}

/// Cancel every running scan and wait for its task to be dropped. Called when
/// the worker's command channel closes so no scan outlives the event channel.
pub(super) async fn abort_all_scans(scans: &ScanRegistry) {
    let handles: Vec<ScanHandle> = match scans.lock() {
        Ok(mut scans) => scans.drain().map(|(_, handle)| handle).collect(),
        Err(poisoned) => poisoned
            .into_inner()
            .drain()
            .map(|(_, handle)| handle)
            .collect(),
    };
    for handle in handles {
        handle.cancel.store(true, Ordering::Relaxed);
        handle.task.abort();
        let _ = handle.task.await;
    }
}

/// Run one N+1 scan. The queue is consumed FIFO with at most
/// [`SCAN_PARALLELISM`] checks in flight; cancellation stops new work and lets
/// the limited in-flight checks finish before the terminal event.
pub(super) async fn run_scan_job(
    scan_id: u64,
    bind: NetworkBinding,
    subnet: Option<SubnetSpec>,
    local_ip: IpAddr,
    port: u16,
    cancel: Arc<AtomicBool>,
    tx: &Sender<Event>,
) {
    if let Some(subnet) = subnet {
        if subnet.usable_count() > SCAN_CONFIRM_THRESHOLD {
            send_scan_finished(
                tx,
                scan_id,
                false,
                0,
                Some(BackendMsg::raw(format!(
                    "{SCAN_SUBNET_TOO_LARGE}:{}",
                    subnet.usable_count()
                ))),
            );
            return;
        }
    }

    let targets = scan_targets(&bind, subnet, local_ip);
    let total = targets.len();
    send_scan_progress(tx, scan_id, total, 0, 0);

    let client = match build_discovery_client() {
        Ok(client) => Arc::new(client),
        Err(message) => {
            send_scan_finished(tx, scan_id, false, 0, Some(message));
            return;
        }
    };

    let mut queue: VecDeque<ScanTarget> = targets.into();
    let mut tasks: JoinSet<(IpAddr, Option<(Vec<UaServerInfo>, bool)>)> = JoinSet::new();
    let mut checked = 0usize;
    let mut found = 0usize;
    let mut seen_servers: HashMap<ScanServerKey, UaServerInfo> = HashMap::new();

    loop {
        let cancelled = cancel.load(Ordering::Relaxed);
        if !cancelled {
            while tasks.len() < SCAN_PARALLELISM {
                let Some(target) = queue.pop_front() else {
                    break;
                };
                let client = Arc::clone(&client);
                tasks.spawn(async move {
                    let result = check_scan_target(&client, target.ip, target.binding, port).await;
                    (target.ip, result)
                });
            }
        }
        if tasks.is_empty() {
            break;
        }

        let Some(joined) = tasks.join_next().await else {
            break;
        };
        let result = match joined {
            Ok((_ip, result)) => {
                checked += 1;
                result
            }
            Err(_) => {
                checked += 1;
                None
            }
        };
        if let Some((batch, invalid_urls)) = result {
            if invalid_urls {
                send_scan_event(
                    tx,
                    EventKind::Log {
                        message: BackendMsg::InvalidDiscoveredUaUrl,
                    },
                );
            }
            let (fresh, new_servers) = merge_scan_batch(&mut seen_servers, batch);
            found += new_servers;
            if !fresh.is_empty() {
                send_scan_event(
                    tx,
                    EventKind::UaScanFound {
                        scan_id,
                        servers: fresh,
                    },
                );
            }
        }
        send_scan_progress(tx, scan_id, total, checked, found);
    }

    send_scan_finished(tx, scan_id, cancel.load(Ordering::Relaxed), found, None);
}

/// TCP precheck followed by a bound UA `GetEndpoints` handshake. An open TCP
/// port that does not answer as OPC UA yields `None`.
async fn check_scan_target(
    client: &opcua::client::Client,
    ip: IpAddr,
    binding: NetworkBinding,
    port: u16,
) -> Option<(Vec<UaServerInfo>, bool)> {
    let address = SocketAddr::new(ip, port);
    let probe = tcp_probe(address, &binding, SCAN_TCP_CONNECT_TIMEOUT);
    let check = async {
        if !probe.await {
            return None;
        }
        let url = endpoint_url_for(ip, port);
        match tokio::time::timeout(
            SCAN_UA_HANDSHAKE_TIMEOUT,
            scan_ua_server_at(client, &url, &binding),
        )
        .await
        {
            Ok(Ok((servers, invalid))) if !servers.is_empty() || invalid => {
                Some((servers, invalid))
            }
            _ => None,
        }
    };
    tokio::time::timeout(SCAN_ADDRESS_TIMEOUT, check)
        .await
        .ok()
        .flatten()
}

/// `GetEndpoints` on one scanner address, normalized into `UaServerInfo`.
async fn scan_ua_server_at(
    client: &opcua::client::Client,
    url: &str,
    binding: &NetworkBinding,
) -> Result<(Vec<UaServerInfo>, bool), String> {
    let connector = BoundConnectorBuilder::new(url, binding.clone());
    let endpoints = client
        .get_server_endpoints_from_url(connector)
        .await
        .map_err(|error| error.to_string())?;
    let Some(first) = endpoints.first() else {
        return Err("the endpoint returned no descriptions".to_string());
    };
    let mut invalid = false;
    let endpoints = endpoints
        .iter()
        .filter_map(|endpoint| {
            let url = match endpoint.endpoint_url.to_string().parse() {
                Ok(url) => url,
                Err(_) => {
                    invalid = true;
                    return None;
                }
            };
            Some(UaEndpointInfo {
                url,
                security_policy: endpoint.security_policy_uri.to_string(),
                security_mode: format!("{:?}", endpoint.security_mode),
            })
        })
        .collect();
    Ok((
        vec![UaServerInfo {
            application_name: first.server.application_name.text.to_string(),
            application_uri: first.server.application_uri.to_string(),
            endpoints,
        }],
        invalid,
    ))
}

/// Merge one scanner result batch into the servers already found by earlier
/// addresses. A server seen through two addresses is not dropped: its endpoints
/// are unioned and only the new endpoint descriptions are emitted as a delta, so
/// the UI can merge them immediately without losing NTBK700582/loopback URLs.
pub(super) fn merge_scan_batch(
    seen: &mut HashMap<ScanServerKey, UaServerInfo>,
    batch: Vec<UaServerInfo>,
) -> (Vec<UaServerInfo>, usize) {
    let mut fresh = Vec::new();
    let mut new_servers = 0;
    for server in batch {
        let key = scan_server_key(&server);
        match seen.get_mut(&key) {
            Some(existing) => {
                let mut added = Vec::new();
                let mut identities: HashSet<_> = existing
                    .endpoints
                    .iter()
                    .map(|owned| owned.url.identity())
                    .collect();
                for endpoint in server.endpoints {
                    let identity = endpoint.url.identity();
                    if identities.insert(identity) {
                        existing.endpoints.push(endpoint.clone());
                        added.push(endpoint);
                    }
                }
                if !added.is_empty() {
                    fresh.push(UaServerInfo {
                        application_name: existing.application_name.clone(),
                        application_uri: existing.application_uri.clone(),
                        endpoints: added,
                    });
                }
            }
            None => {
                new_servers += 1;
                if !server.endpoints.is_empty() {
                    seen.insert(key, server.clone());
                    fresh.push(server);
                } else {
                    seen.insert(key, server);
                }
            }
        }
    }
    (fresh, new_servers)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum ScanServerKey {
    Uri(String),
    Url(UaUrlIdentity),
    Missing,
}

pub(super) fn scan_server_key(server: &UaServerInfo) -> ScanServerKey {
    let uri = server.application_uri.trim().to_ascii_lowercase();
    if !uri.is_empty() {
        return ScanServerKey::Uri(uri);
    }
    server
        .endpoints
        .first()
        .map(|endpoint| ScanServerKey::Url(endpoint.url.identity()))
        .unwrap_or(ScanServerKey::Missing)
}

fn send_scan_event(tx: &Sender<Event>, kind: EventKind) {
    let _ = tx.send(Event {
        request_id: 0,
        session: 0,
        kind,
    });
}

fn send_scan_progress(
    tx: &Sender<Event>,
    scan_id: u64,
    total: usize,
    checked: usize,
    found: usize,
) {
    send_scan_event(
        tx,
        EventKind::UaScanProgress {
            scan_id,
            total,
            checked,
            found,
        },
    );
}

fn send_scan_finished(
    tx: &Sender<Event>,
    scan_id: u64,
    cancelled: bool,
    found: usize,
    error: Option<BackendMsg>,
) {
    send_scan_event(
        tx,
        EventKind::UaScanFinished {
            scan_id,
            cancelled,
            found,
            error,
        },
    );
}
