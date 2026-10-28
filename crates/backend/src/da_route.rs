//! DA route steering through the privileged `opcda-nethelper`.
//!
//! OPC DA/DCOM has no API to pin a COM connection to an adapter, so a temporary
//! host route (`/32` or `/128`) through the selected interface is used instead.
//! This module owns:
//!
//! - pure route planning/classification (testable without elevation),
//! - the app-side helper client (named pipe + UAC launch),
//! - `(IP, adapter)` refcounting and route lifetime.
//!
//! The helper only accepts host routes; the application never edits the system
//! routing table itself.

#![deny(clippy::undocumented_unsafe_blocks)]

use crate::constants::{DA_HELPER_EXE_ENV, DA_HELPER_START_TIMEOUT, DA_ROUTE_LIFETIME_SECS};
use crate::net::{AdapterId, AdapterInfo, SubnetSpec};
pub use opclook_contract::da_route::{DaRouteError, DaRouteStatus, RouteDecision, RoutePlan};
use opclook_contract::helper_ipc::{PROTOCOL_VERSION, Request, Route};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Classify the existing route state.
///
/// `best_luid` is what `GetBestRoute2` reports for the target. `host_luid` is
/// the interface of an exact `/32` or `/128` route for the same target, when
/// one exists. A default route through another adapter is *not* a conflict; a
/// host-specific route through another adapter is.
pub fn route_decision(
    selected_luid: u64,
    best_luid: Option<u64>,
    host_luid: Option<u64>,
) -> RouteDecision {
    if let Some(host) = host_luid {
        if host == selected_luid {
            return RouteDecision::Reuse;
        }
        return RouteDecision::Conflict;
    }
    if best_luid == Some(selected_luid) {
        return RouteDecision::Reuse;
    }
    RouteDecision::Add
}

fn unspecified_for(target: IpAddr) -> IpAddr {
    if target.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    }
}

/// Plan a host route through `adapter`. On-link targets use the unspecified
/// next hop; off-link targets require a same-family gateway on the adapter.
pub fn plan_route(target: IpAddr, adapter: &AdapterInfo) -> Result<RoutePlan, DaRouteError> {
    validate_target(target)?;
    if adapter.luid == 0 {
        return Err(DaRouteError::AdapterUnavailable);
    }
    let prefix = host_prefix(target);
    let on_link = adapter.addresses.iter().any(|address| {
        address.ip.is_ipv4() == target.is_ipv4()
            && SubnetSpec::new(address.ip, address.prefix)
                .map(|subnet| subnet.contains(target))
                .unwrap_or(false)
    });
    if on_link {
        return Ok(RoutePlan {
            target,
            prefix,
            interface_luid: adapter.luid,
            next_hop: unspecified_for(target),
            via_gateway: false,
        });
    }
    let gateway = adapter
        .gateways
        .iter()
        .copied()
        .find(|gateway| gateway.is_ipv4() == target.is_ipv4());
    let Some(gateway) = gateway else {
        return Err(DaRouteError::NoGateway(target));
    };
    Ok(RoutePlan {
        target,
        prefix,
        interface_luid: adapter.luid,
        next_hop: gateway,
        via_gateway: true,
    })
}

fn validate_target(target: IpAddr) -> Result<(), DaRouteError> {
    if target.is_loopback() || target.is_unspecified() || is_multicast_or_broadcast(target) {
        return Err(DaRouteError::InvalidTarget(target));
    }
    Ok(())
}

fn host_prefix(target: IpAddr) -> u8 {
    if target.is_ipv4() { 32 } else { 128 }
}

/// A lease for a route that already exists through the selected adapter. It is
/// never added/removed, so it deliberately does not need a gateway or a
/// reachable next hop; a later gateway change must not invalidate the lease.
fn reuse_plan(target: IpAddr, adapter: &AdapterInfo) -> RoutePlan {
    RoutePlan {
        target,
        prefix: host_prefix(target),
        interface_luid: adapter.luid,
        next_hop: unspecified_for(target),
        via_gateway: false,
    }
}

fn is_multicast_or_broadcast(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_multicast() || ip == Ipv4Addr::BROADCAST,
        IpAddr::V6(ip) => ip.is_multicast(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteKey {
    pub target: IpAddr,
    pub adapter: AdapterId,
}

#[derive(Clone, Debug)]
struct LeaseEntry {
    plan: RoutePlan,
    count: usize,
    owned: bool,
    identity: Arc<()>,
}

/// Pure refcount table keyed by `(IP, adapter)`.
#[derive(Clone, Debug, Default)]
pub struct RouteLeases {
    entries: HashMap<RouteKey, LeaseEntry>,
}

impl RouteLeases {
    pub fn count(&self, key: &RouteKey) -> usize {
        self.entries.get(key).map(|entry| entry.count).unwrap_or(0)
    }

    /// The plan already recorded for a live lease. Re-acquiring must reuse this
    /// snapshot instead of re-planning against a changed gateway/route table.
    fn plan(&self, key: &RouteKey) -> Option<RoutePlan> {
        self.entries.get(key).map(|entry| entry.plan)
    }

    fn acquire(&mut self, plan: RoutePlan, adapter: AdapterId, owned: bool) -> Arc<()> {
        let key = RouteKey {
            target: plan.target,
            adapter,
        };
        let entry = self.entries.entry(key).or_insert(LeaseEntry {
            plan,
            count: 0,
            owned,
            identity: Arc::new(()),
        });
        entry.count += 1;
        if owned {
            entry.owned = true;
            entry.plan = plan;
        }
        Arc::clone(&entry.identity)
    }

    /// Drop one reference. Returns `Some((plan, owned))` only when the last
    /// reference is gone, so the caller knows whether the helper route must be
    /// removed.
    fn release(&mut self, key: &RouteKey, identity: &Arc<()>) -> Option<(RoutePlan, bool)> {
        let entry = self.entries.get_mut(key)?;
        if !Arc::ptr_eq(&entry.identity, identity) {
            return None;
        }
        entry.count = entry.count.saturating_sub(1);
        if entry.count > 0 {
            return None;
        }
        let entry = self.entries.remove(key)?;
        Some((entry.plan, entry.owned))
    }

    fn owned_plans(&self) -> Vec<RoutePlan> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.owned)
            .map(|(_, entry)| entry.plan)
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct PreparedRoutes {
    pub targets: Vec<IpAddr>,
    keys: Vec<(RouteKey, Arc<()>)>,
}

impl PreparedRoutes {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Keeps the launched process object alive until the pipe peer is verified,
/// preventing the operating system from recycling its process ID meanwhile.
#[derive(Debug)]
struct LaunchedProcess {
    #[cfg(windows)]
    _handle: Option<crate::winutil::OwnedHandle>,
    pid: u32,
}

struct HelperLaunch {
    rx: std::sync::mpsc::Receiver<Result<LaunchedProcess, HelperLaunchError>>,
    pipe: String,
}

enum HelperState {
    Idle,
    Launching(HelperLaunch),
    Launched {
        pipe: String,
        process: LaunchedProcess,
        last_error: Option<HelperIpcError>,
    },
    Connected(HelperClient),
    Failed,
}

#[derive(Debug)]
enum HelperLaunchError {
    #[cfg_attr(not(windows), allow(dead_code))]
    Declined,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
enum HelperIpcError {
    #[error("helper I/O: {0}")]
    Io(String),
    #[error("helper protocol: {0}")]
    Protocol(String),
    #[error("helper rejected request: {0}")]
    Rejected(String),
    #[error("helper pipe closed")]
    Closed,
    #[error("helper exchange timed out")]
    TimedOut,
    #[error("helper message exceeds limit: {0}")]
    Oversize(&'static str),
    #[error("unexpected helper pipe server: expected {expected}, received {actual}")]
    PeerMismatch { expected: u32, actual: u32 },
}

#[derive(Debug, Serialize, Deserialize)]
struct PipeReply {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

#[cfg(any(windows, test))]
async fn pipe_exchange<S>(
    stream: &mut S,
    message: &Request,
    timeout: Duration,
) -> Result<(), HelperIpcError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    const LIMIT: usize = 16 * 1024;
    let mut request =
        serde_json::to_vec(message).map_err(|e| HelperIpcError::Protocol(e.to_string()))?;
    if request.len() >= LIMIT {
        return Err(HelperIpcError::Oversize("request"));
    }
    request.push(b'\n');
    tokio::time::timeout(timeout, async {
        stream
            .write_all(&request)
            .await
            .map_err(|e| HelperIpcError::Io(format!("pipe write failed: {e}")))?;
        let mut line = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let n = stream
                .read(&mut byte)
                .await
                .map_err(|e| HelperIpcError::Io(format!("pipe read failed: {e}")))?;
            if n == 0 {
                return Err(HelperIpcError::Closed);
            }
            if byte[0] == b'\n' {
                break;
            }
            if line.len() >= LIMIT {
                return Err(HelperIpcError::Oversize("reply"));
            }
            line.push(byte[0]);
        }
        let reply: PipeReply = serde_json::from_slice(&line)
            .map_err(|e| HelperIpcError::Protocol(format!("invalid helper reply: {e}")))?;
        if reply.ok {
            Ok(())
        } else {
            Err(HelperIpcError::Rejected(
                reply.error.unwrap_or_else(|| "no detail".into()),
            ))
        }
    })
    .await
    .map_err(|_| HelperIpcError::TimedOut)?
}

#[cfg(windows)]
struct HelperClient {
    transport: Option<HelperTransport>,
}

#[cfg(windows)]
struct HelperTransport {
    // Drop the pipe before shutting down the runtime so cancelled overlapped
    // operations are completed and their handles released during shutdown.
    pipe: tokio::net::windows::named_pipe::NamedPipeClient,
    runtime: tokio::runtime::Runtime,
}
#[cfg(not(windows))]
struct HelperClient;

impl HelperClient {
    fn call(&mut self, message: Request) -> Result<(), HelperIpcError> {
        #[cfg(windows)]
        {
            let Some(transport) = self.transport.as_mut() else {
                return Err(HelperIpcError::Closed);
            };
            let result = transport.runtime.block_on(pipe_exchange(
                &mut transport.pipe,
                &message,
                Duration::from_secs(3),
            ));
            if result.is_err() {
                // Dropping both the pipe and its runtime drains cancelled I/O;
                // keeping the runtime alive can retain the pipe's OS handle.
                self.transport = None;
            }
            result
        }
        #[cfg(not(windows))]
        {
            let _ = message;
            Err(HelperIpcError::Io(
                "named pipes are only supported on Windows".into(),
            ))
        }
    }

    fn add(&mut self, plan: RoutePlan) -> Result<(), HelperIpcError> {
        self.call_with_op("add", plan)
    }

    fn refresh(&mut self, plan: RoutePlan) -> Result<(), HelperIpcError> {
        self.call_with_op("refresh", plan)
    }

    fn call_with_op(&mut self, op: &str, plan: RoutePlan) -> Result<(), HelperIpcError> {
        let route = Route {
            target: plan.target,
            prefix: plan.prefix,
            interface_luid: plan.interface_luid,
            next_hop: plan.next_hop,
        };
        let message = if op == "add" {
            Request::Add {
                version: PROTOCOL_VERSION,
                route,
                lifetime_secs: DA_ROUTE_LIFETIME_SECS,
            }
        } else {
            Request::Refresh {
                version: PROTOCOL_VERSION,
                route,
                lifetime_secs: DA_ROUTE_LIFETIME_SECS,
            }
        };
        self.call(message)
    }

    fn remove(&mut self, plan: RoutePlan) -> Result<(), HelperIpcError> {
        self.call(Request::Remove {
            version: PROTOCOL_VERSION,
            route: Route {
                target: plan.target,
                prefix: plan.prefix,
                interface_luid: plan.interface_luid,
                next_hop: plan.next_hop,
            },
        })
    }
}

pub struct DaRouteManager {
    helper_state: HelperState,
    leases: RouteLeases,
    last_error: Option<DaRouteError>,
}

impl Default for DaRouteManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DaRouteManager {
    pub fn new() -> Self {
        Self {
            helper_state: HelperState::Idle,
            leases: RouteLeases::default(),
            last_error: None,
        }
    }

    pub fn status(&self) -> DaRouteStatus {
        match self.helper_state {
            HelperState::Idle => DaRouteStatus::Idle,
            HelperState::Launching(_) | HelperState::Launched { .. } => {
                DaRouteStatus::HelperStarting
            }
            HelperState::Connected(_) => DaRouteStatus::HelperRunning,
            HelperState::Failed => DaRouteStatus::HelperUnavailable,
        }
    }

    pub fn last_error(&self) -> Option<&DaRouteError> {
        self.last_error.as_ref()
    }

    /// Resolve the DA host, skip local/loopback targets, and prepare every
    /// remote address. The route table is only touched when no route through
    /// the selected adapter already works.
    pub fn prepare_host(
        &mut self,
        host: &str,
        adapter: Option<&AdapterInfo>,
        local_addresses: &[IpAddr],
    ) -> Result<PreparedRoutes, DaRouteError> {
        let targets = resolve_targets(host)?;
        self.prepare_resolved(targets, adapter, local_addresses, || false)
    }

    pub(crate) fn prepare_resolved(
        &mut self,
        targets: Vec<IpAddr>,
        adapter: Option<&AdapterInfo>,
        local_addresses: &[IpAddr],
        cancelled: impl Fn() -> bool,
    ) -> Result<PreparedRoutes, DaRouteError> {
        let remote: Vec<IpAddr> = targets
            .into_iter()
            .filter(|ip| !ip.is_loopback() && !local_addresses.iter().any(|local| local == ip))
            .collect();
        if remote.is_empty() {
            return Ok(PreparedRoutes::default());
        }
        let adapter = adapter.ok_or(DaRouteError::NoAdapter)?;
        let mut prepared = PreparedRoutes::default();
        for target in remote {
            if cancelled() {
                self.release_prepared(&mut prepared);
                return Err(DaRouteError::Resolve(
                    "route preparation cancelled or timed out".into(),
                ));
            }
            if prepared.targets.contains(&target) {
                continue;
            }
            let key = RouteKey {
                target,
                adapter: adapter.id,
            };
            if self.leases.count(&key) > 0 {
                let Some(plan) = self.leases.plan(&key) else {
                    continue;
                };
                let identity = self.leases.acquire(plan, adapter.id, false);
                prepared.targets.push(target);
                prepared.keys.push((key, identity));
                continue;
            }
            if let Err(error) = validate_target(target) {
                self.release_prepared(&mut prepared);
                self.last_error = Some(error.clone());
                return Err(error);
            }
            let best = best_route_luid(target);
            let host_route = host_route_luid(target);
            match route_decision(adapter.luid, best, host_route) {
                RouteDecision::Reuse => {
                    let plan = reuse_plan(target, adapter);
                    let identity = self.leases.acquire(plan, adapter.id, false);
                    prepared.targets.push(target);
                    prepared.keys.push((key, identity));
                }
                RouteDecision::Conflict => {
                    self.release_prepared(&mut prepared);
                    self.last_error = Some(DaRouteError::Conflict(target));
                    return Err(DaRouteError::Conflict(target));
                }
                RouteDecision::Add => {
                    let plan = match plan_route(target, adapter) {
                        Ok(plan) => plan,
                        Err(error) => {
                            self.release_prepared(&mut prepared);
                            self.last_error = Some(error.clone());
                            return Err(error);
                        }
                    };
                    if let Err(error) = self.ensure_helper() {
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    }
                    if cancelled() {
                        self.release_prepared(&mut prepared);
                        return Err(DaRouteError::Resolve(
                            "route preparation cancelled or timed out".into(),
                        ));
                    }
                    let Some(helper) = self.helper_mut() else {
                        let error = DaRouteError::HelperUnavailable("no helper client".to_string());
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    };
                    if let Err(detail) = helper.add(plan) {
                        self.invalidate_helper();
                        let error = DaRouteError::Add(target, detail.to_string());
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    }
                    if cancelled() || best_route_luid(target) != Some(adapter.luid) {
                        let error = if let Some(Err(detail)) =
                            self.helper_mut().map(|helper| helper.remove(plan))
                        {
                            self.helper_failure(detail.clone());
                            DaRouteError::HelperUnavailable(detail.to_string())
                        } else {
                            DaRouteError::Verify(target)
                        };
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    }
                    let identity = self.leases.acquire(plan, adapter.id, true);
                    prepared.targets.push(target);
                    prepared.keys.push((key, identity));
                }
            }
        }
        self.last_error = None;
        Ok(prepared)
    }

    /// Refresh the finite lifetime of every helper-owned route. Called
    /// periodically by the app; a missing helper is not an error (the route
    /// simply expires unless a new operation re-adds it).
    pub fn refresh(&mut self) {
        if !matches!(self.helper_state, HelperState::Connected(_)) {
            return;
        }
        let plans = self.leases.owned_plans();
        for plan in plans {
            if let Some(Err(error)) = self.helper_mut().map(|helper| helper.refresh(plan)) {
                self.helper_failure(error);
                break;
            }
        }
    }

    pub fn has_owned_routes(&self) -> bool {
        !self.leases.owned_plans().is_empty()
    }

    fn invalidate_helper(&mut self) {
        self.helper_state = HelperState::Failed;
        self.leases.entries.retain(|_, entry| !entry.owned);
    }

    fn helper_failure(&mut self, error: HelperIpcError) {
        self.invalidate_helper();
        self.last_error = Some(DaRouteError::HelperUnavailable(error.to_string()));
    }

    pub(crate) fn lease_is_valid(&self, prepared: &PreparedRoutes) -> bool {
        prepared.keys.iter().all(|(key, identity)| {
            self.leases
                .entries
                .get(key)
                .is_some_and(|entry| Arc::ptr_eq(&entry.identity, identity))
        })
    }

    pub fn release(&mut self, prepared: PreparedRoutes) {
        let mut prepared = prepared;
        self.release_prepared(&mut prepared);
    }

    fn release_prepared(&mut self, prepared: &mut PreparedRoutes) {
        for (key, identity) in prepared.keys.drain(..) {
            if let Some((plan, owned)) = self.leases.release(&key, &identity) {
                if owned {
                    if let Some(Err(error)) = self.helper_mut().map(|helper| helper.remove(plan)) {
                        self.helper_failure(error);
                    }
                }
            }
        }
        prepared.targets.clear();
    }

    fn helper_mut(&mut self) -> Option<&mut HelperClient> {
        match &mut self.helper_state {
            HelperState::Connected(client) => Some(client),
            _ => None,
        }
    }

    fn ensure_helper(&mut self) -> Result<(), DaRouteError> {
        self.ensure_helper_with(spawn_helper)
    }

    fn ensure_helper_with(
        &mut self,
        spawn: impl FnOnce() -> Result<HelperLaunch, DaRouteError>,
    ) -> Result<(), DaRouteError> {
        if matches!(self.helper_state, HelperState::Connected(_)) {
            return Ok(());
        }
        if matches!(self.helper_state, HelperState::Idle | HelperState::Failed) {
            self.helper_state = match spawn() {
                Ok(launch) => HelperState::Launching(launch),
                Err(error) => {
                    self.helper_state = HelperState::Failed;
                    self.last_error = Some(error.clone());
                    return Err(error);
                }
            };
        }
        let deadline = Instant::now() + DA_HELPER_START_TIMEOUT;
        loop {
            let state = std::mem::replace(&mut self.helper_state, HelperState::Idle);
            self.helper_state = match state {
                HelperState::Launching(launch) => match launch.rx.try_recv() {
                    Ok(Ok(process)) => HelperState::Launched {
                        pipe: launch.pipe,
                        process,
                        last_error: None,
                    },
                    Err(std::sync::mpsc::TryRecvError::Empty) => HelperState::Launching(launch),
                    Ok(Err(HelperLaunchError::Declined)) => {
                        self.helper_state = HelperState::Failed;
                        return Err(DaRouteError::UacDeclined);
                    }
                    Ok(Err(HelperLaunchError::Failed(error))) => {
                        self.helper_state = HelperState::Failed;
                        return Err(DaRouteError::HelperUnavailable(error));
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.helper_state = HelperState::Failed;
                        return Err(DaRouteError::HelperUnavailable(
                            "helper launcher exited".into(),
                        ));
                    }
                },
                HelperState::Launched { pipe, process, .. } => {
                    match HelperClient::connect(&pipe, process.pid, Duration::from_millis(250)) {
                        Ok(client) => HelperState::Connected(client),
                        Err(error @ HelperIpcError::Io(_)) => HelperState::Launched {
                            pipe,
                            process,
                            last_error: Some(error),
                        },
                        Err(error) => {
                            self.helper_state = HelperState::Failed;
                            return Err(DaRouteError::HelperUnavailable(error.to_string()));
                        }
                    }
                }
                other => other,
            };
            if matches!(self.helper_state, HelperState::Connected(_)) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(match &self.helper_state {
                    HelperState::Launched {
                        last_error: Some(error),
                        ..
                    } => DaRouteError::HelperUnavailable(error.to_string()),
                    _ => DaRouteError::HelperStarting,
                });
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for DaRouteManager {
    fn drop(&mut self) {
        // Closing the authenticated pipe makes the helper remove its routes.
        // Avoid a separate, potentially slow round trip for each lease.
        self.helper_state = HelperState::Idle;
    }
}

pub(crate) fn resolve_targets(host: &str) -> Result<Vec<IpAddr>, DaRouteError> {
    let host = host.trim();
    if host.is_empty() {
        return Ok(Vec::new());
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }
    let mut resolved = (host, 135u16)
        .to_socket_addrs()
        .map_err(|_| DaRouteError::Resolve(host.to_string()))?
        .map(|address| address.ip())
        .collect::<Vec<_>>();
    resolved.sort();
    resolved.dedup();
    if resolved.is_empty() {
        return Err(DaRouteError::Resolve(host.to_string()));
    }
    Ok(resolved)
}

#[cfg(windows)]
fn random_bytes() -> Result<[u8; 16], DaRouteError> {
    use windows::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let mut out = [0u8; 16];
    // SAFETY: the buffer is valid for its full length; the system-preferred
    // provider requires no algorithm handle.
    let status = unsafe { BCryptGenRandom(None, &mut out, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if !status.is_ok() {
        return Err(DaRouteError::HelperUnavailable(format!(
            "BCryptGenRandom failed: {status:?}"
        )));
    }
    Ok(out)
}

#[cfg(not(windows))]
fn random_bytes() -> Result<[u8; 16], DaRouteError> {
    Err(DaRouteError::HelperUnavailable(
        "secure helper launch is Windows-only".to_string(),
    ))
}

fn random_hex(bytes: [u8; 16]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn helper_exe() -> Result<PathBuf, DaRouteError> {
    if let Some(path) = std::env::var_os(DA_HELPER_EXE_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(DaRouteError::HelperUnavailable(format!(
            "{} points to a missing file",
            path.display()
        )));
    }
    let exe = std::env::current_exe()
        .map_err(|e| DaRouteError::HelperUnavailable(format!("current exe: {e}")))?;
    let candidate = exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("opcda-nethelper.exe");
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(DaRouteError::HelperUnavailable(format!(
            "opcda-nethelper.exe not found next to {}",
            exe.display()
        )))
    }
}

fn spawn_helper() -> Result<HelperLaunch, DaRouteError> {
    let exe = helper_exe()?;
    let pipe = format!("opcda-nethelper-{}", random_hex(random_bytes()?));
    let args = format!("--pipe {} --parent {}", pipe, std::process::id());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("opcda-nethelper-launch".to_string())
        .spawn(move || {
            let result = launch_elevated(&exe, &args);
            let _ = tx.send(result);
        })
        .map_err(|e| DaRouteError::HelperUnavailable(format!("launcher thread: {e}")))?;
    Ok(HelperLaunch { rx, pipe })
}

#[cfg(windows)]
fn launch_elevated(
    exe: &std::path::Path,
    args: &str,
) -> Result<LaunchedProcess, HelperLaunchError> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Threading::GetProcessId;
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{HRESULT, HSTRING, PCWSTR};
    let operation = HSTRING::from("runas");
    let file = HSTRING::from(exe.to_string_lossy().as_ref());
    let parameters = HSTRING::from(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(operation.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: the structure and all UTF-16 buffers remain valid through the call.
    if let Err(error) = unsafe { ShellExecuteExW(&mut info) } {
        return if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
            Err(HelperLaunchError::Declined)
        } else {
            Err(HelperLaunchError::Failed(format!(
                "ShellExecuteExW: {error}"
            )))
        };
    }
    let handle = info.hProcess;
    if handle.is_invalid() {
        return Err(HelperLaunchError::Failed(
            "launcher returned no process handle".to_string(),
        ));
    }
    // SAFETY: ShellExecuteExW transferred a valid process handle.
    let handle = unsafe { crate::winutil::OwnedHandle::from_raw(handle) };
    // SAFETY: the guard holds the live process handle for the call.
    let pid = unsafe { GetProcessId(handle.raw()) };
    let process = LaunchedProcess {
        _handle: Some(handle),
        pid,
    };
    if pid == 0 {
        return Err(HelperLaunchError::Failed(
            "could not identify helper process".to_string(),
        ));
    }
    Ok(process)
}

#[cfg(not(windows))]
fn launch_elevated(
    _exe: &std::path::Path,
    _args: &str,
) -> Result<LaunchedProcess, HelperLaunchError> {
    Err(HelperLaunchError::Failed(
        "elevated launch is only supported on Windows".to_string(),
    ))
}

impl HelperClient {
    #[cfg(windows)]
    fn connect(pipe: &str, expected_pid: u32, timeout: Duration) -> Result<Self, HelperIpcError> {
        let name = format!(r"\\.\pipe\{pipe}");
        wait_for_pipe(&name, timeout)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| HelperIpcError::Io(format!("pipe runtime: {e}")))?;
        let client = {
            let _guard = runtime.enter();
            tokio::net::windows::named_pipe::ClientOptions::new()
                .open(&name)
                .map_err(|e| HelperIpcError::Io(format!("open helper pipe: {e}")))?
        };
        verify_pipe_server(&client, expected_pid)?;
        let mut client = Self {
            transport: Some(HelperTransport {
                pipe: client,
                runtime,
            }),
        };
        client.call(Request::Hello {
            version: PROTOCOL_VERSION,
            client_pid: std::process::id(),
        })?;
        Ok(client)
    }
    #[cfg(not(windows))]
    fn connect(
        _pipe: &str,
        _expected_pid: u32,
        _timeout: Duration,
    ) -> Result<Self, HelperIpcError> {
        Err(HelperIpcError::Io(
            "named pipes are only supported on Windows".into(),
        ))
    }
}

fn validate_pipe_server(actual_pid: u32, expected_pid: u32) -> Result<(), HelperIpcError> {
    if expected_pid == 0 || actual_pid != expected_pid {
        return Err(HelperIpcError::PeerMismatch {
            expected: expected_pid,
            actual: actual_pid,
        });
    }
    Ok(())
}

#[cfg(windows)]
fn verify_pipe_server(
    file: &impl std::os::windows::io::AsRawHandle,
    expected_pid: u32,
) -> Result<(), HelperIpcError> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
    let mut pid = 0;
    // SAFETY: file owns a live pipe handle and pid points to writable storage.
    unsafe { GetNamedPipeServerProcessId(HANDLE(file.as_raw_handle() as _), &mut pid) }
        .map_err(|e| HelperIpcError::Io(format!("GetNamedPipeServerProcessId: {e}")))?;
    validate_pipe_server(pid, expected_pid)
}

#[cfg(windows)]
fn wait_for_pipe(name: &str, timeout: Duration) -> Result<(), HelperIpcError> {
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    use windows::core::HSTRING;
    let wide = HSTRING::from(name);
    let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: the HSTRING is terminated and stays live for the call.
    let ok = unsafe { WaitNamedPipeW(&wide, millis) };
    if ok.as_bool() {
        Ok(())
    } else {
        // SAFETY: this immediately reads the calling thread's last error.
        let error = unsafe { GetLastError() };
        Err(HelperIpcError::Io(format!(
            "WaitNamedPipeW failed with {}",
            error.0
        )))
    }
}

#[cfg(windows)]
fn best_route_luid(target: IpAddr) -> Option<u64> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::NetworkManagement::IpHelper::{GetBestRoute2, MIB_IPFORWARD_ROW2};
    use windows::Win32::Networking::WinSock::SOCKADDR_INET;

    let destination = sockaddr_inet(target);
    let mut best = MIB_IPFORWARD_ROW2::default();
    let mut source = SOCKADDR_INET::default();
    // SAFETY: destination is initialized and both output structures are writable.
    let result = unsafe { GetBestRoute2(None, 0, None, &destination, 0, &mut best, &mut source) };
    if result == ERROR_SUCCESS {
        // SAFETY: GetBestRoute2 initialized the returned LUID union.
        Some(unsafe { best.InterfaceLuid.Value })
    } else {
        None
    }
}

#[cfg(not(windows))]
fn best_route_luid(_target: IpAddr) -> Option<u64> {
    None
}

#[cfg(windows)]
fn host_route_luid(target: IpAddr) -> Option<u64> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::NetworkManagement::IpHelper::{GetIpForwardTable2, MIB_IPFORWARD_TABLE2};
    use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    let family = if target.is_ipv4() { AF_INET } else { AF_INET6 };
    let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    // SAFETY: family is valid and table points to writable output storage.
    let result = unsafe { GetIpForwardTable2(family, &mut table) };
    if result != ERROR_SUCCESS || table.is_null() {
        return None;
    }
    // SAFETY: success returned a uniquely owned IP Helper allocation.
    let table = unsafe { crate::winutil::MibTable::from_raw(table) };
    let mut found = None;
    // SAFETY: the API table contains NumEntries initialized rows and remains
    // owned by the guard throughout this traversal.
    unsafe {
        let table_ref = &*table.as_ptr();
        for index in 0..table_ref.NumEntries as usize {
            let row = &*table_ref.Table.as_ptr().add(index);
            let prefix = row.DestinationPrefix.PrefixLength;
            let expected = if target.is_ipv4() { 32 } else { 128 };
            if prefix == expected && sockaddr_ip(&row.DestinationPrefix.Prefix) == Some(target) {
                found = Some(row.InterfaceLuid.Value);
                break;
            }
        }
    }
    found
}

#[cfg(not(windows))]
fn host_route_luid(_target: IpAddr) -> Option<u64> {
    None
}

#[cfg(windows)]
fn sockaddr_inet(ip: IpAddr) -> windows::Win32::Networking::WinSock::SOCKADDR_INET {
    use windows::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, IN_ADDR, IN_ADDR_0, IN6_ADDR, IN6_ADDR_0, SOCKADDR_IN, SOCKADDR_IN6,
        SOCKADDR_IN6_0, SOCKADDR_INET,
    };
    match ip {
        IpAddr::V4(address) => {
            let mut sockaddr = SOCKADDR_INET::default();
            sockaddr.Ipv4 = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: u32::from(address).to_be(),
                    },
                },
                sin_zero: [0; 8],
            };
            sockaddr
        }
        IpAddr::V6(address) => {
            let mut sockaddr = SOCKADDR_INET::default();
            sockaddr.Ipv6 = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_port: 0,
                sin6_flowinfo: 0,
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 {
                        Byte: address.octets(),
                    },
                },
                Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 0 },
            };
            sockaddr
        }
    }
}

#[cfg(windows)]
fn sockaddr_ip(address: &windows::Win32::Networking::WinSock::SOCKADDR_INET) -> Option<IpAddr> {
    use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};
    // SAFETY: the family discriminant selects the corresponding sockaddr union member.
    unsafe {
        match address.si_family {
            AF_INET => {
                let raw = address.Ipv4.sin_addr.S_un.S_addr;
                Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(raw))))
            }
            AF_INET6 => Some(IpAddr::V6(Ipv6Addr::from(address.Ipv6.sin6_addr.u.Byte))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn spawn_failure_sets_unavailable_status_and_detail() {
        let mut manager = super::DaRouteManager::new();
        let error = super::DaRouteError::HelperUnavailable("spawn failed".into());
        assert_eq!(
            manager.ensure_helper_with(|| Err(error.clone())),
            Err(error.clone())
        );
        assert_eq!(manager.status(), super::DaRouteStatus::HelperUnavailable);
        assert_eq!(manager.last_error(), Some(&error));
    }

    #[cfg(not(windows))]
    #[test]
    fn refresh_failure_preserves_detail_and_invalidates_lease() {
        let mut manager = super::DaRouteManager::new();
        manager.helper_state = super::HelperState::Connected(super::HelperClient);
        let plan = super::RoutePlan {
            target: "10.0.0.1".parse().unwrap(),
            prefix: 32,
            interface_luid: 1,
            next_hop: "0.0.0.0".parse().unwrap(),
            via_gateway: false,
        };
        manager
            .leases
            .acquire(plan, super::AdapterId::UNSPECIFIED, true);
        manager.refresh();
        assert!(!manager.has_owned_routes());
        assert!(
            matches!(manager.last_error(), Some(super::DaRouteError::HelperUnavailable(detail)) if detail.contains("named pipes"))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn cancellation_after_helper_ready_never_attempts_add() {
        use std::cell::Cell;
        let mut manager = super::DaRouteManager::new();
        manager.helper_state = super::HelperState::Connected(super::HelperClient);
        let selected = adapter("192.168.1.20", 24, &[]);
        let target = "192.168.1.50".parse().unwrap();
        let checks = Cell::new(0);
        let result = manager.prepare_resolved(vec![target], Some(&selected), &[], || {
            checks.set(checks.get() + 1);
            checks.get() >= 2
        });
        assert!(matches!(result, Err(super::DaRouteError::Resolve(_))));
        assert_eq!(checks.get(), 2);
        assert!(manager.leases.entries.is_empty());
    }
    #[test]
    fn pipe_exchange_times_out_and_limits_reply() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let message = super::Request::Hello {
                version: 2,
                client_pid: std::process::id(),
            };
            let (mut client, _server) = tokio::io::duplex(64);
            let error =
                super::pipe_exchange(&mut client, &message, std::time::Duration::from_millis(20))
                    .await
                    .unwrap_err();
            assert_eq!(error, super::HelperIpcError::TimedOut);
            let (mut client, mut server) = tokio::io::duplex(32 * 1024);
            let task = tokio::spawn(async move {
                let mut request = [0u8; 512];
                let _ = server.read(&mut request).await.unwrap();
                server.write_all(&vec![b'x'; 16 * 1024 + 1]).await.unwrap();
            });
            let error =
                super::pipe_exchange(&mut client, &message, std::time::Duration::from_secs(1))
                    .await
                    .unwrap_err();
            assert_eq!(error, super::HelperIpcError::Oversize("reply"));
            task.await.unwrap();
        });
    }
    #[test]
    fn pipe_reply_errors_preserve_kind_and_detail() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let request = super::Request::Hello {
                version: 2,
                client_pid: 1,
            };
            for (reply, expected) in [
                (
                    b"{\"ok\":false,\"error\":\"route denied\"}\n".as_slice(),
                    super::HelperIpcError::Rejected("route denied".into()),
                ),
                (
                    b"not-json\n".as_slice(),
                    super::HelperIpcError::Protocol(
                        "invalid helper reply: expected ident at line 1 column 2".into(),
                    ),
                ),
            ] {
                let (mut client, mut server) = tokio::io::duplex(1024);
                let task = tokio::spawn(async move {
                    let mut buf = [0; 256];
                    let _ = server.read(&mut buf).await.unwrap();
                    server.write_all(reply).await.unwrap();
                });
                let error =
                    super::pipe_exchange(&mut client, &request, std::time::Duration::from_secs(1))
                        .await
                        .unwrap_err();
                assert_eq!(error, expected);
                task.await.unwrap();
            }
        });
    }

    #[test]
    fn helper_loss_invalidates_owned_but_keeps_reused_routes() {
        let mut manager = super::DaRouteManager::new();
        let adapter = super::AdapterId::UNSPECIFIED;
        let owned = super::RoutePlan {
            target: "10.0.0.1".parse().unwrap(),
            prefix: 32,
            interface_luid: 1,
            next_hop: "0.0.0.0".parse().unwrap(),
            via_gateway: false,
        };
        let reused = super::RoutePlan {
            target: "10.0.0.2".parse().unwrap(),
            ..owned
        };
        manager.leases.acquire(owned, adapter, true);
        manager.leases.acquire(reused, adapter, false);
        manager.invalidate_helper();
        assert_eq!(
            manager.leases.count(&super::RouteKey {
                target: owned.target,
                adapter
            }),
            0
        );
        assert_eq!(
            manager.leases.count(&super::RouteKey {
                target: reused.target,
                adapter
            }),
            1
        );
    }
    #[test]
    fn old_lease_cannot_release_reacquired_route_after_helper_loss() {
        let mut manager = super::DaRouteManager::new();
        let adapter = super::AdapterId::UNSPECIFIED;
        let plan = super::RoutePlan {
            target: "10.0.0.1".parse().unwrap(),
            prefix: 32,
            interface_luid: 1,
            next_hop: "0.0.0.0".parse().unwrap(),
            via_gateway: false,
        };
        let key = super::RouteKey {
            target: plan.target,
            adapter,
        };
        let old = manager.leases.acquire(plan, adapter, true);
        let prepared = super::PreparedRoutes {
            targets: vec![plan.target],
            keys: vec![(key, old)],
        };
        manager.invalidate_helper();
        let _new = manager.leases.acquire(plan, adapter, true);
        manager.release(prepared);
        assert_eq!(manager.leases.count(&key), 1);
    }
    #[cfg(windows)]
    #[test]
    fn helper_client_closes_real_pipe_after_timeout() {
        use std::time::Duration;
        let pipe = format!(
            "opclook-client-timeout-{}-{}",
            std::process::id(),
            super::random_hex(super::random_bytes().unwrap())
        );
        let name = format!(r"\\.\pipe\{pipe}");
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (closed_tx, closed_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::windows::named_pipe::ServerOptions;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut server = ServerOptions::new()
                    .first_pipe_instance(true)
                    .create(name)
                    .unwrap();
                ready_tx.send(()).unwrap();
                server.connect().await.unwrap();
                let mut byte = [0u8; 1];
                loop {
                    server.read_exact(&mut byte).await.unwrap();
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                server.write_all(b"{\"ok\":true}\n").await.unwrap();
                loop {
                    server.read_exact(&mut byte).await.unwrap();
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                let closed = matches!(server.read(&mut byte).await, Ok(0) | Err(_));
                closed_tx.send(closed).unwrap();
            });
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut client =
            super::HelperClient::connect(&pipe, std::process::id(), Duration::from_secs(1))
                .unwrap();
        let error = client
            .call(super::Request::Hello {
                version: 2,
                client_pid: std::process::id(),
            })
            .unwrap_err();
        assert_eq!(error, super::HelperIpcError::TimedOut);
        assert!(client.transport.is_none());
        assert!(closed_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        server.join().unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn real_named_pipe_closes_after_exchange_timeout() {
        use tokio::io::AsyncReadExt;
        use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let name = format!(
                r"\\.\pipe\opclook-timeout-{}-{}",
                std::process::id(),
                super::random_hex(super::random_bytes().unwrap())
            );
            let mut server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&name)
                .unwrap();
            let mut client = ClientOptions::new().open(&name).unwrap();
            assert!(super::verify_pipe_server(&client, std::process::id()).is_ok());
            assert!(
                super::verify_pipe_server(&client, std::process::id().wrapping_add(1)).is_err()
            );
            server.connect().await.unwrap();
            let message = super::Request::Hello {
                version: 2,
                client_pid: std::process::id(),
            };
            let error =
                super::pipe_exchange(&mut client, &message, std::time::Duration::from_millis(50))
                    .await
                    .unwrap_err();
            assert_eq!(error, super::HelperIpcError::TimedOut);
            drop(client);
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                let mut buffer = [0; 512];
                loop {
                    match server.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            })
            .await
            .expect("server observed pipe closure");
        });
    }
    #[cfg(windows)]
    #[test]
    fn pipe_server_process_is_verified_before_use() {
        use std::os::windows::io::FromRawHandle;
        use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
        use windows::Win32::System::Pipes::{
            CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
        };
        use windows::core::{HSTRING, PCWSTR};
        let name = format!(
            r"\\.\pipe\opclook-peer-test-{}-{}",
            std::process::id(),
            super::random_hex(super::random_bytes().unwrap())
        );
        let wide = HSTRING::from(name.as_str());
        // SAFETY: the name is valid, and the returned handle is transferred
        // immediately into File for exactly-once closure.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                1024,
                1024,
                0,
                None,
            )
        };
        assert!(!handle.is_invalid());
        // SAFETY: CreateNamedPipeW returned the unique handle transferred to File.
        let _server = unsafe { std::fs::File::from_raw_handle(handle.0 as _) };
        let client = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
            .unwrap();
        assert!(super::verify_pipe_server(&client, std::process::id()).is_ok());
        assert!(super::verify_pipe_server(&client, std::process::id().wrapping_add(1)).is_err());
    }

    #[test]
    fn pipe_server_identity_must_match_launched_process() {
        assert!(super::validate_pipe_server(123, 123).is_ok());
        assert!(super::validate_pipe_server(124, 123).is_err());
        assert!(super::validate_pipe_server(0, 0).is_err());
    }

    #[test]
    fn pending_launch_survives_request_deadline_and_is_reused() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut manager = super::DaRouteManager::new();
        manager.helper_state = super::HelperState::Launching(super::HelperLaunch {
            rx,
            pipe: String::new(),
        });
        assert!(matches!(
            manager.ensure_helper(),
            Err(super::DaRouteError::HelperStarting)
        ));
        assert!(matches!(
            manager.helper_state,
            super::HelperState::Launching(_)
        ));
        tx.send(Err(super::HelperLaunchError::Failed(
            "launch failed".into(),
        )))
        .unwrap();
        assert_eq!(
            manager.ensure_helper(),
            Err(super::DaRouteError::HelperUnavailable(
                "launch failed".into()
            ))
        );
        assert_eq!(manager.status(), super::DaRouteStatus::HelperUnavailable);
    }

    #[cfg(not(windows))]
    #[test]
    fn launched_process_survives_pipe_retry() {
        let mut manager = super::DaRouteManager::new();
        manager.helper_state = super::HelperState::Launched {
            pipe: String::new(),
            process: super::LaunchedProcess { pid: 123 },
            last_error: None,
        };
        assert!(matches!(
            manager.ensure_helper(),
            Err(super::DaRouteError::HelperUnavailable(_))
        ));
        assert!(matches!(
            manager.helper_state,
            super::HelperState::Launched {
                process: super::LaunchedProcess { pid: 123 },
                ..
            }
        ));
    }

    use super::*;
    use crate::net::{AdapterAddress, AdapterState};

    fn address(ip: &str, prefix: u8) -> AdapterAddress {
        AdapterAddress {
            ip: ip.parse().unwrap(),
            prefix,
            scope_id: None,
        }
    }

    fn adapter(ip: &str, prefix: u8, gateways: &[&str]) -> AdapterInfo {
        AdapterInfo {
            id: AdapterId::parse("11111111-1111-1111-1111-111111111111").unwrap(),
            name: "Ethernet".to_string(),
            description: String::new(),
            state: AdapterState::Up,
            if_type: 6,
            addresses: vec![address(ip, prefix)],
            gateways: gateways.iter().map(|g| g.parse().unwrap()).collect(),
            if_index: std::num::NonZeroU32::new(7),
            ipv6_if_index: std::num::NonZeroU32::new(8),
            luid: 42,
            metric: 10,
            is_physical: true,
            is_loopback: false,
        }
    }

    #[test]
    fn on_link_ipv4_route_uses_unspecified_next_hop() {
        let plan = plan_route(
            "192.168.1.50".parse().unwrap(),
            &adapter("192.168.1.20", 24, &[]),
        )
        .unwrap();
        assert_eq!(plan.prefix, 32);
        assert_eq!(plan.next_hop, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(plan.interface_luid, 42);
        assert!(!plan.via_gateway);
    }

    #[test]
    fn off_link_ipv4_route_uses_the_adapter_gateway() {
        let plan = plan_route(
            "10.20.30.40".parse().unwrap(),
            &adapter("192.168.1.20", 24, &["192.168.1.1"]),
        )
        .unwrap();
        assert_eq!(plan.next_hop, "192.168.1.1".parse::<IpAddr>().unwrap());
        assert_eq!(plan.prefix, 32);
        assert!(plan.via_gateway);
    }

    #[test]
    fn off_link_without_gateway_is_an_error() {
        assert_eq!(
            plan_route(
                "10.20.30.40".parse().unwrap(),
                &adapter("192.168.1.20", 24, &[])
            ),
            Err(DaRouteError::NoGateway("10.20.30.40".parse().unwrap()))
        );
    }

    #[test]
    fn ipv6_on_link_and_gateway_plans_use_128_prefix() {
        let on_link = plan_route(
            "2001:db8::5".parse().unwrap(),
            &adapter("2001:db8::1", 64, &[]),
        )
        .unwrap();
        assert_eq!(on_link.prefix, 128);
        assert_eq!(on_link.next_hop, "::".parse::<IpAddr>().unwrap());

        let via_gateway = plan_route(
            "2001:db8:2::5".parse().unwrap(),
            &adapter("2001:db8::1", 64, &["2001:db8::ff"]),
        )
        .unwrap();
        assert_eq!(via_gateway.prefix, 128);
        assert_eq!(
            via_gateway.next_hop,
            "2001:db8::ff".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn loopback_multicast_and_broadcast_targets_are_rejected() {
        let adapter = adapter("192.168.1.20", 24, &[]);
        for target in [
            "127.0.0.1".parse().unwrap(),
            "224.0.0.1".parse().unwrap(),
            "255.255.255.255".parse().unwrap(),
            "::".parse().unwrap(),
            "ff02::1".parse().unwrap(),
        ] {
            assert!(matches!(
                plan_route(target, &adapter),
                Err(DaRouteError::InvalidTarget(_))
            ));
        }
    }

    #[test]
    fn route_decision_reuses_the_selected_adapter_and_rejects_conflicts() {
        assert_eq!(route_decision(42, Some(42), None), RouteDecision::Reuse);
        assert_eq!(route_decision(42, Some(7), None), RouteDecision::Add);
        assert_eq!(route_decision(42, Some(7), Some(42)), RouteDecision::Reuse);
        assert_eq!(
            route_decision(42, Some(42), Some(7)),
            RouteDecision::Conflict
        );
        assert_eq!(
            route_decision(42, Some(7), Some(7)),
            RouteDecision::Conflict
        );
    }

    #[test]
    fn refcount_releases_only_after_the_last_owner() {
        let plan = plan_route(
            "192.168.1.50".parse().unwrap(),
            &adapter("192.168.1.20", 24, &[]),
        )
        .unwrap();
        let key = RouteKey {
            target: plan.target,
            adapter: AdapterId::parse("11111111-1111-1111-1111-111111111111").unwrap(),
        };
        let mut leases = RouteLeases::default();
        let first = leases.acquire(plan, key.adapter, true);
        let second = leases.acquire(plan, key.adapter, false);
        assert_eq!(leases.count(&key), 2);
        assert!(leases.release(&key, &first).is_none());
        let released = leases.release(&key, &second).expect("last reference");
        assert!(released.1, "route was helper-owned");
        assert_eq!(leases.count(&key), 0);
    }

    #[test]
    fn local_targets_do_not_require_an_adapter() {
        let mut manager = DaRouteManager::new();
        let prepared = manager
            .prepare_host("127.0.0.1", None, &["127.0.0.1".parse().unwrap()])
            .unwrap();
        assert!(prepared.is_empty());
        assert_eq!(manager.status(), DaRouteStatus::Idle);
    }

    #[test]
    fn reuse_without_gateway_does_not_require_plan_route() {
        let adapter = adapter("192.168.1.20", 24, &[]);
        let target: IpAddr = "10.20.30.40".parse().unwrap();
        assert_eq!(
            plan_route(target, &adapter),
            Err(DaRouteError::NoGateway(target)),
            "the adapter really has no gateway for an Add path"
        );
        assert_eq!(
            route_decision(adapter.luid, Some(adapter.luid), None),
            RouteDecision::Reuse
        );

        let plan = reuse_plan(target, &adapter);
        assert_eq!(plan.next_hop, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert!(!plan.via_gateway);
        let key = RouteKey {
            target,
            adapter: adapter.id,
        };
        let mut leases = RouteLeases::default();
        leases.acquire(plan, adapter.id, false);
        assert_eq!(leases.count(&key), 1);
        assert_eq!(leases.plan(&key), Some(plan));
    }

    #[test]
    fn live_lease_reuses_the_stored_plan_after_a_gateway_change() {
        let mut adapter = adapter("192.168.1.20", 24, &["192.168.1.1"]);
        let target: IpAddr = "10.20.30.40".parse().unwrap();
        let plan = plan_route(target, &adapter).unwrap();
        let key = RouteKey {
            target,
            adapter: adapter.id,
        };
        let mut leases = RouteLeases::default();
        leases.acquire(plan, adapter.id, false);

        adapter.gateways = vec!["192.168.1.254".parse().unwrap()];
        assert_eq!(
            plan_route(target, &adapter).unwrap().next_hop,
            "192.168.1.254".parse::<IpAddr>().unwrap()
        );
        let stored = leases.plan(&key).expect("live lease plan");
        assert_eq!(
            stored.next_hop,
            "192.168.1.1".parse::<IpAddr>().unwrap(),
            "re-acquiring must not re-plan against a changed gateway"
        );
        leases.acquire(stored, adapter.id, false);
        assert_eq!(leases.count(&key), 2);
        assert_eq!(leases.plan(&key), Some(plan));
    }
}
