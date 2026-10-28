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

use crate::constants::{
    DA_HELPER_EXE_ENV, DA_HELPER_PROTOCOL_VERSION, DA_HELPER_START_TIMEOUT, DA_ROUTE_LIFETIME_SECS,
};
use crate::i18n::{Lang, Tr, fmt, tr};
use crate::net::{AdapterId, AdapterInfo, SubnetSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A planned temporary host route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoutePlan {
    pub target: IpAddr,
    pub prefix: u8,
    pub interface_luid: u64,
    pub next_hop: IpAddr,
    pub via_gateway: bool,
}

/// What to do with the current routing table state for one DA target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteDecision {
    /// The best route already goes through the selected adapter.
    Reuse,
    /// No host route exists; add one through the selected adapter.
    Add,
    /// A host route already goes through a different adapter. Never overwrite.
    Conflict,
}

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

#[derive(Clone, Copy, Debug)]
struct LeaseEntry {
    plan: RoutePlan,
    count: usize,
    owned: bool,
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

    fn acquire(&mut self, plan: RoutePlan, adapter: AdapterId, owned: bool) {
        let key = RouteKey {
            target: plan.target,
            adapter,
        };
        let entry = self.entries.entry(key).or_insert(LeaseEntry {
            plan,
            count: 0,
            owned,
        });
        entry.count += 1;
        if owned {
            entry.owned = true;
            entry.plan = plan;
        }
    }

    /// Drop one reference. Returns `Some((plan, owned))` only when the last
    /// reference is gone, so the caller knows whether the helper route must be
    /// removed.
    fn release(&mut self, key: &RouteKey) -> Option<(RoutePlan, bool)> {
        let entry = self.entries.get_mut(key)?;
        entry.count = entry.count.saturating_sub(1);
        if entry.count > 0 {
            return None;
        }
        let entry = self.entries.remove(key)?;
        Some((entry.plan, entry.owned))
    }

    fn owned_keys(&self) -> Vec<RouteKey> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.owned)
            .map(|(key, _)| *key)
            .collect()
    }

    fn owned_plans(&self) -> Vec<RoutePlan> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.owned)
            .map(|(_, entry)| entry.plan)
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PreparedRoutes {
    pub targets: Vec<IpAddr>,
    keys: Vec<RouteKey>,
}

impl PreparedRoutes {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaRouteError {
    NoAdapter,
    AdapterUnavailable,
    Resolve(String),
    InvalidTarget(IpAddr),
    NoGateway(IpAddr),
    Conflict(IpAddr),
    HelperStarting,
    HelperUnavailable(String),
    Add(IpAddr, String),
    Verify(IpAddr),
    UacDeclined,
}

impl DaRouteError {
    pub fn localized(&self, lang: Lang) -> String {
        let detail = match self {
            Self::NoAdapter => tr(lang, Tr::DaRouteNoAdapter).to_string(),
            Self::AdapterUnavailable => tr(lang, Tr::SelectAdapterRequired).to_string(),
            Self::Resolve(host) => fmt(lang, Tr::DaRouteResolve, &[host]),
            Self::InvalidTarget(ip) => fmt(lang, Tr::DaRouteResolve, &[&ip.to_string()]),
            Self::NoGateway(ip) => fmt(lang, Tr::DaRouteNoGateway, &[&ip.to_string()]),
            Self::Conflict(ip) => fmt(lang, Tr::DaRouteConflict, &[&ip.to_string()]),
            Self::HelperStarting => tr(lang, Tr::DaRouteHelperStarting).to_string(),
            Self::HelperUnavailable(_) | Self::UacDeclined => {
                tr(lang, Tr::DaRouteHelperUnavailable).to_string()
            }
            Self::Add(ip, detail) => fmt(lang, Tr::DaRouteAdd, &[&ip.to_string(), detail]),
            Self::Verify(ip) => fmt(lang, Tr::DaRouteVerify, &[&ip.to_string()]),
        };
        fmt(lang, Tr::DaRouteError, &[&detail])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaRouteStatus {
    Idle,
    HelperStarting,
    HelperRunning,
    HelperUnavailable,
}

impl DaRouteStatus {
    pub fn localized(self, lang: Lang) -> &'static str {
        match self {
            Self::Idle => tr(lang, Tr::DaRouteHelperIdle),
            Self::HelperStarting => tr(lang, Tr::DaRouteHelperStarting),
            Self::HelperRunning => tr(lang, Tr::DaRouteHelperRunning),
            Self::HelperUnavailable => tr(lang, Tr::DaRouteHelperUnavailable),
        }
    }
}

#[derive(Debug)]
struct HelperLaunch {
    rx: std::sync::mpsc::Receiver<Result<(), HelperLaunchError>>,
    pipe: String,
    token: String,
    ready: bool,
}

impl HelperLaunch {
    fn poll_launch(
        &mut self,
    ) -> Result<Result<(), HelperLaunchError>, std::sync::mpsc::TryRecvError> {
        if self.ready {
            return Ok(Ok(()));
        }
        let result = self.rx.try_recv();
        if matches!(result, Ok(Ok(()))) {
            self.ready = true;
        }
        result
    }
}

#[derive(Debug)]
enum HelperLaunchError {
    #[cfg_attr(not(windows), allow(dead_code))]
    Declined,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct PipeMessage {
    version: u32,
    token: String,
    op: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    prefix: Option<u8>,
    #[serde(default)]
    interface_luid: Option<u64>,
    #[serde(default)]
    next_hop: Option<String>,
    #[serde(default)]
    lifetime_secs: Option<u32>,
    #[serde(default)]
    client_pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PipeReply {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

struct HelperClient {
    reader: BufReader<std::fs::File>,
    writer: std::fs::File,
    token: String,
}

impl HelperClient {
    fn send(&mut self, message: &PipeMessage) -> Result<(), String> {
        let mut line = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.writer
            .write_all(&line)
            .map_err(|e| format!("pipe write failed: {e}"))?;
        self.writer
            .flush()
            .map_err(|e| format!("pipe flush failed: {e}"))
    }

    fn call(&mut self, mut message: PipeMessage) -> Result<(), String> {
        message.version = DA_HELPER_PROTOCOL_VERSION;
        message.token.clone_from(&self.token);
        self.send(&message)?;
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| format!("pipe read failed: {e}"))?;
        if line.trim().is_empty() {
            return Err("helper closed the pipe".to_string());
        }
        let reply: PipeReply =
            serde_json::from_str(line.trim()).map_err(|e| format!("invalid helper reply: {e}"))?;
        if reply.ok {
            Ok(())
        } else {
            Err(reply
                .error
                .unwrap_or_else(|| "helper rejected the request".to_string()))
        }
    }

    fn add(&mut self, plan: RoutePlan) -> Result<(), String> {
        self.call_with_op("add", plan)
    }

    fn refresh(&mut self, plan: RoutePlan) -> Result<(), String> {
        self.call_with_op("refresh", plan)
    }

    fn call_with_op(&mut self, op: &str, plan: RoutePlan) -> Result<(), String> {
        self.call(PipeMessage {
            version: DA_HELPER_PROTOCOL_VERSION,
            token: String::new(),
            op: op.to_string(),
            target: Some(plan.target.to_string()),
            prefix: Some(plan.prefix),
            interface_luid: Some(plan.interface_luid),
            next_hop: Some(plan.next_hop.to_string()),
            lifetime_secs: Some(DA_ROUTE_LIFETIME_SECS),
            client_pid: None,
        })
    }

    fn remove(&mut self, plan: RoutePlan) -> Result<(), String> {
        self.call(PipeMessage {
            version: DA_HELPER_PROTOCOL_VERSION,
            token: String::new(),
            op: "remove".to_string(),
            target: Some(plan.target.to_string()),
            prefix: Some(plan.prefix),
            interface_luid: Some(plan.interface_luid),
            next_hop: Some(plan.next_hop.to_string()),
            lifetime_secs: None,
            client_pid: None,
        })
    }
}

pub struct DaRouteManager {
    helper: Option<HelperClient>,
    launch: Option<HelperLaunch>,
    status: DaRouteStatus,
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
            helper: None,
            launch: None,
            status: DaRouteStatus::Idle,
            leases: RouteLeases::default(),
            last_error: None,
        }
    }

    pub fn status(&self) -> DaRouteStatus {
        self.status
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
                self.leases.acquire(plan, adapter.id, false);
                prepared.targets.push(target);
                prepared.keys.push(key);
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
                    self.leases.acquire(plan, adapter.id, false);
                    prepared.targets.push(target);
                    prepared.keys.push(key);
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
                    let Some(helper) = self.helper.as_mut() else {
                        let error = DaRouteError::HelperUnavailable("no helper client".to_string());
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    };
                    if let Err(detail) = helper.add(plan) {
                        let error = DaRouteError::Add(target, detail);
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    }
                    if best_route_luid(target) != Some(adapter.luid) {
                        if let Some(helper) = self.helper.as_mut() {
                            let _ = helper.remove(plan);
                        }
                        let error = DaRouteError::Verify(target);
                        self.release_prepared(&mut prepared);
                        self.last_error = Some(error.clone());
                        return Err(error);
                    }
                    self.leases.acquire(plan, adapter.id, true);
                    prepared.targets.push(target);
                    prepared.keys.push(key);
                }
            }
        }
        self.status = if self.helper.is_some() {
            DaRouteStatus::HelperRunning
        } else {
            DaRouteStatus::Idle
        };
        self.last_error = None;
        Ok(prepared)
    }

    /// Refresh the finite lifetime of every helper-owned route. Called
    /// periodically by the app; a missing helper is not an error (the route
    /// simply expires unless a new operation re-adds it).
    pub fn refresh(&mut self) {
        if self.helper.is_none() {
            return;
        }
        let plans = self.leases.owned_plans();
        for plan in plans {
            if let Some(helper) = self.helper.as_mut() {
                let _ = helper.refresh(plan);
            }
        }
    }

    pub fn has_owned_routes(&self) -> bool {
        !self.leases.owned_plans().is_empty()
    }

    pub fn release(&mut self, prepared: PreparedRoutes) {
        let mut prepared = prepared;
        self.release_prepared(&mut prepared);
    }

    fn release_prepared(&mut self, prepared: &mut PreparedRoutes) {
        for key in prepared.keys.drain(..) {
            if let Some((plan, owned)) = self.leases.release(&key) {
                if owned {
                    if let Some(helper) = self.helper.as_mut() {
                        let _ = helper.remove(plan);
                    }
                }
            }
        }
        prepared.targets.clear();
        if self.leases.entries.is_empty() {
            self.status = if self.helper.is_some() {
                DaRouteStatus::HelperRunning
            } else {
                DaRouteStatus::Idle
            };
        }
    }

    fn ensure_helper(&mut self) -> Result<(), DaRouteError> {
        if self.helper.is_some() {
            return Ok(());
        }
        if self.launch.is_none() {
            self.launch = Some(spawn_helper()?);
            self.status = DaRouteStatus::HelperStarting;
        }
        let deadline = Instant::now() + DA_HELPER_START_TIMEOUT;
        loop {
            let Some(launch) = self.launch.as_mut() else {
                return Ok(());
            };
            let result = launch.poll_launch();
            match result {
                Ok(Ok(())) => {
                    let pipe = launch.pipe.clone();
                    let token = launch.token.clone();
                    match HelperClient::connect(&pipe, &token, Duration::from_millis(250)) {
                        Ok(client) => {
                            self.launch = None;
                            self.helper = Some(client);
                            self.status = DaRouteStatus::HelperRunning;
                            return Ok(());
                        }
                        Err(_) if Instant::now() < deadline => {}
                        Err(error) => {
                            self.launch = None;
                            self.status = DaRouteStatus::HelperUnavailable;
                            return Err(DaRouteError::HelperUnavailable(error));
                        }
                    }
                }
                Ok(Err(HelperLaunchError::Declined)) => {
                    self.launch = None;
                    self.status = DaRouteStatus::HelperUnavailable;
                    return Err(DaRouteError::UacDeclined);
                }
                Ok(Err(HelperLaunchError::Failed(error))) => {
                    self.launch = None;
                    self.status = DaRouteStatus::HelperUnavailable;
                    return Err(DaRouteError::HelperUnavailable(error));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.launch = None;
                    self.status = DaRouteStatus::HelperUnavailable;
                    return Err(DaRouteError::HelperUnavailable(
                        "helper launcher exited".to_string(),
                    ));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    let pipe = launch.pipe.clone();
                    let token = launch.token.clone();
                    if let Ok(client) =
                        HelperClient::connect(&pipe, &token, Duration::from_millis(50))
                    {
                        self.launch = None;
                        self.helper = Some(client);
                        self.status = DaRouteStatus::HelperRunning;
                        return Ok(());
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(DaRouteError::HelperStarting);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for DaRouteManager {
    fn drop(&mut self) {
        let keys = self.leases.owned_keys();
        for key in keys {
            if let Some((plan, owned)) = self.leases.release(&key) {
                if owned {
                    if let Some(helper) = self.helper.as_mut() {
                        let _ = helper.remove(plan);
                    }
                }
            }
        }
    }
}

fn resolve_targets(host: &str) -> Result<Vec<IpAddr>, DaRouteError> {
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

fn random_bytes() -> [u8; 16] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut state = now
        ^ (std::process::id() as u64) << 32
        ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let mut out = [0u8; 16];
    for chunk in out.chunks_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        chunk.copy_from_slice(&state.to_le_bytes());
    }
    out
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
    let pipe = format!("opcda-nethelper-{}", random_hex(random_bytes()));
    let token = random_hex(random_bytes());
    let args = format!(
        "--pipe {} --token {} --parent {}",
        pipe,
        token,
        std::process::id()
    );
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("opcda-nethelper-launch".to_string())
        .spawn(move || {
            let result = launch_elevated(&exe, &args);
            let _ = tx.send(result);
        })
        .map_err(|e| DaRouteError::HelperUnavailable(format!("launcher thread: {e}")))?;
    Ok(HelperLaunch {
        rx,
        pipe,
        token,
        ready: false,
    })
}

#[cfg(windows)]
fn launch_elevated(exe: &std::path::Path, args: &str) -> Result<(), HelperLaunchError> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{HSTRING, PCWSTR};

    let operation = HSTRING::from("runas");
    let file = HSTRING::from(exe.to_string_lossy().as_ref());
    let parameters = HSTRING::from(args);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(parameters.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        )
    };
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else if code == 1223 {
        Err(HelperLaunchError::Declined)
    } else {
        Err(HelperLaunchError::Failed(format!(
            "ShellExecuteW(runas) failed with {code}"
        )))
    }
}

#[cfg(not(windows))]
fn launch_elevated(_exe: &std::path::Path, _args: &str) -> Result<(), HelperLaunchError> {
    Err(HelperLaunchError::Failed(
        "elevated launch is only supported on Windows".to_string(),
    ))
}

impl HelperClient {
    fn connect(pipe: &str, token: &str, timeout: Duration) -> Result<Self, String> {
        let name = format!(r"\\.\pipe\{pipe}");
        wait_for_pipe(&name, timeout)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
            .map_err(|e| format!("open {name}: {e}"))?;
        let writer = file
            .try_clone()
            .map_err(|e| format!("clone pipe handle: {e}"))?;
        let mut client = Self {
            reader: BufReader::new(file),
            writer,
            token: token.to_string(),
        };
        let hello = PipeMessage {
            version: DA_HELPER_PROTOCOL_VERSION,
            token: String::new(),
            op: "hello".to_string(),
            target: None,
            prefix: None,
            interface_luid: None,
            next_hop: None,
            lifetime_secs: None,
            client_pid: Some(std::process::id()),
        };
        client.call(hello)?;
        Ok(client)
    }
}

#[cfg(windows)]
fn wait_for_pipe(name: &str, timeout: Duration) -> Result<(), String> {
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    use windows::core::HSTRING;
    let wide = HSTRING::from(name);
    let millis = timeout.as_millis().min(u32::MAX as u128) as u32;
    let ok = unsafe { WaitNamedPipeW(&wide, millis) };
    if ok.as_bool() {
        Ok(())
    } else {
        let error = unsafe { GetLastError() };
        Err(format!("WaitNamedPipeW failed with {}", error.0))
    }
}

#[cfg(not(windows))]
fn wait_for_pipe(_name: &str, _timeout: Duration) -> Result<(), String> {
    Err("named pipes are only supported on Windows".to_string())
}

#[cfg(windows)]
fn best_route_luid(target: IpAddr) -> Option<u64> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::NetworkManagement::IpHelper::{GetBestRoute2, MIB_IPFORWARD_ROW2};
    use windows::Win32::Networking::WinSock::SOCKADDR_INET;

    let destination = sockaddr_inet(target);
    let mut best = MIB_IPFORWARD_ROW2::default();
    let mut source = SOCKADDR_INET::default();
    let result = unsafe { GetBestRoute2(None, 0, None, &destination, 0, &mut best, &mut source) };
    if result == ERROR_SUCCESS {
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
    use windows::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIpForwardTable2, MIB_IPFORWARD_TABLE2,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    let family = if target.is_ipv4() { AF_INET } else { AF_INET6 };
    let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    let result = unsafe { GetIpForwardTable2(family, &mut table) };
    if result != ERROR_SUCCESS || table.is_null() {
        return None;
    }
    let mut found = None;
    unsafe {
        let table_ref = &*table;
        for index in 0..table_ref.NumEntries as usize {
            let row = &*table_ref.Table.as_ptr().add(index);
            let prefix = row.DestinationPrefix.PrefixLength;
            let expected = if target.is_ipv4() { 32 } else { 128 };
            if prefix == expected && sockaddr_ip(&row.DestinationPrefix.Prefix) == Some(target) {
                found = Some(row.InterfaceLuid.Value);
                break;
            }
        }
        FreeMibTable(table as *const _);
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
    fn successful_launch_remains_ready_after_first_pipe_retry() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(())).unwrap();
        drop(tx);
        let mut launch = super::HelperLaunch {
            rx,
            pipe: String::new(),
            token: String::new(),
            ready: false,
        };
        assert!(matches!(launch.poll_launch(), Ok(Ok(()))));
        assert!(matches!(launch.poll_launch(), Ok(Ok(()))));
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
            if_index: 7,
            ipv6_if_index: 8,
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
        leases.acquire(plan, key.adapter, true);
        leases.acquire(plan, key.adapter, false);
        assert_eq!(leases.count(&key), 2);
        assert!(leases.release(&key).is_none());
        let released = leases.release(&key).expect("last reference");
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
