//! GUI-independent session state, command correlation and scheduling.
//!
//! Transport workers are not part of the production frontend API:
//!
//! ```compile_fail
//! use opclook_engine::Gateway;
//! ```
//!
//! ```compile_fail
//! use opclook_engine::metadata::Gateway;
//! ```
pub mod constants;
/// Plain metadata and inventory available to application frontends.
pub mod metadata {
    pub use opclook_backend::bff::ua::{
        SCAN_SUBNET_TOO_LARGE, SECURITY_MODE_AUTO, SECURITY_MODE_NONE, SECURITY_MODE_SIGN,
        SECURITY_MODE_SIGN_AND_ENCRYPT, SECURITY_POLICY_AUTO, SECURITY_POLICY_BASIC256_SHA256,
        SECURITY_POLICY_NONE, client_cert_subject, pki_dir_public,
    };
    pub use opclook_backend::bff::vt::vt_name;
    pub use opclook_backend::constants::{
        BROWSE_MAX_REFERENCES_PER_NODE, DA_HELPER_PROTOCOL_VERSION, DA_HELPER_START_TIMEOUT,
        DA_ROUTE_LIFETIME, DA_ROUTE_LIFETIME_SECS, DA_ROUTE_REFRESH, DEFAULT_UA_HOST,
        DEFAULT_UA_PORT, DISCOVERY_GUARD_TIMEOUT, DISCOVERY_URLS, MAX_BROWSE_NODES, MDNS_WINDOW,
        READ_MAX_ITEMS_PER_CALL, SCAN_ADDRESS_TIMEOUT, SCAN_CONFIRM_THRESHOLD, SCAN_PARALLELISM,
        SCAN_TCP_CONNECT_TIMEOUT, SCAN_UA_HANDSHAKE_TIMEOUT, UA_SEARCH_TIMEOUT,
    };
    pub use opclook_backend::net::{
        AdapterAddress, AdapterId, AdapterInfo, AdapterSelection, AdapterState, CacheBuild,
        CacheLoadReport, CacheRejection, CacheSkip, CacheSkipReason, CachedAdapter,
        NetworkInventory, NetworkLease, NetworkState, SelectionResolution, SelectionState,
        adapter_is_selected, build_adapter_cache, inventory_adapters, load_cached_adapters,
        reconcile_selection, selectable_adapters, selected_adapter, selected_adapter_count,
        synthetic_loopback,
    };
    pub use opclook_contract::net::{NetError, SubnetHosts, SubnetSpec};
    pub fn validate_ua_security(
        policy: opclook_contract::model::UaSecurityPolicy,
        mode: opclook_contract::model::UaSecurityMode,
        endpoints: &[opclook_contract::model::UaEndpointInfo],
    ) -> Result<(), opclook_contract::model::BackendMsg> {
        opclook_backend::bff::ua::resolve_security(policy, mode, endpoints)
            .map(|_| ())
            .map_err(|detail| opclook_contract::model::BackendMsg::SecurityResolveFailed { detail })
    }
    #[cfg(test)]
    mod tests {
        use super::validate_ua_security;
        use opclook_contract::model::{
            AdvertisedMode, AdvertisedPolicy, BackendMsg, UaEndpointInfo, UaSecurityMode,
            UaSecurityPolicy,
        };

        #[test]
        fn validation_reports_typed_errors_and_accepts_available_pair() {
            let endpoint = UaEndpointInfo {
                url: "opc.tcp://localhost:4840".parse().unwrap(),
                security_policy: AdvertisedPolicy::from_wire("None"),
                security_mode: AdvertisedMode::from_wire("None"),
            };
            assert!(
                validate_ua_security(
                    UaSecurityPolicy::None,
                    UaSecurityMode::None,
                    std::slice::from_ref(&endpoint)
                )
                .is_ok()
            );
            assert!(matches!(
                validate_ua_security(
                    UaSecurityPolicy::Basic256Sha256,
                    UaSecurityMode::Sign,
                    &[endpoint]
                ),
                Err(BackendMsg::SecurityResolveFailed { .. })
            ));
        }
    }
}
#[cfg(feature = "test-support")]
pub mod testing {
    pub use opclook_backend::bff::Gateway as TestGateway;
    pub use opclook_backend::bff::ua::use_private_pki_dir;
}
use opclook_backend::bff::Gateway;
use opclook_backend::net::AdapterInfo;
use opclook_backend::route_service::{RouteService, RouteTicket};
use opclook_contract::da_route::{DaRouteError, DaRouteStatus};
use opclook_contract::model::{
    BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, GatewayShutdown, ItemKey,
    Operation, Origin, Owner, Protocol, RequestId, SendError, SessionId, SubscriptionState,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Connecting,
    Connected,
    Disconnected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconnectState {
    Idle,
    Waiting { at: Instant, attempt: u32 },
    Stopped,
    Exhausted,
}
impl ReconnectState {
    pub fn attempt(self) -> u32 {
        if let Self::Waiting { attempt, .. } = self {
            attempt
        } else {
            0
        }
    }
    pub fn schedule(
        &mut self,
        now: Instant,
        cap: u32,
        base: Duration,
        max: Duration,
    ) -> ReconnectScheduled {
        let previous = match *self {
            Self::Stopped | Self::Exhausted => return ReconnectScheduled::Unchanged,
            Self::Waiting { attempt, .. } => attempt,
            Self::Idle => 0,
        };
        if previous >= cap {
            *self = Self::Exhausted;
            return ReconnectScheduled::Exhausted;
        }
        let attempt = previous + 1;
        let delay = reconnect_delay(attempt, base, max);
        let Some(at) = now.checked_add(delay) else {
            *self = Self::Exhausted;
            return ReconnectScheduled::Exhausted;
        };
        *self = Self::Waiting { at, attempt };
        ReconnectScheduled::Waiting { attempt, delay }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconnectScheduled {
    Unchanged,
    Exhausted,
    Waiting { attempt: u32, delay: Duration },
}
pub fn reconnect_delay(attempt: u32, base: Duration, max: Duration) -> Duration {
    let mut delay = base.min(max);
    if delay.is_zero() {
        return delay;
    }
    // A positive Duration is at least 1 ns, and Duration::MAX is below
    // 2^94 ns. At most 96 doublings reach the cap or overflow, even for
    // adversarial attempt counts.
    for _ in 0..attempt.saturating_sub(1).min(96) {
        delay = delay.checked_mul(2).unwrap_or(max).min(max);
        if delay == max {
            break;
        }
    }
    delay
}
/// Current backend adapter capability, not a negotiated server guarantee.
pub use opclook_backend::{AcquisitionCapabilities, AcquisitionError, acquisition_capabilities};
pub fn recommended_acquisition(protocol: Protocol) -> opclook_contract::model::AcquisitionConfig {
    opclook_backend::recommended_acquisition(protocol)
}

pub fn validate_acquisition(endpoint: &EndpointSpec) -> Result<(), AcquisitionError> {
    opclook_backend::validate_acquisition(endpoint)
}
pub fn supports_fresh_source(protocol: Protocol) -> bool {
    opclook_backend::supports_fresh_source(protocol)
}
pub fn request_timeout(operation: Operation, configured_secs: u64) -> Duration {
    let floor = match operation {
        Operation::Connect
        | Operation::Discover
        | Operation::SearchServers
        | Operation::SearchNetwork => constants::REQUEST_SLOW_MIN,
        _ => constants::REQUEST_MIN,
    };
    Duration::from_secs(configured_secs).max(floor)
}
#[derive(Clone, Debug)]
pub struct PendingRequest<C> {
    pub operation: Operation,
    pub sent: Instant,
    pub ctx: C,
    /// Product demands covered by this particular Read, not later requests.
    pub read_demands: Vec<ReadDemand>,
    /// Per-key terminal results produced by accepting this Read's reply.
    pub read_terminals: Vec<opclook_contract::model::ReadTerminal>,
    read_coverage: Vec<(ItemKey, Vec<opclook_contract::model::ReadNeed>)>,
    read_epoch: Option<RequestId>,
    write_readback_key: Option<ItemKey>,
}
impl<C> PendingRequest<C> {
    pub fn new(operation: Operation, sent: Instant, ctx: C) -> Self {
        Self {
            operation,
            sent,
            ctx,
            read_demands: Vec::new(),
            read_terminals: Vec::new(),
            read_coverage: Vec::new(),
            read_epoch: None,
            write_readback_key: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReadDemandKind {
    ExplicitRefresh,
    WriteReadback,
    RecoverySnapshot,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ReadDemand {
    pub key: ItemKey,
    pub kind: ReadDemandKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DemandError {
    SessionUnavailable,
    ItemNotRegistered,
    UnsupportedFreshness,
}

#[derive(Debug)]
pub enum DemandWriteError {
    Demand(DemandError),
    Dispatch(DispatchError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UaRecoveryPhase {
    Normal,
    Lost,
    Reconciling,
    Ready,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct SessionRuntime {
    pub endpoint: EndpointSpec,
    pub state: SessionState,
    pub reconnect: ReconnectState,
    pub subscription: SubscriptionState,
    /// SDK-reported state for the current UA connection epoch.
    pub ua_subscriptions: Vec<opclook_contract::model::UaSubscriptionParameters>,
    pub ua_recovery_phase: UaRecoveryPhase,
    pub ua_revision: u64,
    pub ua_history_incomplete: bool,
    pub ua_history_uncertain: bool,
    pub recovery_blocked: HashSet<ItemKey>,
    pub read_in_flight: bool,
    pub last_poll: Instant,
    pub last_test: Instant,
    pub items: HashMap<ItemKey, bool>,
    pending_initial: HashMap<ItemKey, Instant>,
    pending_demands: HashSet<ReadDemand>,
    reserved_demands: HashSet<ReadDemand>,
    reserved_initial: HashSet<ItemKey>,
    reserved_poll: HashSet<ItemKey>,
    poll_defer_since: HashMap<ItemKey, Instant>,
    used_keys: HashSet<ItemKey>,
    reserved_read: Option<(ReadPermit, Vec<ItemKey>)>,
    /// Last caller-provided legacy polling interval; never invent a default.
    last_due_interval: Option<Duration>,
    force_poll: bool,
    connection_id: Option<RequestId>,
}
impl SessionRuntime {
    fn reset_ua_scope(&mut self) {
        self.ua_recovery_phase = UaRecoveryPhase::Normal;
        self.ua_revision = 0;
        self.ua_history_incomplete = false;
        self.ua_history_uncertain = false;
        self.recovery_blocked.clear();
    }

    pub fn new(endpoint: EndpointSpec, now: Instant) -> Self {
        Self {
            endpoint,
            state: SessionState::Connecting,
            reconnect: ReconnectState::Idle,
            subscription: SubscriptionState::Inactive,
            ua_subscriptions: Vec::new(),
            ua_recovery_phase: UaRecoveryPhase::Normal,
            ua_revision: 0,
            ua_history_incomplete: false,
            ua_history_uncertain: false,
            recovery_blocked: HashSet::new(),
            read_in_flight: false,
            last_poll: now.checked_sub(Duration::from_secs(10)).unwrap_or(now),
            last_test: now,
            items: HashMap::new(),
            pending_initial: HashMap::new(),
            pending_demands: HashSet::new(),
            reserved_demands: HashSet::new(),
            reserved_initial: HashSet::new(),
            reserved_poll: HashSet::new(),
            poll_defer_since: HashMap::new(),
            used_keys: HashSet::new(),
            reserved_read: None,
            last_due_interval: None,
            force_poll: false,
            connection_id: None,
        }
    }
}
impl SessionRuntime {
    fn initial_due(
        &self,
        key: ItemKey,
        since: Instant,
        now: Instant,
        fallback: Option<Duration>,
    ) -> bool {
        if self.recovery_blocked.contains(&key) {
            return false;
        }
        let configured_poll = self
            .endpoint
            .acquisition
            .as_ref()
            .map(|c| c.update_mode == opclook_contract::model::UpdateMode::Poll);
        let wait_for_push = configured_poll.map(|poll| !poll).unwrap_or(
            !self.force_poll
                && self.endpoint.protocol == Protocol::OpcUa
                && !self.items.get(&key).copied().unwrap_or(false),
        );
        if !wait_for_push {
            return true;
        }
        let grace = self
            .revised_interval_for(key)
            .map(Duration::from_millis)
            .or_else(|| {
                self.endpoint
                    .acquisition
                    .as_ref()
                    .map(|c| Duration::from_millis(c.interval_ms))
            })
            .or(fallback);
        grace.is_none_or(|grace| now.saturating_duration_since(since) >= grace)
    }
    fn revised_interval_for(&self, key: ItemKey) -> Option<u64> {
        self.ua_subscriptions.iter().find_map(|sub| {
            let revised = sub.revised.as_ref().filter(|r| r.valid)?;
            sub.items
                .iter()
                .any(|item| item.key == key)
                .then_some(revised.publishing_ms)
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadPermit(u64);

pub enum Due {
    Read {
        session: SessionId,
        protocol: Protocol,
        keys: Vec<ItemKey>,
        permit: ReadPermit,
    },
    ReadUnavailable {
        session: SessionId,
    },
    Test {
        session: SessionId,
        protocol: Protocol,
    },
    Reconnect {
        session: SessionId,
    },
}
#[derive(Debug)]
pub enum DispatchError {
    InvalidActionPayload(opclook_contract::actions::ActionPayloadError),
    InvalidAcquisition(AcquisitionError),
    Exhausted,
    ReadBusy,
    ReadEmpty,
    ItemKeyReused,
    ItemKeyLimit,
    SessionLimit,
    ActionUnavailable(ActionUnavailable),
    Closed {
        request_id: RequestId,
        cause: SendError,
    },
}
/// A stable, localizable explanation of why a domain action cannot run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionUnavailable {
    RequestIdsExhausted,
    SessionMissing,
    SessionNotConnected,
    ProtocolMismatch,
    ReadPending,
}

pub struct RouteOptions {
    pub adapter: Option<AdapterInfo>,
    pub local_addresses: Vec<IpAddr>,
    pub deadline: Instant,
}
struct DeferredRoute {
    owner: Owner,
    request_id: RequestId,
    kind: CommandKind,
    ticket: RouteTicket,
}
#[derive(Clone, Default)]
struct PendingAdd {
    poll_only: bool,
    initial_seen: bool,
    /// Keep only status metadata, never a second copy of a potentially large value.
    initial_status: Option<opclook_contract::model::ReadTerminalStatus>,
}
pub struct Engine<C> {
    gateway: Gateway,
    next: Option<RequestId>,
    next_permit: Option<u64>,
    requests: HashMap<(Owner, RequestId), PendingRequest<C>>,
    pending_adds: HashMap<(Owner, RequestId), HashMap<ItemKey, PendingAdd>>,
    sessions: HashMap<SessionId, SessionRuntime>,
    auto_reconnect: bool,
    route_service: Option<RouteService>,
    deferred_routes: Vec<DeferredRoute>,
    session_routes: HashMap<SessionId, RouteTicket>,
    discovery_routes: HashMap<RequestId, RouteTicket>,
    route_events: VecDeque<Event>,
    invalidated: HashMap<SessionId, Vec<Operation>>,
    timeout_queued: HashSet<(Owner, RequestId)>,
    identity_cancelled: HashSet<(Owner, RequestId)>,
    read_terminal_sink: Option<
        Box<dyn FnMut(SessionId, Option<RequestId>, &opclook_contract::model::ReadTerminal) + Send>,
    >,
    #[cfg(test)]
    test_skip_dispatch: bool,
    #[cfg(test)]
    test_commands: std::cell::RefCell<Vec<Command>>,
    #[cfg(feature = "test-support")]
    test_dispatch_limit: std::cell::Cell<Option<usize>>,
    #[cfg(feature = "test-support")]
    test_dispatched: std::cell::RefCell<Vec<Command>>,
}
impl<C> Default for Engine<C> {
    fn default() -> Self {
        #[cfg(test)]
        let _ = opclook_backend::bff::ua::use_private_pki_dir();
        Self {
            gateway: Gateway::new(),
            next: Some(RequestId::FIRST),
            next_permit: Some(1),
            requests: HashMap::new(),
            pending_adds: HashMap::new(),
            sessions: HashMap::new(),
            auto_reconnect: false,
            route_service: None,
            deferred_routes: Vec::new(),
            session_routes: HashMap::new(),
            discovery_routes: HashMap::new(),
            route_events: VecDeque::new(),
            invalidated: HashMap::new(),
            timeout_queued: HashSet::new(),
            identity_cancelled: HashSet::new(),
            read_terminal_sink: None,
            #[cfg(test)]
            test_skip_dispatch: false,
            #[cfg(test)]
            test_commands: std::cell::RefCell::new(Vec::new()),
            #[cfg(feature = "test-support")]
            test_dispatch_limit: std::cell::Cell::new(None),
            #[cfg(feature = "test-support")]
            test_dispatched: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl<C> Drop for Engine<C> {
    fn drop(&mut self) {
        // Drain logical needs before fields are dropped. Gateway's existing
        // owner remains responsible for transport shutdown.
        let sessions: Vec<_> = self.sessions.keys().copied().collect();
        for session in sessions {
            self.cancel_reads(
                session,
                None,
                opclook_contract::model::ReadCancelReason::Cancelled,
            );
        }
    }
}
impl<C> Engine<C> {
    /// Observe read terminals synchronously on the calling thread. The sink is
    /// opt-in and is responsible for returning promptly. No event queue or
    /// retained history is created. Successful replies also expose the same
    /// terminals on the returned PendingRequest; these are two views of one
    /// terminal, not two operations. Without a sink lifecycle cancellations are
    /// not retained for later inspection.
    pub fn set_read_terminal_sink(
        &mut self,
        sink: Option<
            Box<
                dyn FnMut(SessionId, Option<RequestId>, &opclook_contract::model::ReadTerminal)
                    + Send,
            >,
        >,
    ) {
        self.read_terminal_sink = sink;
    }

    fn notify_read_terminals(
        &mut self,
        session: SessionId,
        epoch: Option<RequestId>,
        terminals: &[opclook_contract::model::ReadTerminal],
    ) {
        if let Some(sink) = &mut self.read_terminal_sink {
            for terminal in terminals {
                sink(session, epoch, terminal);
            }
        }
    }

    /// Cancel the matching registered needs once, including both pending and
    /// in-flight generations. Taking coverage makes late replies harmless.
    fn cancel_reads(
        &mut self,
        session: SessionId,
        selected: Option<&HashSet<ItemKey>>,
        reason: opclook_contract::model::ReadCancelReason,
    ) {
        use opclook_contract::model::{ReadNeed, ReadTerminal, ReadTerminalStatus};
        let matches = |key: &ItemKey| selected.is_none_or(|keys| keys.contains(key));
        let mut batches: Vec<(Option<RequestId>, Vec<ReadTerminal>)> = Vec::new();
        if let Some(view) = self.sessions.get_mut(&session) {
            let epoch = view.connection_id;
            let mut pending: HashMap<ItemKey, Vec<ReadNeed>> = HashMap::new();
            view.pending_initial.retain(|key, _| {
                if matches(key) {
                    pending.entry(*key).or_default().push(ReadNeed::Initial);
                    false
                } else {
                    true
                }
            });
            view.pending_demands.retain(|d| {
                if matches(&d.key) {
                    pending.entry(d.key).or_default().push(match d.kind {
                        ReadDemandKind::ExplicitRefresh => ReadNeed::ExplicitRefresh,
                        ReadDemandKind::WriteReadback => ReadNeed::WriteReadback,
                        ReadDemandKind::RecoverySnapshot => ReadNeed::RecoverySnapshot,
                    });
                    false
                } else {
                    true
                }
            });
            if !pending.is_empty() {
                batches.push((
                    epoch,
                    pending
                        .into_iter()
                        .map(|(key, needs)| ReadTerminal {
                            request_id: None,
                            key,
                            needs,
                            status: ReadTerminalStatus::Cancelled(reason),
                        })
                        .collect(),
                ));
            }
            let mut reserved: HashMap<ItemKey, Vec<ReadNeed>> = HashMap::new();
            view.reserved_initial.retain(|key| {
                if matches(key) {
                    reserved.entry(*key).or_default().push(ReadNeed::Initial);
                    false
                } else {
                    true
                }
            });
            view.reserved_poll.retain(|key| {
                if matches(key) {
                    reserved.entry(*key).or_default().push(ReadNeed::Poll);
                    false
                } else {
                    true
                }
            });
            view.reserved_demands.retain(|d| {
                if matches(&d.key) {
                    reserved.entry(d.key).or_default().push(match d.kind {
                        ReadDemandKind::ExplicitRefresh => ReadNeed::ExplicitRefresh,
                        ReadDemandKind::WriteReadback => ReadNeed::WriteReadback,
                        ReadDemandKind::RecoverySnapshot => ReadNeed::RecoverySnapshot,
                    });
                    false
                } else {
                    true
                }
            });
            if !reserved.is_empty() {
                batches.push((
                    epoch,
                    reserved
                        .into_iter()
                        .map(|(key, needs)| ReadTerminal {
                            request_id: None,
                            key,
                            needs,
                            status: ReadTerminalStatus::Cancelled(reason),
                        })
                        .collect(),
                ));
            }
        }
        for (&(owner, request_id), pending) in &mut self.requests {
            if owner != Owner::Session(session) || pending.operation != Operation::Read {
                continue;
            }
            let mut terminals = Vec::new();
            pending.read_coverage.retain(|(key, needs)| {
                if matches(key) {
                    terminals.push(ReadTerminal {
                        request_id: Some(request_id),
                        key: *key,
                        needs: needs.clone(),
                        status: ReadTerminalStatus::Cancelled(reason),
                    });
                    false
                } else {
                    true
                }
            });
            pending.read_demands.retain(|d| !matches(&d.key));
            if !terminals.is_empty() {
                batches.push((pending.read_epoch, terminals));
            }
        }
        for (epoch, terminals) in batches {
            self.notify_read_terminals(session, epoch, &terminals);
        }
    }

    fn unavailable_reservation(
        view: &mut SessionRuntime,
    ) -> Vec<opclook_contract::model::ReadTerminal> {
        use opclook_contract::model::{ReadNeed, ReadTerminal, ReadTerminalStatus};
        let mut needs: HashMap<ItemKey, Vec<ReadNeed>> = HashMap::new();
        for key in view.reserved_initial.drain() {
            needs.entry(key).or_default().push(ReadNeed::Initial);
        }
        for key in view.reserved_poll.drain() {
            needs.entry(key).or_default().push(ReadNeed::Poll);
        }
        for demand in view.reserved_demands.drain() {
            needs
                .entry(demand.key)
                .or_default()
                .push(match demand.kind {
                    ReadDemandKind::ExplicitRefresh => ReadNeed::ExplicitRefresh,
                    ReadDemandKind::WriteReadback => ReadNeed::WriteReadback,
                    ReadDemandKind::RecoverySnapshot => ReadNeed::RecoverySnapshot,
                });
        }
        needs
            .into_iter()
            .map(|(key, needs)| ReadTerminal {
                request_id: None,
                key,
                needs,
                status: ReadTerminalStatus::Unavailable,
            })
            .collect()
    }

    fn purge_read_requests(&mut self, session: SessionId) {
        let owner = Owner::Session(session);
        let cancelled: Vec<_> = self
            .requests
            .iter()
            .filter_map(|(&(candidate, id), pending)| {
                (candidate == owner && pending.operation == Operation::Read).then_some(id)
            })
            .collect();
        for id in cancelled {
            self.requests.remove(&(owner, id));
            self.timeout_queued.remove(&(owner, id));
        }
    }

    /// Enqueue DA discovery or connection without doing DNS or helper work on
    /// the caller thread. The engine owns the ticket through the operation.
    pub fn prepare_da(
        &mut self,
        owner: impl Into<Owner>,
        kind: CommandKind,
        ctx: C,
        options: RouteOptions,
    ) -> Result<RequestId, DaRouteError> {
        let owner = owner.into();
        if let CommandKind::Connect { endpoint } = &kind {
            validate_acquisition(endpoint).map_err(|e| {
                DaRouteError::HelperUnavailable(format!("invalid acquisition: {e:?}"))
            })?;
        }
        let host = match (&kind, owner) {
            (CommandKind::Connect { endpoint }, Owner::Session(_))
                if endpoint.protocol == Protocol::OpcDa =>
            {
                endpoint.host.clone()
            }
            (CommandKind::Discover { host }, Owner::Discovery) => host.clone(),
            _ => {
                return Err(DaRouteError::HelperUnavailable(
                    "invalid DA route operation".into(),
                ));
            }
        };
        let id = self.next.ok_or(DaRouteError::RequestIdsExhausted)?;
        if self.route_service.is_none() {
            self.route_service = Some(
                RouteService::new().map_err(|e| DaRouteError::HelperUnavailable(e.to_string()))?,
            );
        }
        let service = self
            .route_service
            .as_ref()
            .ok_or_else(|| DaRouteError::HelperUnavailable("route service unavailable".into()))?;
        let ticket = service.prepare(
            host,
            options.adapter,
            options.local_addresses,
            options.deadline,
        )?;
        self.next = id.successor();
        let operation = kind.operation();
        if let (CommandKind::Connect { endpoint }, Owner::Session(session)) = (&kind, owner) {
            self.cancel_reads(
                session,
                None,
                opclook_contract::model::ReadCancelReason::EpochChanged,
            );
            self.cancel_da_routes(session);
            self.requests.retain(|(sid, _), _| *sid != owner);
            self.pending_adds.retain(|(sid, _), _| *sid != owner);
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint.clone();
            view.connection_id = Some(id);
            view.reset_ua_scope();
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.ua_subscriptions.clear();
            view.items.clear();
            view.pending_initial.clear();
            view.pending_demands.clear();
            view.reserved_demands.clear();
            view.reserved_initial.clear();
            view.reserved_poll.clear();
            view.poll_defer_since.clear();
            view.reserved_read = None;
            view.used_keys.clear();
        }
        self.requests.insert(
            (owner, id),
            PendingRequest {
                operation,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx,
            },
        );
        self.deferred_routes.push(DeferredRoute {
            owner,
            request_id: id,
            kind,
            ticket,
        });
        Ok(id)
    }
    /// Advance route preparation; queue typed failures and dispatch successful
    /// commands through the same correlated event pipeline as backend replies.
    fn pump_da_routes(&mut self) {
        let mut index = 0;
        while index < self.deferred_routes.len() {
            let Some(result) = self.deferred_routes[index].ticket.result() else {
                index += 1;
                continue;
            };
            let deferred = self.deferred_routes.swap_remove(index);
            let operation = deferred.kind.operation();
            match result {
                Ok(()) => {
                    let result = self.gateway.send(Command {
                        request_id: Some(deferred.request_id),
                        owner: deferred.owner,
                        protocol: Protocol::OpcDa,
                        kind: deferred.kind,
                    });
                    match result {
                        Ok(()) if operation == Operation::Connect => {
                            if let Owner::Session(session) = deferred.owner {
                                self.session_routes.insert(session, deferred.ticket);
                            }
                        }
                        Ok(()) => {
                            self.discovery_routes
                                .insert(deferred.request_id, deferred.ticket);
                        }
                        Err(message) => self.route_events.push_back(Event {
                            owner: deferred.owner,
                            origin: Origin::Reply(deferred.request_id),
                            kind: EventKind::Error {
                                operation,
                                message: BackendMsg::SendFailed(message),
                            },
                        }),
                    }
                }
                Err(error) => self.route_events.push_back(Event {
                    owner: deferred.owner,
                    origin: Origin::Reply(deferred.request_id),
                    kind: EventKind::Error {
                        operation,
                        message: BackendMsg::DaRoute(error),
                    },
                }),
            }
        }
        let lost: Vec<_> = self
            .session_routes
            .iter()
            .filter_map(|(&session, ticket)| match ticket.result() {
                Some(Err(error)) => Some((session, error)),
                _ => None,
            })
            .collect();
        for (session, error) in lost {
            self.on_da_route_lost(session, error);
        }
    }
    fn on_da_route_lost(&mut self, session: SessionId, error: DaRouteError) {
        self.cancel_reads(
            session,
            None,
            opclook_contract::model::ReadCancelReason::Disconnected,
        );
        self.session_routes.remove(&session);
        let _ = self.gateway.send(Command {
            request_id: None,
            owner: Owner::Session(session),
            protocol: Protocol::OpcDa,
            kind: CommandKind::Disconnect,
        });
        let connecting = self
            .sessions
            .get(&session)
            .is_some_and(|view| view.state == SessionState::Connecting);
        let connect_request = if connecting {
            self.requests.iter().find_map(|(&(sid, id), pending)| {
                (sid == Owner::Session(session) && pending.operation == Operation::Connect)
                    .then_some(id)
            })
        } else {
            None
        };
        let keys: Vec<_> = self
            .requests
            .keys()
            .filter(|(sid, id)| *sid == Owner::Session(session) && Some(*id) != connect_request)
            .copied()
            .collect();
        let invalidated: Vec<_> = keys
            .into_iter()
            .filter_map(|key| self.requests.remove(&key).map(|pending| pending.operation))
            .collect();
        self.pending_adds
            .retain(|(sid, _), _| *sid != Owner::Session(session));
        self.invalidated
            .entry(session)
            .or_default()
            .extend(invalidated);
        let kind = if connecting {
            EventKind::Error {
                operation: Operation::Connect,
                message: BackendMsg::DaRoute(error),
            }
        } else {
            EventKind::Disconnected {
                reason: BackendMsg::DaRoute(error),
            }
        };
        self.route_events.push_back(Event {
            owner: Owner::Session(session),
            origin: connect_request
                .map(Origin::Reply)
                .or_else(|| {
                    self.sessions
                        .get(&session)
                        .and_then(|view| view.connection_id)
                        .map(Origin::Session)
                })
                .unwrap_or(Origin::Unsolicited),
            kind,
        });
    }

    pub fn cancel_da_routes(&mut self, owner: impl Into<Owner>) {
        let owner = owner.into();
        self.route_events.retain(|event| event.owner != owner);
        self.deferred_routes.retain(|item| item.owner != owner);
        match owner {
            Owner::Discovery => self.discovery_routes.clear(),
            Owner::Session(session) => {
                self.session_routes.remove(&session);
            }
        }
    }
    pub fn da_route_status(&self) -> (DaRouteStatus, Option<DaRouteError>) {
        self.route_service
            .as_ref()
            .map(RouteService::status)
            .unwrap_or((DaRouteStatus::Idle, None))
    }
    pub fn has_da_route(&self, session: SessionId) -> bool {
        self.session_routes.contains_key(&session)
            || self
                .deferred_routes
                .iter()
                .any(|item| item.owner == Owner::Session(session))
    }
    fn dispatch(&self, command: Command) -> Result<(), SendError> {
        #[cfg(feature = "test-support")]
        if let Some(left) = self.test_dispatch_limit.get() {
            if left == 0 {
                return Err(SendError::WorkerClosed);
            }
            self.test_dispatch_limit.set(Some(left - 1));
            self.test_dispatched.borrow_mut().push(command);
            return Ok(());
        }
        #[cfg(test)]
        if self.test_skip_dispatch {
            self.test_commands.borrow_mut().push(command);
            return Ok(());
        }
        self.gateway.send(command)
    }
    /// Tear down an old transport during reconnection without allocating a request ID.
    pub fn disconnect_untracked(
        &mut self,
        session: SessionId,
        protocol: Protocol,
    ) -> Result<(), SendError> {
        self.dispatch(Command {
            request_id: None,
            owner: Owner::Session(session),
            protocol,
            kind: CommandKind::Disconnect,
        })?;
        self.cancel_reads(
            session,
            None,
            opclook_contract::model::ReadCancelReason::EpochChanged,
        );
        self.purge_read_requests(session);
        self.cancel_da_routes(session);
        if let Some(view) = self.sessions.get_mut(&session) {
            view.connection_id = None;
            view.reserved_read = None;
            view.read_in_flight = false;
        }
        Ok(())
    }
    pub fn has_request_capacity(&self) -> bool {
        self.next.is_some()
    }
    /// A pure availability check. It neither allocates request IDs nor dispatches.
    pub fn action_unavailable(
        &self,
        action: &opclook_contract::actions::DomainAction,
    ) -> Option<ActionUnavailable> {
        if !self.has_request_capacity() {
            return Some(ActionUnavailable::RequestIdsExhausted);
        }
        let (session, protocol, write) = match action {
            opclook_contract::actions::DomainAction::Browse {
                session, protocol, ..
            } => (*session, *protocol, false),
            opclook_contract::actions::DomainAction::Write {
                session, protocol, ..
            } => (*session, *protocol, true),
        };
        self.session_action_unavailable(session, protocol, write)
            .or_else(|| {
                (!write && self.browse_deferred_at(session, Instant::now()))
                    .then_some(ActionUnavailable::ReadPending)
            })
    }

    /// A Browse must yield to an occupied Read slot or an already due strong
    /// demand. Ordinary periodic polls do not prevent browsing.
    pub fn browse_deferred_at(&self, session: SessionId, now: Instant) -> bool {
        let Some(view) = self.sessions.get(&session) else {
            return false;
        };
        if view.state != SessionState::Connected || view.connection_id.is_none() {
            return false;
        }
        view.read_in_flight
            || !view.pending_demands.is_empty()
            || view
                .pending_initial
                .iter()
                .any(|(&key, &since)| view.initial_due(key, since, now, view.last_due_interval))
    }

    pub fn write_session_unavailable(
        &self,
        session: SessionId,
        protocol: Protocol,
    ) -> Option<ActionUnavailable> {
        if !self.has_request_capacity() {
            return Some(ActionUnavailable::RequestIdsExhausted);
        }
        self.session_action_unavailable(session, protocol, true)
    }

    fn session_action_unavailable(
        &self,
        session: SessionId,
        protocol: Protocol,
        write: bool,
    ) -> Option<ActionUnavailable> {
        let Some(runtime) = self.sessions.get(&session) else {
            return Some(ActionUnavailable::SessionMissing);
        };
        if runtime.endpoint.protocol != protocol {
            return Some(ActionUnavailable::ProtocolMismatch);
        }
        // Browses can be queued while connecting; writes cannot.
        if runtime.state == SessionState::Disconnected
            || (write
                && (runtime.state != SessionState::Connected
                    || runtime.connection_id.is_none()
                    || matches!(
                        runtime.ua_recovery_phase,
                        UaRecoveryPhase::Lost
                            | UaRecoveryPhase::Reconciling
                            | UaRecoveryPhase::Stopped
                    )))
        {
            return Some(ActionUnavailable::SessionNotConnected);
        }
        None
    }

    /// Execute a domain action through the existing request-correlation path.
    pub fn execute_action(
        &mut self,
        action: opclook_contract::actions::DomainAction,
        ctx: C,
    ) -> Result<RequestId, DispatchError> {
        if let Some(reason) = self.action_unavailable(&action) {
            return Err(match reason {
                ActionUnavailable::RequestIdsExhausted => DispatchError::Exhausted,
                reason => DispatchError::ActionUnavailable(reason),
            });
        }
        match action {
            opclook_contract::actions::DomainAction::Browse {
                session,
                protocol,
                node_id,
                filter,
                view_id,
            } => self.send(
                session,
                protocol,
                CommandKind::Browse {
                    node_id,
                    filter,
                    view_id,
                },
                ctx,
            ),
            opclook_contract::actions::DomainAction::Write {
                session,
                protocol,
                request,
            } => self.send(session, protocol, CommandKind::Write { request }, ctx),
        }
    }

    pub fn send(
        &mut self,
        owner: impl Into<Owner>,
        protocol: Protocol,
        kind: CommandKind,
        ctx: C,
    ) -> Result<RequestId, DispatchError> {
        self.send_inner(owner.into(), protocol, kind, ctx, None)
    }

    /// Queue a one-shot demand on the current registration; equal pending demands coalesce.
    pub fn request_refresh(&mut self, session: SessionId, key: ItemKey) -> Result<(), DemandError> {
        self.enqueue_demand(session, key, ReadDemandKind::ExplicitRefresh)
    }
    /// Opt in before dispatch. Only the matching accepted Wrote reply may create
    /// read-back demand; stale replies cannot cross a connection epoch.
    pub fn send_write_with_readback(
        &mut self,
        session: SessionId,
        protocol: Protocol,
        request: opclook_contract::model::WriteRequest,
        ctx: C,
    ) -> Result<RequestId, DemandWriteError> {
        let view = self
            .sessions
            .get(&session)
            .ok_or(DemandWriteError::Demand(DemandError::SessionUnavailable))?;
        if view.state != SessionState::Connected
            || view.connection_id.is_none()
            || matches!(
                view.ua_recovery_phase,
                UaRecoveryPhase::Lost | UaRecoveryPhase::Reconciling | UaRecoveryPhase::Stopped
            )
        {
            return Err(DemandWriteError::Demand(DemandError::SessionUnavailable));
        }
        if !supports_fresh_source(view.endpoint.protocol) || view.endpoint.protocol != protocol {
            return Err(DemandWriteError::Demand(DemandError::UnsupportedFreshness));
        }
        if !view.items.contains_key(&request.key) || view.recovery_blocked.contains(&request.key) {
            return Err(DemandWriteError::Demand(DemandError::ItemNotRegistered));
        }
        let key = request.key;
        let id = self
            .send(session, protocol, CommandKind::Write { request }, ctx)
            .map_err(DemandWriteError::Dispatch)?;
        if let Some(pending) = self.requests.get_mut(&(Owner::Session(session), id)) {
            pending.write_readback_key = Some(key);
        }
        Ok(id)
    }
    fn enqueue_demand(
        &mut self,
        session: SessionId,
        key: ItemKey,
        kind: ReadDemandKind,
    ) -> Result<(), DemandError> {
        let view = self
            .sessions
            .get_mut(&session)
            .ok_or(DemandError::SessionUnavailable)?;
        if view.state != SessionState::Connected
            || view.connection_id.is_none()
            || matches!(
                view.ua_recovery_phase,
                UaRecoveryPhase::Lost | UaRecoveryPhase::Reconciling | UaRecoveryPhase::Stopped
            )
        {
            return Err(DemandError::SessionUnavailable);
        }
        if !supports_fresh_source(view.endpoint.protocol) {
            return Err(DemandError::UnsupportedFreshness);
        }
        if !view.items.contains_key(&key) || view.recovery_blocked.contains(&key) {
            return Err(DemandError::ItemNotRegistered);
        }
        view.pending_demands.insert(ReadDemand { key, kind });
        Ok(())
    }

    /// Whether this live reservation covers the requested product demand.
    pub fn reservation_covers(
        &self,
        session: SessionId,
        permit: ReadPermit,
        demand: ReadDemand,
    ) -> bool {
        self.sessions.get(&session).is_some_and(|view| {
            view.reserved_read
                .as_ref()
                .is_some_and(|(reserved, _)| *reserved == permit)
                && view.reserved_demands.contains(&demand)
        })
    }

    pub fn send_due_read(
        &mut self,
        session: SessionId,
        protocol: Protocol,
        keys: Vec<ItemKey>,
        permit: ReadPermit,
        ctx: C,
    ) -> Result<RequestId, DispatchError> {
        self.send_inner(
            session.into(),
            protocol,
            CommandKind::Read { keys },
            ctx,
            Some(permit),
        )
    }

    fn send_inner(
        &mut self,
        owner: Owner,
        protocol: Protocol,
        kind: CommandKind,
        ctx: C,
        permit: Option<ReadPermit>,
    ) -> Result<RequestId, DispatchError> {
        // Only this engine may turn a validated reservation into a cached
        // transport read. Public callers cannot bypass reservation checks.
        if matches!(
            &kind,
            CommandKind::ReadCached { .. } | CommandKind::ReadFresh { .. }
        ) {
            return Err(DispatchError::ReadBusy);
        }
        if let CommandKind::Connect { endpoint } = &kind {
            validate_acquisition(endpoint).map_err(DispatchError::InvalidAcquisition)?;
        }
        if let (Owner::Session(session), CommandKind::Read { keys }) = (owner, &kind)
            && self.sessions.get(&session).is_some_and(|view| {
                matches!(
                    view.ua_recovery_phase,
                    UaRecoveryPhase::Lost | UaRecoveryPhase::Reconciling | UaRecoveryPhase::Stopped
                ) || keys.iter().any(|key| view.recovery_blocked.contains(key))
            })
        {
            return Err(DispatchError::ReadBusy);
        }
        if let (Owner::Session(session), CommandKind::Write { request }) = (owner, &kind)
            && self
                .sessions
                .get(&session)
                .is_some_and(|view| view.recovery_blocked.contains(&request.key))
        {
            return Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionNotConnected,
            ));
        }
        if matches!(&kind, CommandKind::Write { .. }) {
            let reason = match owner {
                Owner::Session(session) => self.session_action_unavailable(session, protocol, true),
                Owner::Discovery => Some(ActionUnavailable::SessionMissing),
            };
            if let Some(reason) = reason {
                return Err(DispatchError::ActionUnavailable(reason));
            }
        }
        let session = match owner {
            Owner::Session(session) => Some(session),
            Owner::Discovery => None,
        };
        if matches!(&kind, CommandKind::Connect { endpoint } if endpoint.protocol == Protocol::OpcUa)
            && !session
                .and_then(|session| self.sessions.get(&session))
                .is_some_and(|view| view.endpoint.protocol == Protocol::OpcUa)
            && self
                .sessions
                .values()
                .filter(|view| view.endpoint.protocol == Protocol::OpcUa)
                .count()
                >= constants::MAX_UA_SESSIONS
        {
            return Err(DispatchError::SessionLimit);
        }
        let endpoint = match &kind {
            CommandKind::Connect { endpoint } => Some(endpoint.clone()),
            _ => None,
        };
        let added = match &kind {
            CommandKind::AddItems { items } => {
                items.iter().map(|item| item.key).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        };
        let removed = match &kind {
            CommandKind::RemoveItems { keys } => keys.clone(),
            _ => Vec::new(),
        };
        if let Some(view) = session.and_then(|session| self.sessions.get(&session))
            && !added.is_empty()
        {
            if added.len()
                > constants::MAX_ITEM_KEYS_PER_CONNECTION.saturating_sub(view.used_keys.len())
            {
                return Err(DispatchError::ItemKeyLimit);
            }
            let unique: HashSet<_> = added.iter().copied().collect();
            if unique.len() != added.len()
                || added
                    .iter()
                    .any(|key| view.used_keys.contains(key) || view.items.contains_key(key))
            {
                return Err(DispatchError::ItemKeyReused);
            }
        }
        if let Some(permit) = permit {
            let valid = match (&kind, session.and_then(|id| self.sessions.get(&id))) {
                (CommandKind::Read { keys }, Some(view)) => {
                    let mut keys = keys.clone();
                    keys.sort_unstable();
                    keys.dedup();
                    view.state == SessionState::Connected
                        && view.endpoint.protocol == protocol
                        && view.reserved_read.as_ref() == Some(&(permit, keys))
                }
                _ => false,
            };
            if !valid {
                return Err(DispatchError::ReadBusy);
            }
        }
        let mut covered_demands = Vec::new();
        let mut read_coverage = Vec::new();
        let Some(id) = self.next else {
            if matches!(&kind, CommandKind::Read { .. }) {
                let pending_read = self.requests.iter().any(|(&(sid, _), pending)| {
                    sid == owner && pending.operation == Operation::Read
                });
                if let Some(view) = session.and_then(|session| self.sessions.get_mut(&session)) {
                    let epoch = view.connection_id;
                    let terminals = if permit.is_some() {
                        view.reserved_read = None;
                        Self::unavailable_reservation(view)
                    } else {
                        Vec::new()
                    };
                    view.read_in_flight = pending_read || view.reserved_read.is_some();
                    if let Some(session) = session {
                        self.notify_read_terminals(session, epoch, &terminals);
                    }
                }
            }
            return Err(DispatchError::Exhausted);
        };
        if matches!(&kind, CommandKind::Browse { .. })
            && let Some(session) = session
            && self.browse_deferred_at(session, Instant::now())
        {
            return Err(DispatchError::ActionUnavailable(
                ActionUnavailable::ReadPending,
            ));
        }
        match &kind {
            CommandKind::Browse {
                node_id, view_id, ..
            } => opclook_contract::actions::validate_browse_payload(node_id, view_id.as_deref())
                .map_err(DispatchError::InvalidActionPayload)?,
            CommandKind::Write { request } => {
                opclook_contract::actions::validate_write_payload(&request.value)
                    .map_err(DispatchError::InvalidActionPayload)?
            }
            _ => {}
        }
        if let CommandKind::Read { keys } = &kind {
            if keys.is_empty() {
                return Err(DispatchError::ReadEmpty);
            }
            let Some(view) = session.and_then(|id| self.sessions.get(&id)) else {
                return Err(DispatchError::ActionUnavailable(
                    ActionUnavailable::SessionMissing,
                ));
            };
            if view.endpoint.protocol != protocol {
                return Err(DispatchError::ActionUnavailable(
                    ActionUnavailable::ProtocolMismatch,
                ));
            }
            if view.state != SessionState::Connected {
                return Err(DispatchError::ActionUnavailable(
                    ActionUnavailable::SessionNotConnected,
                ));
            }
        }
        if let (Some(session), CommandKind::Read { keys }) = (session, &kind)
            && let Some(view) = self.sessions.get_mut(&session)
        {
            if let Some(permit) = permit {
                let mut requested = keys.clone();
                requested.sort_unstable();
                requested.dedup();
                if view.reserved_read.as_ref() != Some(&(permit, requested.clone())) {
                    return Err(DispatchError::ReadBusy);
                }
                if requested.iter().any(|key| !view.items.contains_key(key)) {
                    for key in &requested {
                        if view.items.contains_key(key) {
                            view.pending_initial
                                .entry(*key)
                                .or_insert_with(Instant::now);
                        }
                    }
                    view.pending_demands.extend(
                        view.reserved_demands
                            .drain()
                            .filter(|d| view.items.contains_key(&d.key)),
                    );
                    view.reserved_initial.clear();
                    view.reserved_poll.clear();
                    view.read_in_flight = false;
                    view.reserved_read = None;
                    return Err(DispatchError::ReadBusy);
                }
                covered_demands.extend(view.reserved_demands.drain());
                let mut demands_by_key: HashMap<ItemKey, Vec<opclook_contract::model::ReadNeed>> =
                    HashMap::new();
                for demand in &covered_demands {
                    demands_by_key
                        .entry(demand.key)
                        .or_default()
                        .push(match demand.kind {
                            ReadDemandKind::ExplicitRefresh => {
                                opclook_contract::model::ReadNeed::ExplicitRefresh
                            }
                            ReadDemandKind::WriteReadback => {
                                opclook_contract::model::ReadNeed::WriteReadback
                            }
                            ReadDemandKind::RecoverySnapshot => {
                                opclook_contract::model::ReadNeed::RecoverySnapshot
                            }
                        });
                }
                for &key in &requested {
                    let mut needs = Vec::new();
                    if view.reserved_initial.remove(&key) {
                        needs.push(opclook_contract::model::ReadNeed::Initial);
                    }
                    if view.reserved_poll.remove(&key) {
                        needs.push(opclook_contract::model::ReadNeed::Poll);
                    }
                    if let Some(demands) = demands_by_key.remove(&key) {
                        needs.extend(demands);
                    }
                    read_coverage.push((key, needs));
                }
                view.reserved_read = None;
            } else if view.read_in_flight {
                return Err(DispatchError::ReadBusy);
            } else {
                let mut unique = keys.clone();
                unique.sort_unstable();
                unique.dedup();
                for key in unique {
                    let mut needs = vec![opclook_contract::model::ReadNeed::Direct];
                    if view.pending_initial.remove(&key).is_some() {
                        needs.push(opclook_contract::model::ReadNeed::Initial);
                    }
                    read_coverage.push((key, needs));
                }
            }
        } else if permit.is_some() {
            return Err(DispatchError::ReadBusy);
        }
        if matches!(&kind, CommandKind::Connect { .. } | CommandKind::Disconnect) {
            if let Some(session) = session {
                let reason = if matches!(&kind, CommandKind::Connect { .. }) {
                    opclook_contract::model::ReadCancelReason::EpochChanged
                } else {
                    opclook_contract::model::ReadCancelReason::Disconnected
                };
                self.cancel_reads(session, None, reason);
            }
            self.cancel_da_routes(owner);
            self.timeout_queued.retain(|(id, _)| *id != owner);
            self.identity_cancelled.retain(|(id, _)| *id != owner);
            self.requests.retain(|(id, _), _| *id != owner);
            self.pending_adds.retain(|(id, _), _| *id != owner);
        }
        self.next = id.successor();
        // Coalesced explicit refresh/read-back is stronger than the polling
        // profile, so any covered demand forces maxAge=0 for the whole Read.
        // Subscription snapshots use only valid SDK-reported per-item rates;
        // when any rate is unknown, the whole batch stays fresh.
        let cached_age = if permit.is_some()
            && covered_demands.is_empty()
            && (protocol == Protocol::OpcUa || opclook_backend::supports_fresh_source(protocol))
        {
            session
                .and_then(|id| self.sessions.get(&id))
                .and_then(|view| {
                    let config = view.endpoint.acquisition.as_ref()?;
                    if config.poll_freshness != opclook_contract::model::PollFreshness::CacheAllowed
                    {
                        return None;
                    }
                    let age = match config.update_mode {
                        opclook_contract::model::UpdateMode::Poll => config.interval_ms,
                        opclook_contract::model::UpdateMode::Subscription => {
                            let CommandKind::Read { keys } = &kind else {
                                return None;
                            };
                            keys.iter()
                                .map(|key| view.revised_interval_for(*key))
                                .collect::<Option<Vec<_>>>()?
                                .into_iter()
                                .min()?
                        }
                    };
                    u32::try_from(age).ok()
                })
        } else {
            None
        };
        // Unqualified UA Read already requests source freshness. DA keeps
        // its historical cache default, so supported explicit fresh goals
        // need a transport command that selects the SDK device source.
        // A legacy profile-less periodic DA read keeps its cache baseline.
        let fresh_goal = permit.is_none()
            || !covered_demands.is_empty()
            || session
                .and_then(|id| self.sessions.get(&id))
                .and_then(|view| view.endpoint.acquisition.as_ref())
                .is_some_and(|config| {
                    config.poll_freshness == opclook_contract::model::PollFreshness::FreshSource
                });
        let kind = match (kind, cached_age) {
            (CommandKind::Read { keys }, Some(max_age_ms)) => {
                CommandKind::ReadCached { keys, max_age_ms }
            }
            (CommandKind::Read { keys }, None)
                if protocol == Protocol::OpcDa
                    && fresh_goal
                    && opclook_backend::supports_fresh_source(protocol) =>
            {
                CommandKind::ReadFresh { keys }
            }
            (kind, _) => kind,
        };
        let operation = kind.operation();
        let profile_readback_key = match (&kind, session.and_then(|id| self.sessions.get(&id))) {
            (CommandKind::Write { request }, Some(view))
                if view
                    .endpoint
                    .acquisition
                    .as_ref()
                    .is_some_and(|c| c.read_after_write) =>
            {
                Some(request.key)
            }
            _ => None,
        };
        if !matches!(
            &kind,
            CommandKind::ScanUaNetwork { .. } | CommandKind::CancelUaScan { .. }
        ) {
            self.requests.insert(
                (owner, id),
                PendingRequest {
                    operation,
                    sent: Instant::now(),
                    read_demands: covered_demands,
                    read_terminals: Vec::new(),
                    read_coverage,
                    read_epoch: session
                        .and_then(|id| self.sessions.get(&id))
                        .and_then(|view| view.connection_id),
                    write_readback_key: profile_readback_key,
                    ctx,
                },
            );
        }
        if let (Some(endpoint), Some(session)) = (endpoint, session) {
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint;
            view.connection_id = Some(id);
            view.reset_ua_scope();
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.ua_subscriptions.clear();
            view.items.clear();
            view.pending_initial.clear();
            view.pending_demands.clear();
            view.reserved_demands.clear();
            view.reserved_initial.clear();
            view.reserved_poll.clear();
            view.poll_defer_since.clear();
            view.reserved_read = None;
            view.used_keys.clear();
        }
        if let Err(cause) = self.dispatch(Command {
            request_id: Some(id),
            owner,
            protocol,
            kind,
        }) {
            if let Some(pending) = self.requests.remove(&(owner, id))
                && let Some(session) = session
            {
                let terminals: Vec<_> = pending
                    .read_coverage
                    .into_iter()
                    .map(|(key, needs)| opclook_contract::model::ReadTerminal {
                        request_id: Some(id),
                        key,
                        needs,
                        status: opclook_contract::model::ReadTerminalStatus::DispatchFailed(cause),
                    })
                    .collect();
                self.notify_read_terminals(session, pending.read_epoch, &terminals);
            }
            let affected: Vec<_> = self
                .sessions
                .iter()
                .filter_map(|(&id, view)| (view.endpoint.protocol == protocol).then_some(id))
                .collect();
            for id in affected {
                self.cancel_reads(
                    id,
                    None,
                    opclook_contract::model::ReadCancelReason::Disconnected,
                );
                self.purge_read_requests(id);
            }
            for view in self
                .sessions
                .values_mut()
                .filter(|view| view.endpoint.protocol == protocol)
            {
                if operation == Operation::Connect && view.connection_id == Some(id) {
                    view.connection_id = None;
                }
                view.state = SessionState::Disconnected;
                view.reconnect = ReconnectState::Stopped;
                view.read_in_flight = false;
                view.reserved_read = None;
                view.subscription = SubscriptionState::Inactive;
                view.ua_subscriptions.clear();
            }
            return Err(DispatchError::Closed {
                request_id: id,
                cause,
            });
        }
        if operation == Operation::Disconnect {
            if let Some(view) = session.and_then(|id| self.sessions.get_mut(&id)) {
                view.connection_id = None;
                view.pending_initial.clear();
                view.pending_demands.clear();
                view.reserved_demands.clear();
                view.reserved_initial.clear();
                view.reserved_poll.clear();
                view.poll_defer_since.clear();
                view.reserved_read = None;
                view.read_in_flight = false;
            }
        }
        if !added.is_empty() {
            if let Some(view) = session.and_then(|session| self.sessions.get_mut(&session)) {
                view.used_keys.extend(added.iter().copied());
            }
            self.pending_adds.insert(
                (owner, id),
                added
                    .into_iter()
                    .map(|key| (key, PendingAdd::default()))
                    .collect(),
            );
        }
        if let Some(session) = session
            && !removed.is_empty()
        {
            let keys: HashSet<ItemKey> = removed.iter().copied().collect();
            self.cancel_reads(
                session,
                Some(&keys),
                opclook_contract::model::ReadCancelReason::Removed,
            );
        }
        for (&(sid, _), pending) in &mut self.pending_adds {
            if sid == owner {
                for key in &removed {
                    pending.remove(key);
                }
            }
        }
        if let Some(view) = session.and_then(|session| self.sessions.get_mut(&session)) {
            for key in removed {
                view.items.remove(&key);
                view.pending_initial.remove(&key);
                view.pending_demands.retain(|d| d.key != key);
                view.reserved_demands.retain(|d| d.key != key);
                view.poll_defer_since.remove(&key);
            }
            if operation == Operation::Read {
                view.read_in_flight = true;
                view.last_poll = Instant::now();
            }
        }
        Ok(id)
    }
    pub fn accept(&mut self, event: &Event) -> Result<Option<PendingRequest<C>>, ()> {
        self.accept_at(event, Instant::now())
    }
    pub fn accept_at(
        &mut self,
        event: &Event,
        now: Instant,
    ) -> Result<Option<PendingRequest<C>>, ()> {
        if matches!(
            event.kind,
            EventKind::UaItemMetadata { .. } | EventKind::UaNotificationFacts { .. }
        ) && !matches!(event.origin, Origin::UaSession { .. })
        {
            return Err(());
        }
        if matches!(
            event.kind,
            EventKind::UaRecovery { .. } | EventKind::UaSubscriptionParameters { .. }
        ) && !matches!(event.origin, Origin::Session(_) | Origin::UaSession { .. })
        {
            return Err(());
        }
        // An SDK loop can stop before its Connect reply reaches this thread.
        // Never publish that stale success or consume its pending timeout.
        if let (Owner::Session(session), Origin::Reply(id), EventKind::Connected { .. }) =
            (event.owner, event.origin, &event.kind)
            && self.sessions.get(&session).is_some_and(|view| {
                view.connection_id == Some(id) && view.ua_recovery_phase == UaRecoveryPhase::Stopped
            })
        {
            return Err(());
        }
        let pending = match event.origin {
            Origin::Unsolicited | Origin::Session(_) | Origin::UaSession { .. } => None,
            Origin::Reply(id) => {
                // Once the deadline is queued, a racing transport reply cannot
                // take its correlation slot ahead of the synthetic timeout.
                if (self.timeout_queued.contains(&(event.owner, id))
                    && !matches!(
                        &event.kind,
                        EventKind::Error {
                            message: BackendMsg::RequestTimeout { .. },
                            ..
                        }
                    ))
                    || (self.identity_cancelled.contains(&(event.owner, id))
                        && !matches!(
                            &event.kind,
                            EventKind::Error {
                                message: BackendMsg::UaIdentityChanged { .. },
                                ..
                            }
                        ))
                {
                    return Err(());
                }
                let mut pending = self.requests.remove(&(event.owner, id)).ok_or(())?;
                let synthetic_timeout = self.timeout_queued.remove(&(event.owner, id));
                self.identity_cancelled.remove(&(event.owner, id));
                if pending.operation == Operation::Read {
                    use opclook_contract::model::{ReadTerminal, ReadTerminalStatus};
                    // Index only requested keys; duplicate entries are a
                    // malformed reply, never an arbitrarily chosen success.
                    let expected: HashSet<_> =
                        pending.read_coverage.iter().map(|(key, _)| *key).collect();
                    let mut by_key = HashMap::new();
                    if let EventKind::Values { values } = &event.kind {
                        for (key, value) in values {
                            if expected.contains(key) {
                                by_key
                                    .entry(*key)
                                    .and_modify(
                                        |v: &mut Option<&opclook_contract::model::DataValue>| {
                                            *v = None
                                        },
                                    )
                                    .or_insert(Some(value));
                            }
                        }
                    }
                    pending.read_terminals = pending
                        .read_coverage
                        .iter()
                        .map(|(key, needs)| {
                            let status = match &event.kind {
                                EventKind::Values { .. } => match by_key.get(key) {
                                    Some(Some(value)) => {
                                        ReadTerminalStatus::Received((*value).clone())
                                    }
                                    None => ReadTerminalStatus::MissingResult,
                                    Some(None) => ReadTerminalStatus::UnexpectedReply,
                                },
                                EventKind::Error {
                                    message: BackendMsg::RequestTimeout { seconds },
                                    ..
                                } if synthetic_timeout => {
                                    ReadTerminalStatus::Timeout { seconds: *seconds }
                                }
                                EventKind::Error { message, .. } => {
                                    ReadTerminalStatus::ServiceError(message.clone())
                                }
                                _ => ReadTerminalStatus::UnexpectedReply,
                            };
                            ReadTerminal {
                                request_id: Some(id),
                                key: *key,
                                needs: needs.clone(),
                                status,
                            }
                        })
                        .collect();
                }
                self.deferred_routes
                    .retain(|item| !(item.owner == event.owner && item.request_id == id));
                self.discovery_routes.remove(&id);
                Some(pending)
            }
        };
        let Owner::Session(session) = event.owner else {
            return Ok(pending);
        };
        if matches!(event.kind, EventKind::UaSubscriptionParameters { .. })
            && !matches!(event.origin, Origin::Session(_) | Origin::UaSession { .. })
        {
            return Err(());
        }
        // Session state changes require the identity of the Connect that created
        // the producer. Untagged session diagnostics remain visible.
        let affects_session = !matches!(event.kind, EventKind::Log { .. });
        if matches!(event.origin, Origin::Session(_) | Origin::UaSession { .. })
            && matches!(event.kind, EventKind::Values { .. })
            && self
                .sessions
                .get(&session)
                .and_then(|view| view.endpoint.acquisition.as_ref())
                .is_some_and(|config| {
                    config.update_mode == opclook_contract::model::UpdateMode::Poll
                })
        {
            return Err(());
        }

        match event.origin {
            Origin::UaSession {
                connection,
                revision,
            } => {
                let view = self.sessions.get_mut(&session).ok_or(())?;
                let transition = matches!(
                    &event.kind,
                    EventKind::UaRecovery {
                        state: opclook_contract::model::UaRecoveryState::ConnectionLost { .. }
                            | opclook_contract::model::UaRecoveryState::Reconnected { .. }
                            | opclook_contract::model::UaRecoveryState::Stopped { .. }
                    }
                );
                let early_reconciled = view.ua_recovery_phase == UaRecoveryPhase::Reconciling
                    && matches!(
                        &event.kind,
                        EventKind::UaRecovery {
                            state: opclook_contract::model::UaRecoveryState::Reconciled { .. }
                        }
                    );
                if view.endpoint.protocol != Protocol::OpcUa
                    || view.connection_id != Some(connection)
                    || !(view.state == SessionState::Connected
                        || (view.state == SessionState::Connecting
                            && (transition || early_reconciled)))
                {
                    return Err(());
                }
                if transition {
                    let legal = match &event.kind {
                        EventKind::UaRecovery {
                            state: opclook_contract::model::UaRecoveryState::ConnectionLost { .. },
                        } => view.ua_recovery_phase != UaRecoveryPhase::Stopped,
                        EventKind::UaRecovery {
                            state: opclook_contract::model::UaRecoveryState::Reconnected { .. },
                        } => view.ua_recovery_phase == UaRecoveryPhase::Lost,
                        EventKind::UaRecovery {
                            state: opclook_contract::model::UaRecoveryState::Stopped { .. },
                        } => true,
                        _ => false,
                    };
                    if !legal {
                        return Err(());
                    }
                    if revision <= view.ua_revision {
                        return Err(());
                    }
                    view.ua_revision = revision;
                } else if revision != view.ua_revision {
                    return Err(());
                }
                if let EventKind::UaItemMetadata { facts } = &event.kind {
                    if !matches!(
                        view.ua_recovery_phase,
                        UaRecoveryPhase::Normal | UaRecoveryPhase::Ready
                    ) || (!facts.is_empty()
                        && !facts.iter().any(|fact| {
                            view.items.contains_key(&fact.key)
                                && !view.recovery_blocked.contains(&fact.key)
                        }))
                    {
                        return Err(());
                    }
                }
            }
            Origin::Session(id) => {
                if !self.sessions.get(&session).is_some_and(|view| {
                    view.connection_id == Some(id) && view.state == SessionState::Connected
                }) {
                    return Err(());
                }
            }
            Origin::Unsolicited if affects_session => return Err(()),
            _ => {}
        }
        if let EventKind::UaNotificationFacts { evidence, .. } = &event.kind
            && let Some(view) = self.sessions.get_mut(&session)
        {
            match evidence {
                opclook_contract::model::UaNotificationEvidence::SequenceGap { .. }
                | opclook_contract::model::UaNotificationEvidence::DataOverflow { .. } => {
                    view.ua_history_incomplete = true
                }
                opclook_contract::model::UaNotificationEvidence::SequenceUncertain { .. } => {
                    view.ua_history_uncertain = true
                }
            }
        }
        if let EventKind::UaRecovery { state } = &event.kind {
            use opclook_contract::model::UaRecoveryState;
            let phase = self.sessions.get(&session).ok_or(())?.ua_recovery_phase;
            match state {
                UaRecoveryState::ConnectionLost { .. } if phase != UaRecoveryPhase::Stopped => {
                    self.cancel_reads(
                        session,
                        None,
                        opclook_contract::model::ReadCancelReason::Cancelled,
                    );
                    self.purge_read_requests(session);
                    // Reserve each correlation for a typed cancellation before a
                    // raced SDK reply can be accepted in the same Connect epoch.
                    let owner = Owner::Session(session);
                    let cancelled: Vec<_> = self
                        .requests
                        .iter()
                        .filter_map(|(&(candidate, id), pending)| {
                            if candidate != owner
                                || self.timeout_queued.contains(&(owner, id))
                                || self.identity_cancelled.contains(&(owner, id))
                            {
                                return None;
                            }
                            let service = match pending.operation {
                                Operation::Browse => opclook_contract::model::UaService::Browse,
                                Operation::BrowseViews => {
                                    opclook_contract::model::UaService::BrowseViews
                                }
                                Operation::ResolveBrowsePath => {
                                    opclook_contract::model::UaService::TranslatePath
                                }
                                _ => return None,
                            };
                            Some((id, pending.operation, service))
                        })
                        .collect();
                    for (id, operation, service) in cancelled {
                        self.identity_cancelled.insert((owner, id));
                        self.route_events.push_back(Event {
                            owner,
                            origin: Origin::Reply(id),
                            kind: EventKind::Error {
                                operation,
                                message: BackendMsg::UaIdentityChanged { service },
                            },
                        });
                    }
                    let view = self.sessions.get_mut(&session).ok_or(())?;
                    view.ua_recovery_phase = UaRecoveryPhase::Lost;
                    view.ua_history_uncertain = true;
                    view.ua_subscriptions.clear();
                    view.subscription = SubscriptionState::Inactive;
                    view.read_in_flight = false;
                    view.reserved_read = None;
                }
                UaRecoveryState::Reconnected { .. } if phase == UaRecoveryPhase::Lost => {
                    self.sessions.get_mut(&session).ok_or(())?.ua_recovery_phase =
                        UaRecoveryPhase::Reconciling;
                }
                UaRecoveryState::Reconciled { missing, .. }
                    if phase == UaRecoveryPhase::Reconciling =>
                {
                    let blocked: HashSet<_> = missing.iter().copied().collect();
                    self.cancel_reads(
                        session,
                        Some(&blocked),
                        opclook_contract::model::ReadCancelReason::Cancelled,
                    );
                    let view = self.sessions.get_mut(&session).ok_or(())?;
                    view.recovery_blocked = blocked;
                    view.ua_recovery_phase = UaRecoveryPhase::Ready;
                    for key in view
                        .items
                        .keys()
                        .filter(|key| !view.recovery_blocked.contains(key))
                    {
                        view.pending_demands.insert(ReadDemand {
                            key: *key,
                            kind: ReadDemandKind::RecoverySnapshot,
                        });
                    }
                }
                UaRecoveryState::Stopped { .. } => {
                    self.cancel_reads(
                        session,
                        None,
                        opclook_contract::model::ReadCancelReason::Cancelled,
                    );
                    self.purge_read_requests(session);
                    let view = self.sessions.get_mut(&session).ok_or(())?;
                    view.ua_recovery_phase = UaRecoveryPhase::Stopped;
                    if view.state == SessionState::Connecting {
                        view.state = SessionState::Disconnected;
                        view.reconnect = ReconnectState::Stopped;
                    }
                    view.ua_subscriptions.clear();
                    view.subscription = SubscriptionState::Failed;
                    view.read_in_flight = false;
                    view.reserved_read = None;
                }
                UaRecoveryState::TypeMetadata { .. } if phase != UaRecoveryPhase::Stopped => {}
                _ => return Err(()),
            }
        }
        if matches!(event.origin, Origin::Session(_) | Origin::UaSession { .. })
            && let EventKind::Values { values } = &event.kind
            && let Some(view) = self.sessions.get(&session)
            && (matches!(
                view.ua_recovery_phase,
                UaRecoveryPhase::Lost | UaRecoveryPhase::Reconciling | UaRecoveryPhase::Stopped
            ) || values
                .iter()
                .any(|(key, _)| view.recovery_blocked.contains(key)))
        {
            return Err(());
        }
        let failed_connect = matches!(
            &event.kind,
            EventKind::Error {
                operation: Operation::Connect,
                ..
            }
        ) && pending
            .as_ref()
            .is_some_and(|p| p.operation == Operation::Connect);
        let disconnected = matches!(&event.kind, EventKind::Disconnected { .. })
            && self
                .sessions
                .get(&session)
                .is_some_and(|view| view.state != SessionState::Connecting);
        let failed_test = matches!(
            &event.kind,
            EventKind::Error {
                operation: Operation::Test,
                ..
            }
        );
        if failed_connect || disconnected || failed_test {
            self.cancel_reads(
                session,
                None,
                opclook_contract::model::ReadCancelReason::Disconnected,
            );
            self.purge_read_requests(session);
            self.session_routes.remove(&session);
        }
        if let Some(pending) = &pending {
            self.notify_read_terminals(session, pending.read_epoch, &pending.read_terminals);
        }
        if let (
            Some(pending),
            EventKind::Wrote {
                key,
                result: Ok(outcome),
            },
        ) = (&pending, &event.kind)
            && pending.operation == Operation::Write
            && pending.write_readback_key == Some(*key)
            && outcome.accepted()
            && let Some(view) = self.sessions.get_mut(&session)
            && view.state == SessionState::Connected
            && view.items.contains_key(key)
        {
            view.pending_demands.insert(ReadDemand {
                key: *key,
                kind: ReadDemandKind::WriteReadback,
            });
        }
        let additions = match event.origin {
            Origin::Unsolicited | Origin::Session(_) | Origin::UaSession { .. } => None,
            Origin::Reply(id) => self.pending_adds.remove(&(event.owner, id)),
        };
        if let EventKind::PollOnly { keys } = &event.kind {
            for (&(sid, _), additions) in &mut self.pending_adds {
                if sid == event.owner {
                    for key in keys {
                        if let Some(flag) = additions.get_mut(key) {
                            flag.poll_only = true;
                        }
                    }
                }
            }
        }
        let mut initial_terminals = Vec::new();
        if let (Some(additions), EventKind::AddItems { results }) = (additions, &event.kind)
            && let Some(view) = self.sessions.get_mut(&session)
        {
            let mut seen = HashSet::new();
            let mut duplicate = HashSet::new();
            for (key, _) in results {
                if !seen.insert(*key) {
                    duplicate.insert(*key);
                }
            }
            for (key, result) in results {
                if !duplicate.contains(key)
                    && result.is_ok()
                    && let Some(addition) = additions.get(key)
                {
                    view.items
                        .entry(*key)
                        .and_modify(|flag| *flag |= addition.poll_only)
                        .or_insert(addition.poll_only);
                    if !addition.initial_seen {
                        view.pending_initial.insert(*key, now);
                    } else if let Some(status) = &addition.initial_status {
                        initial_terminals.push(opclook_contract::model::ReadTerminal {
                            request_id: None,
                            key: *key,
                            needs: vec![opclook_contract::model::ReadNeed::Initial],
                            status: status.clone(),
                        });
                    }
                    view.poll_defer_since.insert(*key, now);
                }
            }
        }
        if matches!(event.origin, Origin::Session(_) | Origin::UaSession { .. })
            && let EventKind::Values { values } = &event.kind
            && let Some(view) = self.sessions.get_mut(&session)
        {
            for (key, value) in values {
                let waiting = value
                    .quality
                    .waiting_for_initial_data(view.endpoint.protocol);
                if waiting {
                    continue;
                }
                for (&(sid, _), additions) in &mut self.pending_adds {
                    if sid == event.owner
                        && let Some(addition) = additions.get_mut(key)
                    {
                        addition.initial_seen = true;
                        addition.initial_status =
                            Some(opclook_contract::model::ReadTerminalStatus::InitialPush {
                                quality: value.quality.clone(),
                                timestamp: value.timestamp,
                                times: value.times.clone(),
                                reason: value.reason.clone(),
                            });
                    }
                }
                if view.pending_initial.remove(key).is_some() {
                    initial_terminals.push(opclook_contract::model::ReadTerminal {
                        request_id: None,
                        key: *key,
                        needs: vec![opclook_contract::model::ReadNeed::Initial],
                        status: opclook_contract::model::ReadTerminalStatus::InitialPush {
                            quality: value.quality.clone(),
                            timestamp: value.timestamp,
                            times: value.times.clone(),
                            reason: value.reason.clone(),
                        },
                    });
                }
            }
        }
        if !initial_terminals.is_empty() {
            let epoch = self
                .sessions
                .get(&session)
                .and_then(|view| view.connection_id);
            self.notify_read_terminals(session, epoch, &initial_terminals);
        }
        let correlated_read = pending
            .as_ref()
            .is_some_and(|p| p.operation == Operation::Read);
        let read_pending = self
            .requests
            .iter()
            .any(|(&(sid, _), p)| sid == event.owner && p.operation == Operation::Read);
        let mut schedule = false;
        if let Some(view) = self.sessions.get_mut(&session) {
            match &event.kind {
                EventKind::Connected { endpoint, .. } => {
                    let acquisition = view.endpoint.acquisition.clone();
                    view.endpoint = endpoint.clone();
                    view.endpoint.acquisition = acquisition;
                    view.state = SessionState::Connected;
                    view.reconnect = ReconnectState::Idle;
                    view.subscription = SubscriptionState::Inactive;
                    view.ua_subscriptions.clear();
                    view.read_in_flight = false;
                    let now = Instant::now();
                    view.last_poll = now.checked_sub(Duration::from_secs(5)).unwrap_or(now);
                    view.last_test = now;
                }
                EventKind::Disconnected { .. } if view.state != SessionState::Connecting => {
                    schedule = view.state == SessionState::Connected;
                    view.state = SessionState::Disconnected;
                    view.read_in_flight = false;
                    view.subscription = SubscriptionState::Inactive;
                    view.ua_subscriptions.clear();
                    view.pending_initial.clear();
                    view.pending_demands.clear();
                    view.reserved_demands.clear();
                    view.reserved_initial.clear();
                    view.reserved_poll.clear();
                    view.poll_defer_since.clear();
                    view.reserved_read = None;
                }
                EventKind::Error {
                    operation: Operation::Connect,
                    ..
                } => {
                    let was_connecting = view.state == SessionState::Connecting;
                    view.state = SessionState::Disconnected;
                    view.read_in_flight = false;
                    view.subscription = SubscriptionState::Inactive;
                    view.ua_subscriptions.clear();
                    view.pending_initial.clear();
                    view.pending_demands.clear();
                    view.reserved_demands.clear();
                    view.reserved_initial.clear();
                    view.reserved_poll.clear();
                    view.poll_defer_since.clear();
                    view.reserved_read = None;
                    if was_connecting && matches!(view.reconnect, ReconnectState::Waiting { .. }) {
                        schedule = true;
                    } else if was_connecting {
                        view.reconnect = ReconnectState::Stopped;
                    }
                }
                EventKind::Error {
                    operation: Operation::Test,
                    ..
                } => {
                    schedule = view.state == SessionState::Connected;
                    view.state = SessionState::Disconnected;
                    view.read_in_flight = false;
                    view.subscription = SubscriptionState::Inactive;
                    view.ua_subscriptions.clear();
                    view.pending_initial.clear();
                    view.pending_demands.clear();
                    view.reserved_demands.clear();
                    view.reserved_initial.clear();
                    view.reserved_poll.clear();
                    view.poll_defer_since.clear();
                    view.reserved_read = None;
                }
                EventKind::Error {
                    operation: Operation::Read,
                    ..
                } if correlated_read => view.read_in_flight = read_pending,
                EventKind::Values { .. } if correlated_read => view.read_in_flight = read_pending,
                EventKind::UaSubscriptionParameters { subscriptions } => {
                    if !matches!(
                        view.ua_recovery_phase,
                        UaRecoveryPhase::Lost | UaRecoveryPhase::Stopped
                    ) {
                        view.ua_subscriptions = subscriptions.clone();
                    }
                }
                EventKind::SubscriptionState { state }
                    if !matches!(
                        view.ua_recovery_phase,
                        UaRecoveryPhase::Lost | UaRecoveryPhase::Stopped
                    ) =>
                {
                    view.subscription = *state;
                    if *state == SubscriptionState::Active
                        && !read_pending
                        && view.reserved_read.is_none()
                    {
                        view.read_in_flight = false;
                    }
                }
                EventKind::PollOnly { keys } => {
                    for key in keys {
                        if let Some(flag) = view.items.get_mut(key) {
                            *flag = true;
                        }
                    }
                    view.last_poll = Instant::now()
                        .checked_sub(Duration::from_secs(3600))
                        .unwrap_or_else(Instant::now);
                }
                _ => {}
            }
        }
        if schedule {
            self.schedule_reconnect(session, Instant::now());
        }
        Ok(pending)
    }
    pub fn due(
        &mut self,
        now: Instant,
        interval: Duration,
        test_interval: Option<Duration>,
    ) -> Vec<Due> {
        let mut work = Vec::new();
        let mut unavailable = Vec::new();
        for (&id, view) in &mut self.sessions {
            match view.state {
                SessionState::Connected => {
                    if matches!(
                        view.ua_recovery_phase,
                        UaRecoveryPhase::Lost
                            | UaRecoveryPhase::Reconciling
                            | UaRecoveryPhase::Stopped
                    ) {
                        continue;
                    }
                    let profile = view.endpoint.acquisition.as_ref();
                    let interval = profile
                        .map(|c| Duration::from_millis(c.interval_ms))
                        .unwrap_or(interval);
                    view.last_due_interval = Some(interval);
                    let configured_poll =
                        profile.map(|c| c.update_mode == opclook_contract::model::UpdateMode::Poll);
                    let busy = self.requests.iter().any(|(&(sid, _), p)| {
                        sid == Owner::Session(id) && p.operation == Operation::Browse
                    });
                    let push = configured_poll.map(|poll| !poll).unwrap_or(
                        !view.force_poll
                            && view.endpoint.protocol == Protocol::OpcUa
                            && view.subscription == SubscriptionState::Active,
                    );
                    if view.connection_id.is_some() && !view.read_in_flight && !busy {
                        let mut keys: Vec<_> = view
                            .pending_initial
                            .iter()
                            .filter_map(|(&key, &since)| {
                                view.initial_due(key, since, now, Some(interval))
                                    .then_some(key)
                            })
                            .collect();
                        let initial: HashSet<ItemKey> = keys.iter().copied().collect();
                        keys.extend(view.pending_demands.iter().map(|d| d.key));
                        let mut poll = HashSet::new();
                        if now.saturating_duration_since(view.last_poll) >= interval {
                            poll.extend(view.items.iter().filter_map(|(&key, &poll_only)| {
                                let startup_wait = configured_poll.is_none()
                                    && !view.force_poll
                                    && view.endpoint.protocol == Protocol::OpcUa
                                    && view.poll_defer_since.get(&key).is_some_and(|since| {
                                        now.saturating_duration_since(*since) < interval
                                    });
                                ((!push || (configured_poll.is_none() && poll_only))
                                    && !startup_wait
                                    && !view.recovery_blocked.contains(&key))
                                .then_some(key)
                            }));
                            keys.extend(poll.iter().copied());
                        }
                        view.poll_defer_since
                            .retain(|_, since| now.saturating_duration_since(*since) < interval);
                        keys.sort_unstable();
                        keys.dedup();
                        if !keys.is_empty() {
                            let Some(raw_permit) = self.next_permit else {
                                view.reserved_initial = initial;
                                view.reserved_poll = poll;
                                view.reserved_demands = std::mem::take(&mut view.pending_demands);
                                for key in &keys {
                                    view.pending_initial.remove(key);
                                }
                                let epoch = view.connection_id;
                                let terminals = Self::unavailable_reservation(view);
                                unavailable.push((id, epoch, terminals));
                                view.last_poll = now;
                                work.push(Due::ReadUnavailable { session: id });
                                continue;
                            };
                            self.next_permit = raw_permit.checked_add(1);
                            view.reserved_demands = std::mem::take(&mut view.pending_demands);
                            view.reserved_initial = initial;
                            view.reserved_poll = poll;
                            let permit = ReadPermit(raw_permit);
                            for key in &keys {
                                view.pending_initial.remove(key);
                            }
                            view.read_in_flight = true;
                            view.reserved_read = Some((permit, keys.clone()));
                            view.last_poll = now;
                            work.push(Due::Read {
                                session: id,
                                protocol: view.endpoint.protocol,
                                keys,
                                permit,
                            });
                        }
                    }
                    let test_pending = self.requests.iter().any(|(&(sid, _), p)| {
                        sid == Owner::Session(id) && p.operation == Operation::Test
                    });
                    if let Some(interval) = test_interval
                        && !test_pending
                        && now.saturating_duration_since(view.last_test) >= interval
                    {
                        view.last_test = now;
                        work.push(Due::Test {
                            session: id,
                            protocol: view.endpoint.protocol,
                        });
                    }
                }
                SessionState::Disconnected
                    if self.auto_reconnect
                        && matches!(view.reconnect, ReconnectState::Waiting { at, .. } if now >= at) =>
                {
                    work.push(Due::Reconnect { session: id })
                }
                _ => {}
            }
        }
        for (session, epoch, terminals) in unavailable {
            self.notify_read_terminals(session, epoch, &terminals);
        }
        work
    }
    pub fn has_pending(&self, owner: impl Into<Owner>, request: RequestId) -> bool {
        self.requests.contains_key(&(owner.into(), request))
    }
    pub fn discard_pending(&mut self, owner: impl Into<Owner>, request: RequestId) {
        let session = owner.into();
        self.timeout_queued.remove(&(session, request));
        self.identity_cancelled.remove(&(session, request));
        self.pending_adds.remove(&(session, request));
        self.deferred_routes
            .retain(|item| !(item.owner == session && item.request_id == request));
        self.discovery_routes.remove(&request);
        if let Some(pending) = self.requests.remove(&(session, request))
            && let Owner::Session(session) = session
        {
            let terminals: Vec<_> = pending
                .read_coverage
                .into_iter()
                .map(|(key, needs)| opclook_contract::model::ReadTerminal {
                    request_id: Some(request),
                    key,
                    needs,
                    status: opclook_contract::model::ReadTerminalStatus::Cancelled(
                        opclook_contract::model::ReadCancelReason::Cancelled,
                    ),
                })
                .collect();
            self.notify_read_terminals(session, pending.read_epoch, &terminals);
            if pending.operation == Operation::Read {
                let has_read = self.requests.iter().any(|(&(owner, _), p)| {
                    owner == Owner::Session(session) && p.operation == Operation::Read
                });
                if let Some(view) = self.sessions.get_mut(&session) {
                    view.read_in_flight = has_read || view.reserved_read.is_some();
                }
            }
        }
    }
    /// Discard only one family of requests for an owner. Other discovery
    /// operations can continue independently under the same owner.
    pub fn discard_discovery_operation(&mut self, operation: Operation) {
        let owner = Owner::Discovery;
        let ids: Vec<_> = self
            .requests
            .iter()
            .filter_map(|(&(request_owner, id), pending)| {
                (request_owner == owner && pending.operation == operation).then_some(id)
            })
            .collect();
        for id in ids {
            self.discard_pending(owner, id);
        }
    }
    pub fn invalidate_session_requests(&mut self, owner: impl Into<Owner>) -> Vec<Operation> {
        let owner = owner.into();
        if let Owner::Session(session) = owner {
            self.cancel_reads(
                session,
                None,
                opclook_contract::model::ReadCancelReason::Cancelled,
            );
            if let Some(view) = self.sessions.get_mut(&session) {
                view.reserved_read = None;
                view.read_in_flight = false;
            }
        }
        self.deferred_routes.retain(|item| item.owner != owner);
        self.timeout_queued.retain(|(sid, _)| *sid != owner);
        self.identity_cancelled.retain(|(sid, _)| *sid != owner);
        self.pending_adds.retain(|(sid, _), _| *sid != owner);
        if owner == Owner::Discovery {
            self.discovery_routes.clear();
        }
        let keys: Vec<_> = self
            .requests
            .keys()
            .filter(|(sid, _)| *sid == owner)
            .copied()
            .collect();
        let mut operations = match owner {
            Owner::Session(session) => self.invalidated.remove(&session).unwrap_or_default(),
            Owner::Discovery => Vec::new(),
        };
        operations.extend(
            keys.into_iter()
                .filter_map(|key| self.requests.remove(&key).map(|p| p.operation)),
        );
        operations
    }
    /// Queue the caller's already-expired Read deadline using the same
    /// correlation and late-reply rejection as `expire_requests`.
    pub fn timeout_read_request(
        &mut self,
        session: SessionId,
        request: RequestId,
        seconds: u64,
    ) -> bool {
        let owner = Owner::Session(session);
        if self.timeout_queued.contains(&(owner, request))
            || !self
                .requests
                .get(&(owner, request))
                .is_some_and(|pending| pending.operation == Operation::Read)
        {
            return false;
        }
        self.timeout_queued.insert((owner, request));
        self.route_events.push_back(Event {
            owner,
            origin: Origin::Reply(request),
            kind: EventKind::Error {
                operation: Operation::Read,
                message: BackendMsg::RequestTimeout { seconds },
            },
        });
        true
    }

    /// Queue one correlated timeout event per expired request. The engine
    /// cancels unstarted preparation and tells a late Connect to disconnect.
    pub fn expire_requests(&mut self, now: Instant, configured: u64) -> Vec<SessionId> {
        let mut disconnects = Vec::new();
        let expired: Vec<_> = self
            .requests
            .iter()
            .filter_map(|(&(sid, id), pending)| {
                let limit = request_timeout(pending.operation, configured);
                (now.saturating_duration_since(pending.sent) > limit
                    && !self.timeout_queued.contains(&(sid, id))
                    && !self.identity_cancelled.contains(&(sid, id)))
                .then_some((sid, id, pending.operation, limit.as_secs()))
            })
            .collect();
        for (session, request_id, operation, seconds) in expired {
            if matches!(
                operation,
                Operation::Disconnect
                    | Operation::Subscription
                    | Operation::UaRuntime
                    | Operation::Add
                    | Operation::Remove
            ) {
                self.discard_pending(session, request_id);
                continue;
            }
            self.deferred_routes
                .retain(|item| !(item.owner == session && item.request_id == request_id));
            if operation == Operation::Connect
                && let Owner::Session(session) = session
            {
                self.session_routes.remove(&session);
                if let Some(view) = self.sessions.get(&session) {
                    disconnects.push(session);
                    let _ = self.dispatch(Command {
                        request_id: None,
                        owner: Owner::Session(session),
                        protocol: view.endpoint.protocol,
                        kind: CommandKind::Disconnect,
                    });
                }
            }
            self.timeout_queued.insert((session, request_id));
            self.route_events.push_back(Event {
                owner: session,
                origin: Origin::Reply(request_id),
                kind: EventKind::Error {
                    operation,
                    message: BackendMsg::RequestTimeout { seconds },
                },
            });
        }
        disconnects
    }
    pub fn expired_requests(
        &self,
        now: Instant,
        configured: u64,
    ) -> Vec<(Owner, RequestId, Operation, C, u64)>
    where
        C: Clone,
    {
        self.requests
            .iter()
            .filter_map(|(&(sid, id), p)| {
                let limit = request_timeout(p.operation, configured);
                (now.saturating_duration_since(p.sent) > limit)
                    .then(|| (sid, id, p.operation, p.ctx.clone(), limit.as_secs()))
            })
            .collect()
    }
    /// Return the first Read whose caller-defined deadline has elapsed.
    pub fn expired_read_request(
        &self,
        session: SessionId,
        now: Instant,
        limit: Duration,
    ) -> Option<RequestId> {
        self.requests.iter().find_map(|(&(owner, id), pending)| {
            (owner == Owner::Session(session)
                && pending.operation == Operation::Read
                && now.saturating_duration_since(pending.sent) >= limit
                && !self.timeout_queued.contains(&(owner, id)))
            .then_some(id)
        })
    }
    pub fn has_expired_operation(
        &self,
        session: SessionId,
        now: Instant,
        limit: Duration,
        operations: &[Operation],
    ) -> bool {
        self.requests.iter().any(|(&(sid, _), p)| {
            sid == Owner::Session(session)
                && operations.contains(&p.operation)
                && now.saturating_duration_since(p.sent) >= limit
        })
    }
    pub fn set_auto_reconnect(&mut self, enabled: bool) {
        self.auto_reconnect = enabled;
    }
    pub fn session(&self, id: SessionId) -> Option<&SessionRuntime> {
        self.sessions.get(&id)
    }
    pub fn connection_id(&self, id: SessionId) -> Option<RequestId> {
        self.sessions
            .get(&id)
            .and_then(|session| session.connection_id)
    }

    pub fn set_force_poll(&mut self, id: SessionId, enabled: bool) {
        if let Some(view) = self.sessions.get_mut(&id) {
            view.force_poll = enabled;
        }
    }
    pub fn remove_session(&mut self, id: SessionId) {
        self.cancel_reads(
            id,
            None,
            opclook_contract::model::ReadCancelReason::Disconnected,
        );
        self.purge_read_requests(id);
        self.cancel_da_routes(id);
        self.invalidated.remove(&id);
        self.pending_adds
            .retain(|(sid, _), _| *sid != Owner::Session(id));
        self.sessions.remove(&id);
    }
    pub fn manual_disconnect(&mut self, id: SessionId) {
        self.cancel_reads(
            id,
            None,
            opclook_contract::model::ReadCancelReason::Disconnected,
        );
        self.purge_read_requests(id);
        if let Some(view) = self.sessions.get_mut(&id) {
            view.state = SessionState::Disconnected;
            view.reconnect = ReconnectState::Stopped;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.ua_subscriptions.clear();
            view.pending_initial.clear();
            view.pending_demands.clear();
            view.reserved_demands.clear();
            view.reserved_initial.clear();
            view.reserved_poll.clear();
            view.poll_defer_since.clear();
            view.reserved_read = None;
        }
    }
    pub fn stop_reconnect(&mut self, id: SessionId) {
        if let Some(view) = self.sessions.get_mut(&id) {
            view.reconnect = ReconnectState::Stopped;
        }
    }
    pub fn resume_reconnect(&mut self, id: SessionId) {
        if let Some(view) = self.sessions.get_mut(&id) {
            view.reconnect = ReconnectState::Idle;
        }
    }
    pub fn schedule_reconnect(&mut self, id: SessionId, now: Instant) -> ReconnectScheduled {
        if !self.auto_reconnect {
            return ReconnectScheduled::Unchanged;
        }
        self.sessions
            .get_mut(&id)
            .map(|view| {
                view.reconnect.schedule(
                    now,
                    constants::RECONNECT_MAX_ATTEMPTS,
                    constants::RECONNECT_BASE_DELAY,
                    constants::RECONNECT_MAX_DELAY,
                )
            })
            .unwrap_or(ReconnectScheduled::Unchanged)
    }
    pub fn try_recv(&mut self) -> Option<Event> {
        self.pump_da_routes();
        let mut event = self
            .route_events
            .pop_front()
            .or_else(|| self.gateway.try_recv())?;
        // The request identity is authoritative when a worker reports a
        // legacy operation label for a correlated failure.
        if let EventKind::Error { operation, .. } = &mut event.kind
            && let Origin::Reply(id) = event.origin
            && let Some(pending) = self.requests.get(&(event.owner, id))
        {
            *operation = pending.operation;
        }
        Some(event)
    }
    /// Explicit shutdown emits cancellation terminals. Dropping the Engine
    /// without calling shutdown cannot guarantee observer delivery.
    pub fn shutdown(&mut self) -> GatewayShutdown {
        let sessions: Vec<_> = self.sessions.keys().copied().collect();
        for session in sessions {
            self.cancel_reads(
                session,
                None,
                opclook_contract::model::ReadCancelReason::Cancelled,
            );
            self.purge_read_requests(session);
        }
        self.deferred_routes.clear();
        self.session_routes.clear();
        self.discovery_routes.clear();
        self.route_events.clear();
        self.invalidated.clear();
        self.pending_adds.clear();
        self.timeout_queued.clear();
        self.identity_cancelled.clear();
        self.route_service = None;
        self.gateway.shutdown()
    }
    #[cfg(feature = "test-support")]
    pub fn seed_da_route_for_test(
        &mut self,
        owner: impl Into<Owner>,
        request: RequestId,
    ) -> Result<(), DaRouteError> {
        if self.route_service.is_none() {
            self.route_service = Some(
                RouteService::new().map_err(|e| DaRouteError::HelperUnavailable(e.to_string()))?,
            );
        }
        let service = self
            .route_service
            .as_ref()
            .ok_or_else(|| DaRouteError::HelperUnavailable("route service unavailable".into()))?;
        let ticket = service.prepare(
            "127.0.0.1".into(),
            None,
            vec![],
            Instant::now() + Duration::from_secs(2),
        )?;
        match owner.into() {
            Owner::Discovery => {
                self.discovery_routes.insert(request, ticket);
            }
            Owner::Session(session) => {
                self.session_routes.insert(session, ticket);
            }
        }
        Ok(())
    }
    #[cfg(feature = "test-support")]
    pub fn has_discovery_route_for_test(&self, request: RequestId) -> bool {
        self.discovery_routes.contains_key(&request)
    }
    #[cfg(feature = "test-support")]
    pub fn exhaust_requests_for_test(&mut self) {
        self.next = None;
    }
    #[cfg(feature = "test-support")]
    pub fn limit_dispatch_for_test(&self, accepted: usize) {
        self.test_dispatch_limit.set(Some(accepted));
    }
    #[cfg(feature = "test-support")]
    pub fn dispatched_for_test(&self) -> Vec<Command> {
        self.test_dispatched.borrow().clone()
    }
    #[cfg(feature = "test-support")]
    pub fn gateway_for_test(&self) -> &Gateway {
        &self.gateway
    }
    #[cfg(feature = "test-support")]
    pub fn seed_session_for_test(&mut self, id: SessionId, mut session: SessionRuntime) {
        if let Some(old) = self.sessions.get(&id) {
            session.connection_id = old.connection_id;
            session.ua_subscriptions = old.ua_subscriptions.clone();
            session.pending_initial = old.pending_initial.clone();
            session.pending_demands = old.pending_demands.clone();
            session.reserved_demands = old.reserved_demands.clone();
            session.reserved_initial = old.reserved_initial.clone();
            session.reserved_poll = old.reserved_poll.clone();
            session.poll_defer_since = old.poll_defer_since.clone();
            session.reserved_read = old.reserved_read.clone();
            session.last_due_interval = old.last_due_interval;
            session.used_keys = old.used_keys.clone();
        } else {
            session.connection_id = Some(RequestId::FIRST);
        }
        self.sessions.insert(id, session);
    }
    #[cfg(feature = "test-support")]
    pub fn inject_event_for_test(&mut self, event: Event) {
        self.route_events.push_back(event);
    }
    #[cfg(feature = "test-support")]
    pub fn connection_id_for_test(&self, id: SessionId) -> Option<RequestId> {
        self.sessions
            .get(&id)
            .and_then(|session| session.connection_id)
    }
    #[cfg(feature = "test-support")]
    pub fn insert_pending_for_test(
        &mut self,
        sid: impl Into<Owner>,
        id: RequestId,
        pending: PendingRequest<C>,
    ) {
        self.requests.insert((sid.into(), id), pending);
    }
    #[cfg(feature = "test-support")]
    pub fn pending_ids_for_test(&self, sid: impl Into<Owner>) -> Vec<RequestId> {
        let sid = sid.into();
        let mut ids: Vec<_> = self
            .requests
            .keys()
            .filter_map(|(s, id)| (*s == sid).then_some(*id))
            .collect();
        ids.sort_unstable();
        ids
    }
    #[cfg(feature = "test-support")]
    pub fn age_requests_for_test(&mut self, sid: impl Into<Owner>, age: Duration) {
        let sid = sid.into();
        for ((session, _), pending) in &mut self.requests {
            if *session == sid
                && let Some(sent) = pending.sent.checked_sub(age)
            {
                pending.sent = sent;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sid(n: u64) -> SessionId {
        SessionId::new(n).unwrap()
    }
    fn owner(n: u64) -> Owner {
        Owner::Session(sid(n))
    }
    fn rid(n: u64) -> RequestId {
        RequestId::new(n).unwrap()
    }
    use opclook_contract::model::BackendMsg;
    fn cached_poll_engine(now: Instant) -> Engine<()> {
        use opclook_contract::model::{AcquisitionConfig, PollFreshness, UpdateMode};
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let mut profile = AcquisitionConfig::ua_default();
        profile.update_mode = UpdateMode::Poll;
        profile.poll_freshness = PollFreshness::CacheAllowed;
        profile.interval_ms = 1234;
        endpoint.acquisition = Some(profile);
        let mut view = SessionRuntime::new(endpoint, now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.items.insert(key(1), false);
        view.items.insert(key(2), false);
        let mut engine = Engine::default();
        engine.test_skip_dispatch = true;
        engine.sessions.insert(sid(1), view);
        engine
    }

    #[test]
    fn ua_connect_accepts_early_recovery_without_losing_remote_scope() {
        use opclook_contract::model::{UaNotificationEvidence, UaRecoveryMode, UaRecoveryState};
        let now = Instant::now();
        let mut engine = Engine::default();
        engine.test_skip_dispatch = true;
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        let event = |connection, revision, kind| Event {
            owner: owner(1),
            origin: Origin::UaSession {
                connection,
                revision,
            },
            kind,
        };
        let lost = EventKind::UaRecovery {
            state: UaRecoveryState::ConnectionLost { status: 1 },
        };
        assert!(
            engine
                .accept_at(&event(rid(99), 1, lost.clone()), now)
                .is_err()
        );
        engine
            .accept_at(&event(id, 1, lost), now)
            .expect("early Lost");
        assert!(
            engine
                .accept_at(&event(id, 1, EventKind::Values { values: vec![] }), now)
                .is_err()
        );
        assert!(
            engine
                .accept_at(
                    &event(
                        id,
                        1,
                        EventKind::UaNotificationFacts {
                            subscription_generation: 1,
                            evidence: UaNotificationEvidence::SequenceGap {
                                previous: 1,
                                current: 3
                            }
                        }
                    ),
                    now
                )
                .is_err()
        );
        assert!(
            engine
                .accept_at(
                    &event(id, 1, EventKind::UaItemMetadata { facts: vec![] }),
                    now
                )
                .is_err()
        );
        assert!(
            engine
                .accept_at(
                    &event(
                        id,
                        1,
                        EventKind::UaRecovery {
                            state: UaRecoveryState::Reconciled {
                                missing: vec![],
                                ambiguous: false
                            }
                        }
                    ),
                    now
                )
                .is_err()
        );
        engine
            .accept_at(
                &event(
                    id,
                    2,
                    EventKind::UaRecovery {
                        state: UaRecoveryState::Reconnected {
                            mode: UaRecoveryMode::NewSession,
                        },
                    },
                ),
                now,
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Connected {
                        endpoint: endpoint.clone(),
                        info: BackendMsg::raw("connected"),
                    },
                },
                now,
            )
            .unwrap();
        let view = engine.session(sid(1)).unwrap();
        assert_eq!(view.ua_revision, 2);
        assert_eq!(view.ua_recovery_phase, UaRecoveryPhase::Reconciling);
        assert!(view.ua_history_uncertain);
        engine
            .accept_at(
                &event(
                    id,
                    2,
                    EventKind::UaRecovery {
                        state: UaRecoveryState::Reconciled {
                            missing: vec![],
                            ambiguous: false,
                        },
                    },
                ),
                now,
            )
            .unwrap();
        assert_eq!(
            engine.session(sid(1)).unwrap().ua_recovery_phase,
            UaRecoveryPhase::Ready
        );
        let facts = || EventKind::UaNotificationFacts {
            subscription_generation: 1,
            evidence: UaNotificationEvidence::SequenceGap {
                previous: 4,
                current: 6,
            },
        };
        assert!(engine.accept_at(&event(id, 1, facts()), now).is_err());
        engine.accept_at(&event(id, 2, facts()), now).unwrap();
        assert!(engine.session(sid(1)).unwrap().ua_history_incomplete);
        assert!(engine.session(sid(1)).unwrap().items.is_empty());
        let next = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect { endpoint },
                (),
            )
            .unwrap();
        let view = engine.session(sid(1)).unwrap();
        assert_eq!(view.connection_id, Some(next));
        assert_eq!(view.ua_revision, 0);
        assert_eq!(view.ua_recovery_phase, UaRecoveryPhase::Normal);
        assert!(!view.ua_history_uncertain);
        assert!(
            engine
                .accept_at(
                    &event(
                        id,
                        3,
                        EventKind::UaRecovery {
                            state: UaRecoveryState::ConnectionLost { status: 1 }
                        }
                    ),
                    now
                )
                .is_err()
        );
    }

    #[test]
    fn ua_reconciliation_before_connect_reply_keeps_current_scope() {
        use opclook_contract::model::{UaRecoveryMode, UaRecoveryState};
        let now = Instant::now();
        let mut engine = Engine::default();
        engine.test_skip_dispatch = true;
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        let recovery = |revision, state| Event {
            owner: owner(1),
            origin: Origin::UaSession {
                connection: id,
                revision,
            },
            kind: EventKind::UaRecovery { state },
        };
        engine
            .accept_at(
                &recovery(1, UaRecoveryState::ConnectionLost { status: 1 }),
                now,
            )
            .unwrap();
        engine
            .accept_at(
                &recovery(
                    2,
                    UaRecoveryState::Reconnected {
                        mode: UaRecoveryMode::NewSession,
                    },
                ),
                now,
            )
            .unwrap();
        engine
            .accept_at(
                &recovery(
                    2,
                    UaRecoveryState::Reconciled {
                        missing: vec![key(3)],
                        ambiguous: false,
                    },
                ),
                now,
            )
            .unwrap();
        let view = engine.session(sid(1)).unwrap();
        assert_eq!(view.state, SessionState::Connecting);
        assert_eq!(view.ua_recovery_phase, UaRecoveryPhase::Ready);
        assert!(view.items.is_empty());
        assert!(view.recovery_blocked.contains(&key(3)));
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Connected {
                        endpoint,
                        info: BackendMsg::raw("connected"),
                    },
                },
                now,
            )
            .unwrap();
        let view = engine.session(sid(1)).unwrap();
        assert_eq!(view.ua_revision, 2);
        assert_eq!(view.ua_recovery_phase, UaRecoveryPhase::Ready);
        assert!(view.ua_history_uncertain);
        assert!(view.recovery_blocked.contains(&key(3)));
        assert!(view.items.is_empty());
    }

    #[test]
    fn da_route_connect_clears_old_ua_scope_on_registration() {
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let old = engine.sessions.get_mut(&sid(1)).unwrap();
        old.ua_revision = 4;
        old.ua_recovery_phase = UaRecoveryPhase::Stopped;
        old.ua_history_incomplete = true;
        old.ua_history_uncertain = true;
        old.recovery_blocked.insert(key(1));
        let id = engine
            .prepare_da(
                sid(1),
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("127.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    adapter: None,
                    local_addresses: vec![],
                    deadline: now + Duration::from_secs(2),
                },
            )
            .unwrap();
        let view = engine.session(sid(1)).unwrap();
        assert_eq!(view.connection_id, Some(id));
        assert_eq!(view.ua_revision, 0);
        assert_eq!(view.ua_recovery_phase, UaRecoveryPhase::Normal);
        assert!(!view.ua_history_uncertain && !view.ua_history_incomplete);
        assert!(view.recovery_blocked.is_empty());
        engine.cancel_da_routes(sid(1));
    }

    #[test]
    fn early_stopped_does_not_resurrect_on_connect_reply() {
        use opclook_contract::model::UaRecoveryState;
        let now = Instant::now();
        let mut engine = Engine::default();
        engine.test_skip_dispatch = true;
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::UaSession {
                        connection: id,
                        revision: 1,
                    },
                    kind: EventKind::UaRecovery {
                        state: UaRecoveryState::Stopped { status: 1 },
                    },
                },
                now,
            )
            .unwrap();
        assert!(
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(id),
                        kind: EventKind::Connected {
                            endpoint,
                            info: BackendMsg::raw("connected")
                        }
                    },
                    now
                )
                .is_err()
        );
        assert_ne!(
            engine.session(sid(1)).unwrap().state,
            SessionState::Connected
        );
    }

    #[test]
    fn ua_early_overflow_marks_sticky_history_without_acknowledging_registration() {
        use opclook_contract::model::UaNotificationEvidence;
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let unknown = key(99);
        let fact = Event {
            owner: owner(1),
            origin: Origin::UaSession {
                connection: rid(77),
                revision: 0,
            },
            kind: EventKind::UaNotificationFacts {
                subscription_generation: 1,
                evidence: UaNotificationEvidence::DataOverflow {
                    key: unknown,
                    status: 0x480,
                },
            },
        };
        engine.accept_at(&fact, now).unwrap();
        assert!(engine.session(sid(1)).unwrap().ua_history_incomplete);
        assert!(!engine.session(sid(1)).unwrap().items.contains_key(&unknown));
        let id = dispatch_due(&mut engine, now);
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap();
        assert!(engine.session(sid(1)).unwrap().ua_history_incomplete);
    }

    #[test]
    fn ua_metadata_rejects_old_sdk_revision_and_unregistered_keys() {
        use opclook_contract::model::{UaItemMetadata, UaRecoveryMode, UaRecoveryState};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let event = |revision, kind| Event {
            owner: owner(1),
            origin: Origin::UaSession {
                connection: rid(77),
                revision,
            },
            kind,
        };
        engine
            .accept_at(
                &event(
                    1,
                    EventKind::UaRecovery {
                        state: UaRecoveryState::ConnectionLost { status: 1 },
                    },
                ),
                now,
            )
            .unwrap();
        engine
            .accept_at(
                &event(
                    2,
                    EventKind::UaRecovery {
                        state: UaRecoveryState::Reconnected {
                            mode: UaRecoveryMode::NewSession,
                        },
                    },
                ),
                now,
            )
            .unwrap();
        engine
            .accept_at(
                &event(
                    2,
                    EventKind::UaRecovery {
                        state: UaRecoveryState::Reconciled {
                            missing: vec![key(2)],
                            ambiguous: true,
                        },
                    },
                ),
                now,
            )
            .unwrap();
        let fact = |key| UaItemMetadata {
            key,
            data_type: Some("Int32".into()),
            type_id: Some(6),
            user_access_level: Some(3),
            diagnostic: None,
        };
        assert!(
            engine
                .accept_at(
                    &event(
                        1,
                        EventKind::UaItemMetadata {
                            facts: vec![fact(key(1))]
                        }
                    ),
                    now
                )
                .is_err()
        );
        assert!(
            engine
                .accept_at(
                    &event(
                        2,
                        EventKind::UaItemMetadata {
                            facts: vec![fact(key(2))]
                        }
                    ),
                    now
                )
                .is_err()
        );
        assert!(
            engine
                .accept_at(
                    &event(
                        2,
                        EventKind::UaItemMetadata {
                            facts: vec![fact(key(1))]
                        }
                    ),
                    now
                )
                .is_ok()
        );
        assert!(
            engine
                .accept_at(&event(1, EventKind::Values { values: vec![] }), now)
                .is_err()
        );
    }

    #[test]
    fn recovery_lost_cancels_browse_with_correlated_typed_terminal() {
        use opclook_contract::model::{NodeClassFilter, UaRecoveryState, UaService};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Browse {
                    node_id: "ns=2;s=old".into(),
                    filter: NodeClassFilter::All,
                    view_id: None,
                },
                (),
            )
            .unwrap();
        let lost = Event {
            owner: owner(1),
            origin: Origin::Session(rid(77)),
            kind: EventKind::UaRecovery {
                state: UaRecoveryState::ConnectionLost { status: 1 },
            },
        };
        engine.accept_at(&lost, now).unwrap();
        let late = Event {
            owner: owner(1),
            origin: Origin::Reply(id),
            kind: EventKind::Browse {
                node_id: "ns=2;s=old".into(),
                children: vec![],
            },
        };
        assert!(engine.accept_at(&late, now).is_err());
        let terminal = engine.try_recv().unwrap();
        assert!(matches!(
            terminal.kind,
            EventKind::Error {
                operation: Operation::Browse,
                message: BackendMsg::UaIdentityChanged {
                    service: UaService::Browse
                }
            }
        ));
        assert!(engine.accept_at(&terminal, now).unwrap().is_some());
        assert!(engine.accept_at(&late, now).is_err());
    }

    #[test]
    fn recovery_cancels_views_and_resolve_and_beats_timeout() {
        use opclook_contract::model::{UaRecoveryState, UaService};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let views = engine
            .send(sid(1), Protocol::OpcUa, CommandKind::BrowseViews, ())
            .unwrap();
        let path = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::ResolveBrowsePath {
                    start_node: None,
                    path: "old".into(),
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Session(rid(77)),
                    kind: EventKind::UaRecovery {
                        state: UaRecoveryState::ConnectionLost { status: 1 },
                    },
                },
                now,
            )
            .unwrap();
        engine.expire_requests(now + Duration::from_secs(120), 1);
        for (id, kind) in [
            (views, EventKind::Views { children: vec![] }),
            (path, EventKind::PathResolved { targets: vec![] }),
        ] {
            assert!(
                engine
                    .accept_at(
                        &Event {
                            owner: owner(1),
                            origin: Origin::Reply(id),
                            kind
                        },
                        now
                    )
                    .is_err()
            );
        }
        let mut seen = Vec::new();
        for _ in 0..2 {
            let terminal = engine.try_recv().unwrap();
            if let EventKind::Error {
                message: BackendMsg::UaIdentityChanged { service },
                ..
            } = &terminal.kind
            {
                seen.push(*service);
            } else {
                panic!("typed cancellation expected");
            }
            engine.accept_at(&terminal, now).unwrap();
        }
        assert!(seen.contains(&UaService::BrowseViews));
        assert!(seen.contains(&UaService::TranslatePath));
        assert!(engine.try_recv().is_none());
    }

    #[test]
    fn recovery_is_epoch_guarded_and_schedules_one_fresh_snapshot_per_valid_key() {
        use opclook_contract::model::{ReadNeed, UaRecoveryMode, UaRecoveryState};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let old_read = dispatch_due(&mut engine, now);
        let event = |origin, state| Event {
            owner: owner(1),
            origin,
            kind: EventKind::UaRecovery { state },
        };
        let lost = UaRecoveryState::ConnectionLost { status: 0x80000000 };
        assert!(
            engine
                .accept_at(&event(Origin::Session(rid(78)), lost.clone()), now)
                .is_err()
        );
        engine
            .accept_at(&event(Origin::Session(rid(77)), lost), now)
            .unwrap();
        assert!(engine.session(sid(1)).unwrap().ua_subscriptions.is_empty());
        assert!(engine.session(sid(1)).unwrap().ua_history_uncertain);
        assert!(!engine.session(sid(1)).unwrap().ua_history_incomplete);
        assert!(
            engine
                .due(now + Duration::from_secs(10), Duration::ZERO, None)
                .is_empty()
        );
        assert!(
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(old_read),
                        kind: EventKind::Values { values: vec![] }
                    },
                    now
                )
                .is_err()
        );
        let reconnected = UaRecoveryState::Reconnected {
            mode: UaRecoveryMode::NewSession,
        };
        engine
            .accept_at(&event(Origin::Session(rid(77)), reconnected.clone()), now)
            .unwrap();
        let reconciled = UaRecoveryState::Reconciled {
            missing: vec![key(2)],
            ambiguous: true,
        };
        engine
            .accept_at(&event(Origin::Session(rid(77)), reconciled.clone()), now)
            .unwrap();
        assert!(
            engine
                .accept_at(&event(Origin::Session(rid(77)), reconciled), now)
                .is_err()
        );
        assert!(engine.request_refresh(sid(1), key(2)).is_err());
        let due = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("one snapshot")
        };
        assert_eq!(keys, &vec![key(1)]);
        let read = engine
            .send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ())
            .unwrap();
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::Read { .. }
        ));
        let pending = engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(read),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap()
            .unwrap();
        assert!(
            pending.read_terminals[0]
                .needs
                .contains(&ReadNeed::RecoverySnapshot)
        );
        assert!(engine.session(sid(1)).unwrap().ua_history_uncertain);
        assert!(!engine.session(sid(1)).unwrap().ua_history_incomplete);
        assert!(
            engine
                .accept_at(&event(Origin::Session(rid(77)), reconnected), now)
                .is_err()
        );
        engine
            .accept_at(
                &event(
                    Origin::Session(rid(77)),
                    UaRecoveryState::Stopped { status: 1 },
                ),
                now,
            )
            .unwrap();
        assert_eq!(
            engine.session(sid(1)).unwrap().subscription,
            SubscriptionState::Failed
        );
        assert!(
            engine
                .due(now + Duration::from_secs(10), Duration::ZERO, None)
                .is_empty()
        );
    }

    fn dispatch_due(engine: &mut Engine<()>, now: Instant) -> RequestId {
        let due = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("expected one read")
        };
        engine
            .send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ())
            .unwrap()
    }

    #[test]
    fn native_da_cache_poll_refresh_and_direct_read_keep_permit_and_freshness() {
        use opclook_contract::model::{AcquisitionConfig, PollFreshness};
        if !supports_fresh_source(Protocol::OpcDa) {
            return;
        }
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint = EndpointSpec::da("", "test");
        view.endpoint.acquisition = Some(AcquisitionConfig::da_available());
        let due = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("poll due")
        };
        let (keys, permit) = (keys.clone(), *permit);
        let id = engine
            .send_due_read(sid(1), Protocol::OpcDa, keys.clone(), permit, ())
            .unwrap();
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::ReadCached {
                max_age_ms: 1000,
                ..
            }
        ));
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap();
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcDa, keys, permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcDa,
                CommandKind::ReadFresh { keys: vec![key(1)] },
                ()
            ),
            Err(DispatchError::ReadBusy)
        ));
        engine.request_refresh(sid(1), key(1)).unwrap();
        let due = engine.due(now + Duration::from_secs(2), Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("refresh due")
        };
        assert!(keys.contains(&key(1)) && keys.contains(&key(2)));
        let id = engine
            .send_due_read(sid(1), Protocol::OpcDa, keys.clone(), *permit, ())
            .unwrap();
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::ReadFresh { .. }
        ));
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap();
        engine
            .send(
                sid(1),
                Protocol::OpcDa,
                CommandKind::Read { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::ReadFresh { .. }
        ));
        assert_eq!(
            AcquisitionConfig::da_available().poll_freshness,
            PollFreshness::CacheAllowed
        );
    }

    #[test]
    fn native_da_accepted_profile_write_schedules_fresh_readback() {
        use opclook_contract::model::{AcquisitionConfig, Value, WriteOutcome, WriteRequest};
        if !supports_fresh_source(Protocol::OpcDa) {
            return;
        }
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        let mut endpoint = EndpointSpec::da("", "test");
        let mut profile = AcquisitionConfig::da_available();
        profile.read_after_write = true;
        endpoint.acquisition = Some(profile);
        view.endpoint = endpoint;
        let id = engine
            .send(
                sid(1),
                Protocol::OpcDa,
                CommandKind::Write {
                    request: WriteRequest {
                        key: key(1),
                        value: Value::I64(7),
                        type_hint: None,
                    },
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Wrote {
                        key: key(1),
                        result: Ok(WriteOutcome::from_da(0, "ok")),
                    },
                },
                now,
            )
            .unwrap();
        let due = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("readback due")
        };
        let id = engine
            .send_due_read(sid(1), Protocol::OpcDa, keys.clone(), *permit, ())
            .unwrap();
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::ReadFresh { .. }
        ));
        let pending = engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap()
            .unwrap();
        assert!(pending.read_terminals.iter().any(|terminal| {
            terminal.key == key(1)
                && terminal
                    .needs
                    .contains(&opclook_contract::model::ReadNeed::WriteReadback)
        }));
    }

    #[test]
    fn cached_poll_uses_requested_interval_but_direct_read_stays_fresh() {
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let id = dispatch_due(&mut engine, now);
        assert!(
            matches!(&engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::ReadCached { keys, max_age_ms: 1234 } if keys == &vec![key(1), key(2)])
        );
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap();
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::ReadCached {
                    keys: vec![key(1)],
                    max_age_ms: 1234
                },
                ()
            ),
            Err(DispatchError::ReadBusy)
        ));
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::Read { .. }
        ));
    }

    #[test]
    fn coalesced_refresh_and_readback_force_fresh_poll_batch() {
        use opclook_contract::model::{Value, WriteOutcome, WriteRequest};
        let now = Instant::now();
        for readback in [false, true] {
            let mut engine = cached_poll_engine(now);
            if readback {
                let id = engine
                    .send_write_with_readback(
                        sid(1),
                        Protocol::OpcUa,
                        WriteRequest {
                            key: key(1),
                            value: Value::I64(7),
                            type_hint: None,
                        },
                        (),
                    )
                    .unwrap();
                engine
                    .accept_at(
                        &Event {
                            owner: owner(1),
                            origin: Origin::Reply(id),
                            kind: EventKind::Wrote {
                                key: key(1),
                                result: Ok(WriteOutcome::from_ua(0, "good")),
                            },
                        },
                        now,
                    )
                    .unwrap();
            } else {
                engine.request_refresh(sid(1), key(1)).unwrap();
            }
            dispatch_due(&mut engine, now);
            assert!(
                matches!(&engine.test_commands.borrow().last().unwrap().kind,
                CommandKind::Read { keys } if keys == &vec![key(1), key(2)])
            );
        }
    }

    #[test]
    fn stale_cached_poll_permit_cannot_dispatch() {
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let due = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("read due")
        };
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::RemoveItems { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        assert!(
            engine
                .test_commands
                .borrow()
                .iter()
                .all(|command| !matches!(command.kind, CommandKind::ReadCached { .. }))
        );
    }

    #[test]
    fn subscription_initial_snapshot_keeps_fresh_read() {
        use opclook_contract::model::UpdateMode;
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint.acquisition.as_mut().unwrap().update_mode = UpdateMode::Subscription;
        view.pending_initial
            .insert(key(1), now - Duration::from_secs(2));
        dispatch_due(&mut engine, now);
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::Read { .. }
        ));
    }

    fn negotiated_sub(
        id: u32,
        item: ItemKey,
        ms: u64,
        valid: bool,
    ) -> opclook_contract::model::UaSubscriptionParameters {
        use opclook_contract::model::{
            UaMonitoredParameters, UaRevisedSubscription, UaSubscriptionParameters,
        };
        UaSubscriptionParameters {
            subscription_id: id,
            requested_publishing_ms: 1234,
            requested_lifetime: 60,
            requested_keepalive: 20,
            revised: Some(UaRevisedSubscription {
                publishing_ms: ms,
                lifetime: 60,
                keepalive: 20,
                valid,
            }),
            items: vec![UaMonitoredParameters {
                key: item,
                requested_sampling_ms: -1.0,
                requested_queue_size: 1,
                revised_sampling_ms: ms as f64,
                revised_queue_size: 2,
            }],
        }
    }

    fn negotiated_engine(now: Instant) -> Engine<()> {
        use opclook_contract::model::UpdateMode;
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint.acquisition.as_mut().unwrap().update_mode = UpdateMode::Subscription;
        view.pending_initial.insert(key(1), now);
        view.pending_initial.insert(key(2), now);
        let event = Event {
            owner: owner(1),
            origin: Origin::Session(rid(77)),
            kind: EventKind::UaSubscriptionParameters {
                subscriptions: vec![
                    negotiated_sub(1, key(1), 2000, true),
                    negotiated_sub(2, key(2), 3000, true),
                ],
            },
        };
        engine.accept_at(&event, now).unwrap();
        engine
    }

    #[test]
    fn sdk_revised_lifetime_ratio_checked_without_overflow() {
        use opclook_contract::model::UaRevisedSubscription;
        assert!(UaRevisedSubscription::from_sdk(2000, 60, 20).valid);
        assert!(!UaRevisedSubscription::from_sdk(2000, 59, 20).valid);
        assert!(!UaRevisedSubscription::from_sdk(2000, u32::MAX, u32::MAX).valid);
        assert!(!UaRevisedSubscription::from_sdk(0, 60, 20).valid);
        assert!(UaRevisedSubscription::from_sdk(3_600_000, 60, 20).valid);
        let too_large = UaRevisedSubscription::from_sdk(u64::MAX, 60, 20);
        assert!(!too_large.valid);
        assert_eq!(too_large.publishing_ms, u64::MAX);
        assert!(!UaRevisedSubscription::from_sdk(2000, 60, 0).valid);
    }

    #[test]
    fn revised_subscription_grace_is_per_item_and_cached_batch_uses_minimum() {
        let now = Instant::now();
        let mut engine = negotiated_engine(now);
        assert!(
            engine
                .due(now + Duration::from_millis(1999), Duration::ZERO, None)
                .is_empty()
        );
        let due = engine.due(now + Duration::from_millis(2000), Duration::ZERO, None);
        assert!(matches!(due.as_slice(), [Due::Read { keys, .. }] if keys == &vec![key(1)]));
        let mut engine = negotiated_engine(now);
        dispatch_due(&mut engine, now + Duration::from_millis(3000));
        assert!(
            matches!(&engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::ReadCached { keys, max_age_ms: 2000 } if keys == &vec![key(1), key(2)])
        );
    }

    #[test]
    fn revised_snapshot_stale_epoch_and_invalid_values_never_authorize_cache() {
        let now = Instant::now();
        let mut engine = negotiated_engine(now);
        let stale = Event {
            owner: owner(1),
            origin: Origin::Session(rid(76)),
            kind: EventKind::UaSubscriptionParameters {
                subscriptions: vec![],
            },
        };
        assert!(engine.accept_at(&stale, now).is_err());
        assert_eq!(engine.sessions[&sid(1)].ua_subscriptions.len(), 2);
        let invalid = Event {
            owner: owner(1),
            origin: Origin::Session(rid(77)),
            kind: EventKind::UaSubscriptionParameters {
                subscriptions: vec![negotiated_sub(1, key(1), 2000, false)],
            },
        };
        engine.accept_at(&invalid, now).unwrap();
        dispatch_due(&mut engine, now + Duration::from_millis(3000));
        assert!(matches!(
            &engine.test_commands.borrow().last().unwrap().kind,
            CommandKind::Read { .. }
        ));
    }

    #[test]
    fn revised_subscription_mixed_explicit_refresh_forces_fresh() {
        let now = Instant::now();
        let mut engine = negotiated_engine(now);
        engine.request_refresh(sid(1), key(1)).unwrap();
        dispatch_due(&mut engine, now + Duration::from_millis(3000));
        assert!(
            matches!(&engine.test_commands.borrow().last().unwrap().kind, CommandKind::Read { keys } if keys == &vec![key(1), key(2)])
        );
    }

    fn terminal_observer(
        engine: &mut Engine<()>,
    ) -> std::sync::Arc<
        std::sync::Mutex<
            Vec<(
                SessionId,
                Option<RequestId>,
                opclook_contract::model::ReadTerminal,
            )>,
        >,
    > {
        let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = output.clone();
        engine.set_read_terminal_sink(Some(Box::new(move |session, epoch, terminal| {
            captured
                .lock()
                .unwrap()
                .push((session, epoch, terminal.clone()));
        })));
        output
    }

    #[test]
    fn read_terminals_are_per_key_and_preserve_later_demands_and_quality() {
        use opclook_contract::model::{DataValue, QualityKind, ReadNeed, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let captured = terminal_observer(&mut engine);
        engine.request_refresh(sid(1), key(1)).unwrap();
        let read = dispatch_due(&mut engine, now);
        engine.request_refresh(sid(1), key(1)).unwrap();
        let push = Event {
            owner: owner(1),
            origin: Origin::Session(rid(77)),
            kind: EventKind::Values { values: vec![] },
        };
        // Poll mode rejects unsolicited values; it cannot complete the read.
        assert!(engine.accept_at(&push, now).is_err());
        assert!(captured.lock().unwrap().is_empty());
        let response = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values {
                values: vec![(
                    key(1),
                    DataValue::bad(BackendMsg::Raw("native failure".into())),
                )],
            },
        };
        let pending = engine.accept_at(&response, now).unwrap().unwrap();
        assert_eq!(pending.read_terminals.len(), 2);
        let a = pending
            .read_terminals
            .iter()
            .find(|r| r.key == key(1))
            .unwrap();
        assert!(a.needs.contains(&ReadNeed::ExplicitRefresh));
        assert!(
            matches!(&a.status, ReadTerminalStatus::Received(v) if v.quality.kind == QualityKind::Bad && v.reason.is_some())
        );
        assert!(matches!(
            pending
                .read_terminals
                .iter()
                .find(|r| r.key == key(2))
                .unwrap()
                .status,
            ReadTerminalStatus::MissingResult
        ));
        assert_eq!(captured.lock().unwrap().len(), 2);
        assert!(engine.accept_at(&response, now).is_err());
        assert_eq!(captured.lock().unwrap().len(), 2);
        assert!(
            engine.sessions[&sid(1)]
                .pending_demands
                .contains(&ReadDemand {
                    key: key(1),
                    kind: ReadDemandKind::ExplicitRefresh
                })
        );
        let next = dispatch_due(&mut engine, now);
        let failed = Event {
            owner: owner(1),
            origin: Origin::Reply(next),
            kind: EventKind::Error {
                operation: Operation::Read,
                message: BackendMsg::NoConnection,
            },
        };
        let outcome = engine.accept_at(&failed, now).unwrap().unwrap();
        assert!(matches!(
            outcome.read_terminals[0].status,
            ReadTerminalStatus::ServiceError(BackendMsg::NoConnection)
        ));
        assert_eq!(captured.lock().unwrap().len(), 3);
    }

    #[test]
    fn remove_cancels_inflight_and_new_demand_once_without_harming_other_keys() {
        use opclook_contract::model::{DataValue, ReadCancelReason, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let captured = terminal_observer(&mut engine);
        engine.request_refresh(sid(1), key(1)).unwrap();
        let read = dispatch_due(&mut engine, now);
        engine.request_refresh(sid(1), key(1)).unwrap();
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::RemoveItems { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        let snapshot = captured.lock().unwrap().clone();
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot.iter().all(|(_, epoch, t)| *epoch == Some(rid(77))
            && t.key == key(1)
            && matches!(
                t.status,
                ReadTerminalStatus::Cancelled(ReadCancelReason::Removed)
            )));
        assert!(snapshot.iter().any(|(_, _, t)| t.request_id == Some(read)));
        assert!(snapshot.iter().any(|(_, _, t)| t.request_id.is_none()));
        let reply = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values {
                values: vec![
                    (key(1), DataValue::bad(BackendMsg::NoConnection)),
                    (key(2), DataValue::bad(BackendMsg::NoConnection)),
                ],
            },
        };
        let result = engine.accept_at(&reply, now).unwrap().unwrap();
        assert_eq!(result.read_terminals.len(), 1);
        assert_eq!(result.read_terminals[0].key, key(2));
        engine.manual_disconnect(sid(1));
        assert_eq!(captured.lock().unwrap().len(), 3);
        assert!(engine.accept_at(&reply, now).is_err());
    }

    #[test]
    fn synthetic_timeout_is_one_terminal_and_late_reply_is_ignored() {
        use opclook_contract::model::ReadTerminalStatus;
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let captured = terminal_observer(&mut engine);
        engine.request_refresh(sid(1), key(1)).unwrap();
        let read = dispatch_due(&mut engine, now);
        engine.expire_requests(now + Duration::from_secs(100), 1);
        let racing = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values { values: vec![] },
        };
        assert!(engine.accept_at(&racing, now).is_err());
        assert!(engine.has_pending(sid(1), read));
        let timeout = engine.try_recv().unwrap();
        let pending = engine.accept_at(&timeout, now).unwrap().unwrap();
        assert!(
            pending
                .read_terminals
                .iter()
                .all(|t| matches!(t.status, ReadTerminalStatus::Timeout { .. }))
        );
        assert_eq!(captured.lock().unwrap().len(), 2);
        let late = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values { values: vec![] },
        };
        assert!(engine.accept_at(&late, now).is_err());
        assert_eq!(captured.lock().unwrap().len(), 2);
    }

    #[test]
    fn observer_is_optional_and_disconnect_cancels_pending_and_inflight() {
        use opclook_contract::model::{ReadCancelReason, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let read = dispatch_due(&mut engine, now);
        let event = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values { values: vec![] },
        };
        assert_eq!(
            engine
                .accept_at(&event, now)
                .unwrap()
                .unwrap()
                .read_terminals
                .len(),
            2
        );
        let mut engine = cached_poll_engine(now);
        let captured = terminal_observer(&mut engine);
        let read = dispatch_due(&mut engine, now);
        engine.request_refresh(sid(1), key(1)).unwrap();
        engine.manual_disconnect(sid(1));
        engine.manual_disconnect(sid(1));
        let seen = captured.lock().unwrap().clone();
        assert_eq!(seen.len(), 3);
        assert!(seen.iter().all(|(_, _, t)| matches!(
            t.status,
            ReadTerminalStatus::Cancelled(ReadCancelReason::Disconnected)
        )));
        let late = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values { values: vec![] },
        };
        assert!(engine.accept_at(&late, now).is_err());
        assert_eq!(captured.lock().unwrap().len(), 3);
    }

    #[test]
    fn terminal_untracked_reconnect_discards_old_read_and_pending_once() {
        use opclook_contract::model::{ReadCancelReason, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let observed = terminal_observer(&mut engine);
        let id = dispatch_due(&mut engine, now);
        engine.request_refresh(sid(1), key(1)).unwrap();
        engine
            .disconnect_untracked(sid(1), Protocol::OpcUa)
            .unwrap();
        let seen = observed.lock().unwrap().clone();
        assert_eq!(seen.len(), 3);
        assert!(
            seen.iter()
                .all(|(_, epoch, terminal)| *epoch == Some(rid(77))
                    && matches!(
                        terminal.status,
                        ReadTerminalStatus::Cancelled(ReadCancelReason::EpochChanged)
                    ))
        );
        assert!(!engine.has_pending(sid(1), id));
        assert!(!engine.sessions[&sid(1)].read_in_flight);
        let late = Event {
            owner: owner(1),
            origin: Origin::Reply(id),
            kind: EventKind::Values { values: vec![] },
        };
        assert!(engine.accept_at(&late, now).is_err());
        assert_eq!(observed.lock().unwrap().len(), 3);
    }

    #[test]
    fn terminal_discard_and_invalidate_emit_cancellation_once() {
        use opclook_contract::model::{ReadCancelReason, ReadTerminalStatus};
        let now = Instant::now();
        for invalidate in [false, true] {
            let mut engine = cached_poll_engine(now);
            let observed = terminal_observer(&mut engine);
            let read = dispatch_due(&mut engine, now);
            if invalidate {
                engine.invalidate_session_requests(sid(1));
            } else {
                engine.discard_pending(sid(1), read);
            }
            engine.discard_pending(sid(1), read);
            let seen = observed.lock().unwrap().clone();
            assert_eq!(seen.len(), 2);
            assert!(
                seen.iter()
                    .all(|(_, _, terminal)| terminal.request_id == Some(read)
                        && matches!(
                            terminal.status,
                            ReadTerminalStatus::Cancelled(ReadCancelReason::Cancelled)
                        ))
            );
            assert!(!engine.sessions[&sid(1)].read_in_flight);
        }
    }

    #[test]
    fn terminal_removed_during_reservation_does_not_recancel_requeued_key() {
        use opclook_contract::model::{ReadCancelReason, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let observed = terminal_observer(&mut engine);
        engine.request_refresh(sid(1), key(1)).unwrap();
        engine.request_refresh(sid(1), key(2)).unwrap();
        let due = engine.due(now, Duration::ZERO, None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("read")
        };
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::RemoveItems { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        assert!(engine.sessions[&sid(1)].reserved_initial.is_empty());
        assert!(engine.sessions[&sid(1)].reserved_poll.is_empty());
        engine.manual_disconnect(sid(1));
        let seen = observed.lock().unwrap().clone();
        assert_eq!(
            seen.iter()
                .filter(|(_, _, t)| t.key == key(1)
                    && matches!(
                        t.status,
                        ReadTerminalStatus::Cancelled(ReadCancelReason::Removed)
                    ))
                .count(),
            1
        );
        assert_eq!(
            seen.iter()
                .filter(|(_, _, t)| t.key == key(2)
                    && matches!(
                        t.status,
                        ReadTerminalStatus::Cancelled(ReadCancelReason::Disconnected)
                    ))
                .count(),
            1
        );
    }

    #[test]
    fn terminal_exhausted_ids_and_permits_report_unavailable() {
        use opclook_contract::model::ReadTerminalStatus;
        let now = Instant::now();
        for exhaust_permit in [false, true] {
            let mut engine = cached_poll_engine(now);
            let observed = terminal_observer(&mut engine);
            engine.request_refresh(sid(1), key(1)).unwrap();
            if exhaust_permit {
                engine.next_permit = None;
                assert!(matches!(
                    engine.due(now, Duration::ZERO, None).as_slice(),
                    [Due::ReadUnavailable { .. }]
                ));
            } else {
                let due = engine.due(now, Duration::ZERO, None);
                let [Due::Read { keys, permit, .. }] = due.as_slice() else {
                    panic!("read")
                };
                engine.next = None;
                assert!(matches!(
                    engine.send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ()),
                    Err(DispatchError::Exhausted)
                ));
            }
            let seen = observed.lock().unwrap().clone();
            assert_eq!(seen.len(), 2);
            assert!(seen.iter().all(|(_, _, t)| t.request_id.is_none()
                && matches!(t.status, ReadTerminalStatus::Unavailable)));
        }
    }

    #[test]
    fn terminal_subscription_push_does_not_cover_readback() {
        use opclook_contract::model::{ReadNeed, UpdateMode};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        engine
            .sessions
            .get_mut(&sid(1))
            .unwrap()
            .endpoint
            .acquisition
            .as_mut()
            .unwrap()
            .update_mode = UpdateMode::Subscription;
        let observed = terminal_observer(&mut engine);
        engine
            .enqueue_demand(sid(1), key(1), ReadDemandKind::WriteReadback)
            .unwrap();
        let read = dispatch_due(&mut engine, now);
        let push = Event {
            owner: owner(1),
            origin: Origin::Session(rid(77)),
            kind: EventKind::Values { values: vec![] },
        };
        engine.accept_at(&push, now).unwrap();
        assert!(engine.has_pending(sid(1), read));
        assert!(observed.lock().unwrap().is_empty());
        let response = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values { values: vec![] },
        };
        let terminal = engine.accept_at(&response, now).unwrap().unwrap();
        assert_eq!(terminal.read_terminals.len(), 1);
        assert!(
            terminal.read_terminals[0]
                .needs
                .contains(&ReadNeed::WriteReadback)
        );
        assert_eq!(observed.lock().unwrap().len(), 1);
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn terminal_dispatch_failure_is_reported_without_retry() {
        use opclook_contract::model::ReadTerminalStatus;
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let observed = terminal_observer(&mut engine);
        engine.request_refresh(sid(1), key(1)).unwrap();
        let due = engine.due(now, Duration::ZERO, None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("read")
        };
        engine.limit_dispatch_for_test(0);
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ()),
            Err(DispatchError::Closed { .. })
        ));
        let seen = observed.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert!(seen.iter().all(|(_, _, t)| matches!(
            t.status,
            ReadTerminalStatus::DispatchFailed(SendError::WorkerClosed)
        )));
        assert!(engine.due(now, Duration::ZERO, None).is_empty());
    }

    #[test]
    fn terminal_test_failure_and_shutdown_cancel_pending_reads() {
        use opclook_contract::model::{ReadCancelReason, ReadTerminalStatus};
        let now = Instant::now();
        for test_failure in [false, true] {
            let mut engine = cached_poll_engine(now);
            let observed = terminal_observer(&mut engine);
            let read = dispatch_due(&mut engine, now);
            if test_failure {
                let test = engine
                    .send(sid(1), Protocol::OpcUa, CommandKind::Test, ())
                    .unwrap();
                engine
                    .accept_at(
                        &Event {
                            owner: owner(1),
                            origin: Origin::Reply(test),
                            kind: EventKind::Error {
                                operation: Operation::Test,
                                message: BackendMsg::NoConnection,
                            },
                        },
                        now,
                    )
                    .unwrap();
            } else {
                engine.shutdown();
            }
            let seen = observed.lock().unwrap().clone();
            assert_eq!(seen.len(), 2);
            let expected = if test_failure {
                ReadCancelReason::Disconnected
            } else {
                ReadCancelReason::Cancelled
            };
            assert!(seen.iter().all(|(_, _, t)| t.request_id == Some(read) && matches!(t.status, ReadTerminalStatus::Cancelled(reason) if reason == expected)));
            assert!(!engine.has_pending(sid(1), read));
        }
    }

    #[test]
    fn terminal_duplicate_results_are_not_reported_as_success() {
        use opclook_contract::model::{DataValue, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let observed = terminal_observer(&mut engine);
        let read = dispatch_due(&mut engine, now);
        let value = DataValue::bad(BackendMsg::NoConnection);
        let result = engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(read),
                    kind: EventKind::Values {
                        values: vec![(key(1), value.clone()), (key(1), value)],
                    },
                },
                now,
            )
            .unwrap()
            .unwrap();
        assert!(matches!(
            result
                .read_terminals
                .iter()
                .find(|t| t.key == key(1))
                .unwrap()
                .status,
            ReadTerminalStatus::UnexpectedReply
        ));
        assert!(matches!(
            result
                .read_terminals
                .iter()
                .find(|t| t.key == key(2))
                .unwrap()
                .status,
            ReadTerminalStatus::MissingResult
        ));
        assert_eq!(observed.lock().unwrap().len(), 2);
    }

    #[test]
    fn terminal_write_readback_after_send_requires_next_attempt() {
        use opclook_contract::model::{ReadNeed, UpdateMode, Value, WriteOutcome, WriteRequest};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        engine
            .sessions
            .get_mut(&sid(1))
            .unwrap()
            .endpoint
            .acquisition
            .as_mut()
            .unwrap()
            .update_mode = UpdateMode::Subscription;
        let observed = terminal_observer(&mut engine);
        let request = WriteRequest {
            key: key(1),
            value: Value::I64(7),
            type_hint: None,
        };
        let write = engine
            .send_write_with_readback(sid(1), Protocol::OpcUa, request.clone(), ())
            .unwrap();
        let wrote = |id| Event {
            owner: owner(1),
            origin: Origin::Reply(id),
            kind: EventKind::Wrote {
                key: key(1),
                result: Ok(WriteOutcome::from_ua(0, "good")),
            },
        };
        engine.accept_at(&wrote(write), now).unwrap();
        let first = dispatch_due(&mut engine, now);
        let next_write = engine
            .send_write_with_readback(sid(1), Protocol::OpcUa, request, ())
            .unwrap();
        engine.accept_at(&wrote(next_write), now).unwrap();
        let reply = |id| Event {
            owner: owner(1),
            origin: Origin::Reply(id),
            kind: EventKind::Values { values: vec![] },
        };
        let first_result = engine.accept_at(&reply(first), now).unwrap().unwrap();
        assert_eq!(first_result.read_terminals.len(), 1);
        assert!(
            first_result.read_terminals[0]
                .needs
                .contains(&ReadNeed::WriteReadback)
        );
        assert!(
            engine.sessions[&sid(1)]
                .pending_demands
                .contains(&ReadDemand {
                    key: key(1),
                    kind: ReadDemandKind::WriteReadback
                })
        );
        let second = dispatch_due(&mut engine, now);
        let second_result = engine.accept_at(&reply(second), now).unwrap().unwrap();
        assert_eq!(second_result.read_terminals.len(), 1);
        assert!(
            second_result.read_terminals[0]
                .needs
                .contains(&ReadNeed::WriteReadback)
        );
        assert_ne!(first, second);
        assert_eq!(observed.lock().unwrap().len(), 2);
    }

    #[test]
    fn terminal_uncertain_quality_and_native_reason_are_preserved() {
        use opclook_contract::model::{DataValue, Quality, QualityKind, ReadTerminalStatus, Value};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let read = dispatch_due(&mut engine, now);
        let value = DataValue {
            value: Value::I64(9),
            quality: Quality::from_ua(0x4000_0000, "uncertain"),
            timestamp: None,
            reason: Some(BackendMsg::Raw("native-detail".into())),
            times: Default::default(),
        };
        let reply = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values {
                values: vec![(key(1), value)],
            },
        };
        let result = engine.accept_at(&reply, now).unwrap().unwrap();
        assert!(
            matches!(&result.read_terminals.iter().find(|t| t.key == key(1)).unwrap().status,
            ReadTerminalStatus::Received(v) if v.quality.kind == QualityKind::Uncertain && matches!(&v.reason, Some(BackendMsg::Raw(detail)) if detail == "native-detail"))
        );
    }

    #[test]
    fn initial_push_before_add_reply_emits_only_after_success() {
        use opclook_contract::model::{
            DataValue, ItemInfo, ItemSpec, ReadNeed, ReadTerminalStatus,
        };
        let now = Instant::now();
        for accepted in [false, true] {
            let mut engine = cached_poll_engine(now);
            let view = engine.sessions.get_mut(&sid(1)).unwrap();
            view.endpoint.acquisition.as_mut().unwrap().update_mode =
                opclook_contract::model::UpdateMode::Subscription;
            view.items.clear();
            let observed = terminal_observer(&mut engine);
            let add = engine
                .send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::AddItems {
                        items: vec![ItemSpec {
                            key: key(1),
                            item_id: "tag".into(),
                        }],
                    },
                    (),
                )
                .unwrap();
            let push = Event {
                owner: owner(1),
                origin: Origin::Session(rid(77)),
                kind: EventKind::Values {
                    values: vec![(key(1), DataValue::bad(BackendMsg::NoConnection))],
                },
            };
            engine.accept_at(&push, now).unwrap();
            assert!(observed.lock().unwrap().is_empty());
            let result = if accepted {
                Ok(ItemInfo {
                    data_type: None,
                    type_id: None,
                    writable: false,
                })
            } else {
                Err(BackendMsg::NoConnection)
            };
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(add),
                        kind: EventKind::AddItems {
                            results: vec![(key(1), result)],
                        },
                    },
                    now,
                )
                .unwrap();
            let seen = observed.lock().unwrap().clone();
            assert_eq!(seen.len(), usize::from(accepted));
            if accepted {
                assert!(matches!(
                    &seen[0].2.status,
                    ReadTerminalStatus::InitialPush {
                        reason: Some(BackendMsg::NoConnection),
                        ..
                    }
                ));
                assert_eq!(seen[0].2.needs, vec![ReadNeed::Initial]);
            }
        }
    }

    #[test]
    fn initial_push_after_add_completes_only_once() {
        use opclook_contract::model::{DataValue, ReadTerminalStatus};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint.acquisition.as_mut().unwrap().update_mode =
            opclook_contract::model::UpdateMode::Subscription;
        view.pending_initial.insert(key(1), now);
        let observed = terminal_observer(&mut engine);
        let push = Event {
            owner: owner(1),
            origin: Origin::Session(rid(77)),
            kind: EventKind::Values {
                values: vec![(key(1), DataValue::bad(BackendMsg::NoConnection))],
            },
        };
        engine.accept_at(&push, now).unwrap();
        engine.accept_at(&push, now).unwrap();
        let seen = observed.lock().unwrap().clone();
        assert_eq!(seen.len(), 1);
        assert!(matches!(
            &seen[0].2.status,
            ReadTerminalStatus::InitialPush { .. }
        ));
    }

    #[test]
    fn caller_read_deadline_queues_one_correlated_terminal_and_rejects_late() {
        use opclook_contract::model::ReadTerminalStatus;
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let observed = terminal_observer(&mut engine);
        let read = dispatch_due(&mut engine, now);
        assert!(engine.timeout_read_request(sid(1), read, 1));
        assert!(!engine.timeout_read_request(sid(1), read, 1));
        let late = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values { values: vec![] },
        };
        assert!(engine.accept_at(&late, now).is_err());
        let timeout = engine.try_recv().unwrap();
        let result = engine.accept_at(&timeout, now).unwrap().unwrap();
        assert!(
            result
                .read_terminals
                .iter()
                .all(|t| matches!(t.status, ReadTerminalStatus::Timeout { seconds: 1 }))
        );
        assert_eq!(observed.lock().unwrap().len(), 2);
        assert!(!engine.timeout_read_request(sid(1), read, 1));
    }

    #[test]
    fn initial_waiting_and_old_epoch_push_never_finish_initial_need() {
        use opclook_contract::model::{DataValue, ItemInfo, ItemSpec, Quality, Value};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint.acquisition.as_mut().unwrap().update_mode =
            opclook_contract::model::UpdateMode::Subscription;
        view.items.clear();
        let observed = terminal_observer(&mut engine);
        let add = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: key(1),
                        item_id: "tag".into(),
                    }],
                },
                (),
            )
            .unwrap();
        let value = DataValue {
            value: Value::Empty,
            quality: Quality::from_ua(0x8032_0000, "waiting"),
            timestamp: None,
            reason: None,
            times: Default::default(),
        };
        let push = |epoch| Event {
            owner: owner(1),
            origin: Origin::Session(epoch),
            kind: EventKind::Values {
                values: vec![(key(1), value.clone())],
            },
        };
        assert!(engine.accept_at(&push(rid(76)), now).is_err());
        engine.accept_at(&push(rid(77)), now).unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(add),
                    kind: EventKind::AddItems {
                        results: vec![(
                            key(1),
                            Ok(ItemInfo {
                                data_type: None,
                                type_id: None,
                                writable: false,
                            }),
                        )],
                    },
                },
                now,
            )
            .unwrap();
        assert!(observed.lock().unwrap().is_empty());
        assert!(
            engine.sessions[&sid(1)]
                .pending_initial
                .contains_key(&key(1))
        );
    }

    #[test]
    fn initial_push_during_reserved_read_does_not_finish_read_need() {
        use opclook_contract::model::{DataValue, UpdateMode};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint.acquisition.as_mut().unwrap().update_mode = UpdateMode::Subscription;
        view.pending_initial
            .insert(key(1), now - Duration::from_secs(2));
        let observed = terminal_observer(&mut engine);
        let due = engine.due(now, Duration::ZERO, None);
        assert!(matches!(due.as_slice(), [Due::Read { .. }]));
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Session(rid(77)),
                    kind: EventKind::Values {
                        values: vec![(key(1), DataValue::bad(BackendMsg::NoConnection))],
                    },
                },
                now,
            )
            .unwrap();
        assert!(observed.lock().unwrap().is_empty());
    }

    #[test]
    fn duplicate_registration_never_emits_initial_push_success() {
        use opclook_contract::model::{DataValue, ItemInfo, ItemSpec};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let view = engine.sessions.get_mut(&sid(1)).unwrap();
        view.endpoint.acquisition.as_mut().unwrap().update_mode =
            opclook_contract::model::UpdateMode::Subscription;
        view.items.clear();
        let observed = terminal_observer(&mut engine);
        let add = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: key(1),
                        item_id: "tag".into(),
                    }],
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Session(rid(77)),
                    kind: EventKind::Values {
                        values: vec![(key(1), DataValue::bad(BackendMsg::NoConnection))],
                    },
                },
                now,
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(add),
                    kind: EventKind::AddItems {
                        results: vec![
                            (
                                key(1),
                                Ok(ItemInfo {
                                    data_type: None,
                                    type_id: None,
                                    writable: false,
                                }),
                            ),
                            (key(1), Err(BackendMsg::NoConnection)),
                        ],
                    },
                },
                now,
            )
            .unwrap();
        assert!(observed.lock().unwrap().is_empty());
        assert!(!engine.sessions[&sid(1)].items.contains_key(&key(1)));
    }

    #[test]
    fn read_terminal_and_metadata_observer_keep_time_facts() {
        use opclook_contract::model::{DataValue, ReadTerminalStatus, SourceClockId, ValueOrigin};
        let now = Instant::now();
        let mut engine = cached_poll_engine(now);
        let mut value = DataValue::bad(BackendMsg::NoConnection);
        value.times.origin = ValueOrigin::Read;
        value.times.source_clock = Some(SourceClockId(42));
        let buffer = std::sync::Arc::new(std::sync::Mutex::new(
            opclook_contract::read_observer::ReadObservations::default(),
        ));
        let sink = buffer.clone();
        engine.set_read_terminal_sink(Some(Box::new(move |session, epoch, terminal| {
            sink.lock().unwrap().record(session, epoch, terminal)
        })));
        let read = dispatch_due(&mut engine, now);
        let event = Event {
            owner: owner(1),
            origin: Origin::Reply(read),
            kind: EventKind::Values {
                values: vec![(key(1), value)],
            },
        };
        let result = engine.accept_at(&event, now).unwrap().unwrap();
        assert!(
            matches!(&result.read_terminals.iter().find(|t| t.key == key(1)).unwrap().status, ReadTerminalStatus::Received(v) if v.times.origin == ValueOrigin::Read && v.times.source_clock == Some(SourceClockId(42)))
        );
        let batch = buffer.lock().unwrap().drain();
        assert!(batch.latest.iter().any(|entry| matches!(&entry.terminal.status, ReadTerminalStatus::ReceivedMetadata { times, .. } if times.origin == ValueOrigin::Read && times.source_clock == Some(SourceClockId(42)))));
    }

    #[test]
    fn action_payload_limits_follow_availability_and_never_dispatch() {
        use opclook_contract::actions::{ACTION_MAX_BYTES, ActionPayloadError, DomainAction};
        use opclook_contract::model::{NodeClassFilter, Value, WriteRequest};
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let make_write = || WriteRequest {
            key: key(1),
            value: Value::Str("x".repeat(ACTION_MAX_BYTES + 1)),
            type_hint: None,
        };
        let action = || DomainAction::Write {
            session: sid(1),
            protocol: Protocol::OpcUa,
            request: make_write(),
        };
        let oversized_browse = || DomainAction::Browse {
            session: sid(1),
            protocol: Protocol::OpcUa,
            node_id: "x".repeat(ACTION_MAX_BYTES),
            filter: NodeClassFilter::All,
            view_id: Some("y".into()),
        };
        assert!(matches!(
            engine.execute_action(action(), ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionMissing
            ))
        ));
        assert!(matches!(
            engine.execute_action(oversized_browse(), ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionMissing
            ))
        ));
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now());
        engine.sessions.insert(sid(1), view.clone());
        assert!(matches!(
            engine.execute_action(action(), ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionNotConnected
            ))
        ));
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        engine.sessions.insert(sid(1), view);
        assert!(matches!(
            engine.send_write_with_readback(sid(1), Protocol::OpcUa, make_write(), ()),
            Err(DemandWriteError::Demand(DemandError::ItemNotRegistered))
        ));
        engine
            .sessions
            .get_mut(&sid(1))
            .unwrap()
            .items
            .insert(key(1), false);
        assert!(matches!(
            engine.send_write_with_readback(sid(1), Protocol::OpcUa, make_write(), ()),
            Err(DemandWriteError::Dispatch(
                DispatchError::InvalidActionPayload(ActionPayloadError::Bytes)
            ))
        ));
        assert!(matches!(
            engine.execute_action(action(), ()),
            Err(DispatchError::InvalidActionPayload(
                ActionPayloadError::Bytes
            ))
        ));
        assert!(matches!(
            engine.execute_action(oversized_browse(), ()),
            Err(DispatchError::InvalidActionPayload(
                ActionPayloadError::Bytes
            ))
        ));
        assert_eq!(engine.next, Some(RequestId::FIRST));
        assert!(engine.requests.is_empty());
        assert!(engine.test_commands.borrow().is_empty());
        let valid = DomainAction::Write {
            session: sid(1),
            protocol: Protocol::OpcUa,
            request: WriteRequest {
                key: key(1),
                value: Value::Array(vec![Value::I64(42)]),
                type_hint: None,
            },
        };
        assert_eq!(engine.execute_action(valid, ()).unwrap(), RequestId::FIRST);
        assert_eq!(engine.test_commands.borrow().len(), 1);
        engine.next = None;
        assert!(matches!(
            engine.execute_action(action(), ()),
            Err(DispatchError::Exhausted)
        ));
    }

    #[test]
    fn browse_action_availability_is_pure_and_execution_correlates_request() {
        use opclook_contract::actions::DomainAction;
        use opclook_contract::model::NodeClassFilter;
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let action = DomainAction::Browse {
            session: sid(1),
            protocol: Protocol::OpcUa,
            node_id: "ns=2;s=Plant".into(),
            filter: NodeClassFilter::Variables,
            view_id: None,
        };
        assert_eq!(
            engine.action_unavailable(&action),
            Some(ActionUnavailable::SessionMissing)
        );
        assert!(matches!(
            engine.execute_action(action.clone(), ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionMissing
            ))
        ));
        let mut runtime =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now());
        engine.sessions.insert(sid(1), runtime.clone());
        assert_eq!(engine.action_unavailable(&action), None);
        runtime.state = SessionState::Disconnected;
        engine.sessions.insert(sid(1), runtime.clone());
        assert_eq!(
            engine.action_unavailable(&action),
            Some(ActionUnavailable::SessionNotConnected)
        );
        assert!(matches!(
            engine.execute_action(action.clone(), ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionNotConnected
            ))
        ));
        runtime.state = SessionState::Connected;
        runtime.endpoint = EndpointSpec::da("", "test");
        engine.sessions.insert(sid(1), runtime.clone());
        assert_eq!(
            engine.action_unavailable(&action),
            Some(ActionUnavailable::ProtocolMismatch)
        );
        assert!(matches!(
            engine.execute_action(action.clone(), ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::ProtocolMismatch
            ))
        ));
        runtime.endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        engine.sessions.insert(sid(1), runtime);
        for _ in 0..3 {
            assert_eq!(engine.action_unavailable(&action), None);
        }
        assert!(engine.requests.is_empty());
        assert_eq!(engine.next, Some(RequestId::FIRST));
        let request = engine.execute_action(action.clone(), ()).unwrap();
        assert_eq!(request, RequestId::FIRST);
        assert_eq!(
            engine.requests.get(&(owner(1), request)).unwrap().operation,
            Operation::Browse
        );
        engine.next = None;
        assert_eq!(
            engine.action_unavailable(&action),
            Some(ActionUnavailable::RequestIdsExhausted)
        );
        assert!(matches!(
            engine.execute_action(action, ()),
            Err(DispatchError::Exhausted)
        ));
        assert_eq!(engine.requests.len(), 1);
    }
    #[test]
    fn write_during_connecting_is_rejected_without_allocating_or_dispatching() {
        use opclook_contract::actions::DomainAction;
        use opclook_contract::model::{Value, WriteRequest};
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        engine.sessions.insert(
            sid(1),
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now()),
        );
        let action = DomainAction::Write {
            session: sid(1),
            protocol: Protocol::OpcUa,
            request: WriteRequest {
                key: ItemKey::FIRST,
                value: Value::I64(7),
                type_hint: None,
            },
        };
        assert_eq!(
            engine.action_unavailable(&action),
            Some(ActionUnavailable::SessionNotConnected)
        );
        assert!(matches!(
            engine.execute_action(action, ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionNotConnected
            ))
        ));
        assert_eq!(engine.next, Some(RequestId::FIRST));
        assert!(engine.requests.is_empty());
        let request = WriteRequest {
            key: ItemKey::FIRST,
            value: Value::I64(7),
            type_hint: None,
        };
        assert!(matches!(
            engine.send(sid(1), Protocol::OpcUa, CommandKind::Write { request }, ()),
            Err(DispatchError::ActionUnavailable(
                ActionUnavailable::SessionNotConnected
            ))
        ));
        assert_eq!(engine.next, Some(RequestId::FIRST));
        assert!(engine.requests.is_empty());
    }
    #[test]
    fn disconnect_blocks_writes_before_state_changes() {
        use opclook_contract::actions::DomainAction;
        use opclook_contract::model::{Value, WriteRequest};
        for tracked in [false, true] {
            let mut engine = Engine::<()>::default();
            engine.test_skip_dispatch = true;
            let mut view =
                SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now());
            view.state = SessionState::Connected;
            view.connection_id = Some(rid(77));
            engine.sessions.insert(sid(1), view);
            if tracked {
                engine
                    .send(sid(1), Protocol::OpcUa, CommandKind::Disconnect, ())
                    .unwrap();
            } else {
                engine
                    .disconnect_untracked(sid(1), Protocol::OpcUa)
                    .unwrap();
            }
            assert_eq!(
                engine.session(sid(1)).unwrap().state,
                SessionState::Connected
            );
            let action = DomainAction::Write {
                session: sid(1),
                protocol: Protocol::OpcUa,
                request: WriteRequest {
                    key: ItemKey::FIRST,
                    value: Value::I64(7),
                    type_hint: None,
                },
            };
            let next = engine.next;
            let count = engine.requests.len();
            assert_eq!(
                engine.action_unavailable(&action),
                Some(ActionUnavailable::SessionNotConnected)
            );
            assert!(matches!(
                engine.execute_action(action.clone(), ()),
                Err(DispatchError::ActionUnavailable(
                    ActionUnavailable::SessionNotConnected
                ))
            ));
            if let DomainAction::Write { request, .. } = action {
                assert!(matches!(
                    engine.send(sid(1), Protocol::OpcUa, CommandKind::Write { request }, ()),
                    Err(DispatchError::ActionUnavailable(
                        ActionUnavailable::SessionNotConnected
                    ))
                ));
            }
            assert_eq!(engine.next, next);
            assert_eq!(engine.requests.len(), count);
        }
    }
    #[test]
    fn discovery_and_session_correlation_are_isolated() {
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let session = sid(1);
        let request = rid(42);
        engine.requests.insert(
            (Owner::Discovery, request),
            PendingRequest {
                operation: Operation::Discover,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.requests.insert(
            (Owner::Session(session), request),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        let discovery = Event {
            owner: Owner::Discovery,
            origin: Origin::Reply(request),
            kind: EventKind::Servers {
                host: String::new(),
                servers: vec![],
            },
        };
        engine.accept(&discovery).unwrap();
        assert!(engine.has_pending(session, request));
        assert!(engine.accept(&discovery).is_err());
        let session_reply = Event {
            owner: Owner::Session(session),
            origin: Origin::Reply(request),
            kind: EventKind::Values { values: vec![] },
        };
        engine.accept(&session_reply).unwrap();
        assert!(!engine.has_pending(session, request));
    }
    #[test]
    fn discovery_timeout_has_discovery_owner_and_does_not_disconnect_a_session() {
        let mut engine = Engine::<()>::default();
        let request = rid(12);
        engine.requests.insert(
            (Owner::Discovery, request),
            PendingRequest {
                operation: Operation::Discover,
                sent: Instant::now() - Duration::from_secs(100),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        assert!(engine.expire_requests(Instant::now(), 1).is_empty());
        let event = engine.route_events.pop_front().unwrap();
        assert_eq!(event.owner, Owner::Discovery);
        assert_eq!(event.origin, Origin::Reply(request));
        engine.accept(&event).unwrap();
        assert!(engine.accept(&event).is_err());
    }
    #[test]
    fn rejected_reads_preserve_real_pending_and_release_unsent_poll() {
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.items.insert(ItemKey::FIRST, false);
        engine.sessions.insert(sid(1), view);
        engine.next = RequestId::new(u64::MAX);
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read {
                    keys: vec![ItemKey::FIRST],
                },
                (),
            )
            .unwrap();
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![] },
                ()
            ),
            Err(DispatchError::Exhausted)
        ));
        assert!(engine.session(sid(1)).unwrap().read_in_flight);
        engine.discard_pending(sid(1), id);
        engine.sessions.get_mut(&sid(1)).unwrap().read_in_flight = true;
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![] },
                ()
            ),
            Err(DispatchError::Exhausted)
        ));
        assert!(!engine.session(sid(1)).unwrap().read_in_flight);
    }
    #[test]
    fn request_exhaustion_keeps_pending_work_and_reports_da_error() {
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now());
        view.state = SessionState::Connected;
        engine.sessions.insert(sid(1), view);
        engine.next = RequestId::new(u64::MAX);
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read {
                    keys: vec![ItemKey::FIRST],
                },
                (),
            )
            .unwrap();
        assert_eq!(id.get(), u64::MAX);
        assert!(engine.has_pending(sid(1), id));
        assert!(matches!(
            engine.send(sid(1), Protocol::OpcUa, CommandKind::Disconnect, ()),
            Err(DispatchError::Exhausted)
        ));
        assert!(engine.has_pending(sid(1), id));
        let result = engine.prepare_da(
            Owner::Discovery,
            CommandKind::Discover {
                host: String::new(),
            },
            (),
            RouteOptions {
                adapter: None,
                local_addresses: vec![],
                deadline: Instant::now() + Duration::from_secs(1),
            },
        );
        assert_eq!(result, Err(DaRouteError::RequestIdsExhausted));
        assert!(engine.route_service.is_none());
    }
    #[test]
    fn da_preparation_rejects_ua_endpoint() {
        let mut engine = Engine::<()>::default();
        let result = engine.prepare_da(
            sid(1),
            CommandKind::Connect {
                endpoint: EndpointSpec::ua("opc.tcp://127.0.0.1:4840"),
            },
            (),
            RouteOptions {
                adapter: None,
                local_addresses: vec![],
                deadline: Instant::now() + Duration::from_secs(1),
            },
        );
        assert!(result.is_err());
        assert!(engine.session(sid(1)).is_none());
    }
    #[test]
    fn cancelled_route_preparation_never_dispatches_late_connect() {
        let mut engine = Engine::<()>::default();
        let id = engine
            .prepare_da(
                sid(1),
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("127.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    adapter: None,
                    local_addresses: vec![],
                    deadline: Instant::now() + Duration::from_secs(2),
                },
            )
            .unwrap();
        engine
            .accept(&Event {
                origin: Origin::Reply(id),
                owner: owner(1),
                kind: EventKind::Error {
                    operation: Operation::Connect,
                    message: BackendMsg::raw("timed out"),
                },
            })
            .unwrap();
        assert!(!engine.has_da_route(sid(1)));
        engine.pump_da_routes();
        assert!(engine.route_events.is_empty());
        assert!(!engine.has_pending(sid(1), id));
    }
    #[test]
    fn route_preparation_dispatches_and_engine_owns_the_ticket() {
        let mut engine = Engine::<()>::default();
        let id = engine
            .prepare_da(
                sid(1),
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("127.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    adapter: None,
                    local_addresses: vec![],
                    deadline: Instant::now() + Duration::from_secs(2),
                },
            )
            .unwrap();
        assert_eq!(engine.session(sid(1)).unwrap().connection_id, Some(id));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !engine.session_routes.contains_key(&sid(1)) && Instant::now() < deadline {
            engine.pump_da_routes();
            assert!(engine.route_events.is_empty());
            std::thread::yield_now();
        }
        assert!(engine.session_routes.contains_key(&sid(1)));
        assert!(engine.has_pending(sid(1), id));
        engine.invalidate_session_requests(sid(1));
        assert!(
            engine.session_routes.contains_key(&sid(1)),
            "request cleanup must retain connected route"
        );
        engine.cancel_da_routes(sid(1));
        assert!(!engine.has_da_route(sid(1)));
    }
    #[test]
    fn route_failure_keeps_request_for_correlated_error() {
        let mut engine = Engine::<()>::default();
        let id = engine
            .prepare_da(
                sid(1),
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("10.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    adapter: None,
                    local_addresses: vec![],
                    deadline: Instant::now() + Duration::from_secs(2),
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let issue = loop {
            engine.pump_da_routes();
            if let Some(issue) = engine.route_events.pop_front() {
                break Some(issue);
            }
            if Instant::now() >= deadline {
                break None;
            }
            std::thread::yield_now();
        };
        assert!(
            matches!(issue, Some(Event { origin: Origin::Reply(request_id), kind: EventKind::Error { message: BackendMsg::DaRoute(DaRouteError::NoAdapter), .. }, .. }) if request_id == id)
        );
        assert!(engine.has_pending(sid(1), id));
    }
    #[test]
    fn route_loss_during_connect_completes_connect_and_rejects_late_reply() {
        let mut engine = Engine::<()>::default();
        let endpoint = EndpointSpec::da("10.0.0.1", "Srv");
        engine.sessions.insert(
            sid(1),
            SessionRuntime::new(endpoint.clone(), Instant::now()),
        );
        engine.requests.insert(
            (owner(1), rid(5)),
            PendingRequest {
                operation: Operation::Connect,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.on_da_route_lost(sid(1), DaRouteError::HelperUnavailable("lost".into()));
        let event = engine.route_events.pop_front().unwrap();
        assert_eq!(event.origin, Origin::Reply(rid(5)));
        assert!(matches!(
            event.kind,
            EventKind::Error {
                operation: Operation::Connect,
                ..
            }
        ));
        engine.accept(&event).unwrap();
        assert_eq!(
            engine.session(sid(1)).unwrap().state,
            SessionState::Disconnected
        );
        assert!(
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(rid(5)),
                    kind: EventKind::Connected {
                        endpoint,
                        info: BackendMsg::raw("late")
                    }
                })
                .is_err()
        );
    }
    #[test]
    fn route_loss_when_connected_invalidates_requests_and_disconnects() {
        let mut engine = Engine::<()>::default();
        let mut view = SessionRuntime::new(EndpointSpec::da("10.0.0.1", "Srv"), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(7)),
            PendingRequest {
                operation: Operation::Write,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.on_da_route_lost(sid(1), DaRouteError::HelperUnavailable("lost".into()));
        assert!(!engine.has_pending(owner(1), rid(7)));
        let event = engine.route_events.pop_front().unwrap();
        assert_eq!(event.origin, Origin::Session(rid(77)));
        engine.accept(&event).unwrap();
        assert_eq!(
            engine.session(sid(1)).unwrap().state,
            SessionState::Disconnected
        );
        assert_eq!(
            engine.invalidate_session_requests(sid(1)),
            vec![Operation::Write]
        );
    }
    #[test]
    fn reconnect_duration_is_bounded_for_fractional_and_extreme_values() {
        let base = Duration::from_millis(250);
        let max = Duration::from_secs(2);
        assert_eq!(reconnect_delay(1, base, max), base);
        assert_eq!(reconnect_delay(3, base, max), Duration::from_secs(1));
        assert_eq!(reconnect_delay(u32::MAX, base, max), max);
        assert_eq!(reconnect_delay(u32::MAX, Duration::MAX, max), max);
        assert_eq!(
            reconnect_delay(u32::MAX, Duration::from_nanos(1), Duration::MAX),
            Duration::MAX
        );
        assert_eq!(
            reconnect_delay(u32::MAX, Duration::ZERO, Duration::MAX),
            Duration::ZERO
        );
        assert_eq!(request_timeout(Operation::Read, 0), constants::REQUEST_MIN);
        assert_eq!(
            request_timeout(Operation::Connect, 0),
            constants::REQUEST_SLOW_MIN
        );
    }

    #[test]
    fn timeout_is_one_correlated_event_and_late_reply_is_dropped() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.read_in_flight = true;
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(19)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now() - Duration::from_secs(100),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.expire_requests(Instant::now(), 1);
        engine.expire_requests(Instant::now(), 1);
        assert_eq!(engine.route_events.len(), 1);
        let timeout = engine.route_events.pop_front().unwrap();
        assert_eq!(timeout.origin, Origin::Reply(rid(19)));
        assert!(matches!(
            timeout.kind,
            EventKind::Error {
                operation: Operation::Read,
                message: BackendMsg::RequestTimeout { .. }
            }
        ));
        engine.accept(&timeout).unwrap();
        assert!(!engine.session(sid(1)).unwrap().read_in_flight);
        assert!(
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(rid(19)),
                    kind: EventKind::Values { values: vec![] }
                })
                .is_err()
        );
    }
    fn key(raw: u32) -> ItemKey {
        ItemKey::new(raw).unwrap()
    }
    fn pending_add_engine() -> Engine<()> {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(9)),
            PendingRequest {
                operation: Operation::AddItems,
                sent: Instant::now() - Duration::from_secs(100),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.pending_adds.insert(
            (owner(1), rid(9)),
            [
                (key(2), PendingAdd::default()),
                (key(3), PendingAdd::default()),
            ]
            .into_iter()
            .collect(),
        );
        engine
    }
    #[test]
    fn c14_c22_connection_profiles_are_snapshotted_and_poll_independently() {
        use opclook_contract::model::{AcquisitionConfig, ItemInfo, ItemSpec, UpdateMode};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        for (session, interval) in [(sid(1), 200), (sid(2), 1000)] {
            let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
            let mut profile = AcquisitionConfig::ua_default();
            profile.update_mode = UpdateMode::Poll;
            profile.interval_ms = interval;
            endpoint.acquisition = Some(profile.clone());
            let connect = engine
                .send(
                    session,
                    Protocol::OpcUa,
                    CommandKind::Connect {
                        endpoint: endpoint.clone(),
                    },
                    (),
                )
                .unwrap();
            let mut echoed = endpoint.clone();
            echoed.acquisition = None;
            engine
                .accept_at(
                    &Event {
                        owner: Owner::Session(session),
                        origin: Origin::Reply(connect),
                        kind: EventKind::Connected {
                            endpoint: echoed,
                            info: BackendMsg::raw("connected"),
                        },
                    },
                    now,
                )
                .unwrap();
            assert_eq!(
                engine.session(session).unwrap().endpoint.acquisition,
                Some(profile)
            );
            let add = engine
                .send(
                    session,
                    Protocol::OpcUa,
                    CommandKind::AddItems {
                        items: vec![ItemSpec {
                            key: key(1),
                            item_id: "a".into(),
                        }],
                    },
                    (),
                )
                .unwrap();
            engine
                .accept_at(
                    &Event {
                        owner: Owner::Session(session),
                        origin: Origin::Reply(add),
                        kind: EventKind::AddItems {
                            results: vec![(
                                key(1),
                                Ok(ItemInfo {
                                    data_type: None,
                                    type_id: None,
                                    writable: true,
                                }),
                            )],
                        },
                    },
                    now,
                )
                .unwrap();
        }
        let epoch = engine.sessions[&sid(1)].connection_id.unwrap();
        assert!(
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Session(epoch),
                        kind: EventKind::Values { values: vec![] }
                    },
                    now
                )
                .is_err()
        );
        let due = engine.due(now, Duration::from_secs(60), None);
        assert_eq!(due.len(), 2);
        for work in due {
            let Due::Read {
                session,
                protocol,
                keys,
                permit,
            } = work
            else {
                panic!("read")
            };
            let id = engine
                .send_due_read(session, protocol, keys, permit, ())
                .unwrap();
            engine
                .accept_at(
                    &Event {
                        owner: Owner::Session(session),
                        origin: Origin::Reply(id),
                        kind: EventKind::Values { values: vec![] },
                    },
                    now,
                )
                .unwrap();
        }
        let due = engine.due(
            now + Duration::from_millis(500),
            Duration::from_secs(500),
            None,
        );
        assert!(matches!(due.as_slice(), [Due::Read { session, .. }] if *session == sid(1)));
    }
    #[test]
    fn c23_c24_profile_write_and_invalid_connect_guard() {
        use opclook_contract::model::{AcquisitionConfig, Value, WriteOutcome, WriteRequest};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let profile = AcquisitionConfig::ua_default();
        endpoint.acquisition = Some(profile.clone());
        let mut view = SessionRuntime::new(endpoint.clone(), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.last_poll = now;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let mut invalid = endpoint.clone();
        invalid.acquisition.as_mut().unwrap().interval_ms = 0;
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect { endpoint: invalid },
                ()
            ),
            Err(DispatchError::InvalidAcquisition(
                AcquisitionError::Interval
            ))
        ));
        assert_eq!(engine.session(sid(1)).unwrap().connection_id, Some(rid(77)));
        for (enabled, accepted) in [(false, true), (true, false), (true, true)] {
            engine
                .sessions
                .get_mut(&sid(1))
                .unwrap()
                .endpoint
                .acquisition
                .as_mut()
                .unwrap()
                .read_after_write = enabled;
            let id = engine
                .send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::Write {
                        request: WriteRequest {
                            key: key(1),
                            value: Value::I64(1),
                            type_hint: None,
                        },
                    },
                    (),
                )
                .unwrap();
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(id),
                        kind: EventKind::Wrote {
                            key: key(1),
                            result: Ok(WriteOutcome::from_ua(
                                if accepted { 0 } else { 0x80000000 },
                                "status",
                            )),
                        },
                    },
                    now,
                )
                .unwrap();
            let due = engine.due(now, Duration::from_secs(1), None);
            assert_eq!(!due.is_empty(), enabled && accepted);
        }
    }
    #[test]
    fn explicit_failed_subscription_has_no_hidden_periodic_poll() {
        use opclook_contract::model::AcquisitionConfig;
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        endpoint.acquisition = Some(AcquisitionConfig::ua_default());
        let mut view = SessionRuntime::new(endpoint, now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Failed;
        view.items.insert(key(1), true);
        view.last_poll = now;
        engine.sessions.insert(sid(1), view);
        assert!(
            engine
                .due(
                    now + Duration::from_secs(10),
                    Duration::from_millis(1),
                    None
                )
                .is_empty()
        );
    }

    #[test]
    fn old_read_reply_error_or_synthetic_timeout_does_not_cover_new_write() {
        use opclook_contract::model::{BackendMsg, DataValue, Value, WriteOutcome, WriteRequest};
        for old_terminal in 0..3 {
            let now = Instant::now();
            let mut engine = Engine::<()>::default();
            engine.test_skip_dispatch = true;
            let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
            view.state = SessionState::Connected;
            view.connection_id = Some(rid(77));
            view.subscription = SubscriptionState::Active;
            view.last_poll = now;
            view.items.insert(key(1), false);
            engine.sessions.insert(sid(1), view);
            let old = engine
                .send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::Read { keys: vec![key(1)] },
                    (),
                )
                .unwrap();
            let request = WriteRequest {
                key: key(1),
                value: Value::I64(7),
                type_hint: None,
            };
            for _ in 0..2 {
                let write = engine
                    .send_write_with_readback(sid(1), Protocol::OpcUa, request.clone(), ())
                    .unwrap();
                engine
                    .accept_at(
                        &Event {
                            owner: owner(1),
                            origin: Origin::Reply(write),
                            kind: EventKind::Wrote {
                                key: key(1),
                                result: Ok(WriteOutcome::from_ua(0, "Good")),
                            },
                        },
                        now,
                    )
                    .unwrap();
            }
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Session(rid(77)),
                        kind: EventKind::Values {
                            values: vec![(key(1), DataValue::bad(BackendMsg::raw("push")))],
                        },
                    },
                    now,
                )
                .unwrap();
            assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
            if old_terminal == 2 {
                engine.expire_requests(now + Duration::from_secs(120), 5);
                let timeout = engine.route_events.pop_front().unwrap();
                assert_eq!(timeout.origin, Origin::Reply(old));
                engine.accept_at(&timeout, now).unwrap();
            } else {
                let kind = if old_terminal == 0 {
                    EventKind::Values { values: vec![] }
                } else {
                    EventKind::Error {
                        operation: Operation::Read,
                        message: BackendMsg::raw("failed"),
                    }
                };
                engine
                    .accept_at(
                        &Event {
                            owner: owner(1),
                            origin: Origin::Reply(old),
                            kind,
                        },
                        now,
                    )
                    .unwrap();
            }
            assert!(
                engine
                    .accept_at(
                        &Event {
                            owner: owner(1),
                            origin: Origin::Reply(old),
                            kind: EventKind::Values { values: vec![] }
                        },
                        now
                    )
                    .is_err()
            );
            let due = engine.due(now, Duration::from_secs(60), None);
            let [Due::Read { keys, permit, .. }] = due.as_slice() else {
                panic!("read-back due")
            };
            assert_eq!(keys, &vec![key(1)]);
            let read = engine
                .send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ())
                .unwrap();
            let write = engine
                .send_write_with_readback(sid(1), Protocol::OpcUa, request.clone(), ())
                .unwrap();
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(write),
                        kind: EventKind::Wrote {
                            key: key(1),
                            result: Ok(WriteOutcome::from_ua(0x002e0000, "async")),
                        },
                    },
                    now,
                )
                .unwrap();
            let covered = engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(read),
                        kind: EventKind::Values { values: vec![] },
                    },
                    now,
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                covered.read_demands,
                vec![ReadDemand {
                    key: key(1),
                    kind: ReadDemandKind::WriteReadback
                }]
            );
            assert!(
                matches!(engine.due(now, Duration::from_secs(60), None).as_slice(), [Due::Read { keys, .. }] if keys == &vec![key(1)])
            );
        }
    }

    #[test]
    fn write_opt_in_only_accepts_matching_good_replies() {
        use opclook_contract::model::{BackendMsg, Value, WriteOutcome, WriteRequest};
        for mode in 0..5 {
            let now = Instant::now();
            let mut engine = Engine::<()>::default();
            engine.test_skip_dispatch = true;
            let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
            view.state = SessionState::Connected;
            view.connection_id = Some(rid(77));
            view.subscription = SubscriptionState::Active;
            view.last_poll = now;
            view.items.insert(key(1), false);
            engine.sessions.insert(sid(1), view);
            let request = WriteRequest {
                key: key(1),
                value: Value::I64(1),
                type_hint: None,
            };
            let id = if mode == 0 {
                engine
                    .send(sid(1), Protocol::OpcUa, CommandKind::Write { request }, ())
                    .unwrap()
            } else {
                engine
                    .send_write_with_readback(sid(1), Protocol::OpcUa, request, ())
                    .unwrap()
            };
            let result = match mode {
                1 => Ok(WriteOutcome::unknown("timeout")),
                2 => Ok(WriteOutcome::from_ua(0x80000000, "bad")),
                3 => Err(BackendMsg::NodeNotSubscribed),
                _ => Ok(WriteOutcome::from_ua(0, "good")),
            };
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(id),
                        kind: EventKind::Wrote {
                            key: if mode == 4 { key(2) } else { key(1) },
                            result,
                        },
                    },
                    now,
                )
                .unwrap();
            assert!(
                engine.due(now, Duration::from_secs(60), None).is_empty(),
                "mode {mode}"
            );
        }
    }

    #[test]
    fn new_write_after_reservation_and_remove_are_distinct() {
        use opclook_contract::model::{Value, WriteOutcome, WriteRequest};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.last_poll = now;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let request = WriteRequest {
            key: key(1),
            value: Value::I64(1),
            type_hint: None,
        };
        let first = engine
            .send_write_with_readback(sid(1), Protocol::OpcUa, request.clone(), ())
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(first),
                    kind: EventKind::Wrote {
                        key: key(1),
                        result: Ok(WriteOutcome::from_ua(0, "good")),
                    },
                },
                now,
            )
            .unwrap();
        let due = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = due.as_slice() else {
            panic!("due")
        };
        let second = engine
            .send_write_with_readback(sid(1), Protocol::OpcUa, request, ())
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(second),
                    kind: EventKind::Wrote {
                        key: key(1),
                        result: Ok(WriteOutcome::from_ua(0, "good")),
                    },
                },
                now,
            )
            .unwrap();
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
        let read = engine
            .send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ())
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(read),
                    kind: EventKind::Values { values: vec![] },
                },
                now,
            )
            .unwrap();
        let next = engine.due(now, Duration::from_secs(60), None);
        let [Due::Read { keys, permit, .. }] = next.as_slice() else {
            panic!("new write must remain pending")
        };
        assert_eq!(keys, &vec![key(1)]);
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::RemoveItems { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcUa, keys.clone(), *permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
    }

    #[test]
    fn old_write_and_read_replies_do_not_cross_reconnect() {
        use opclook_contract::model::{BackendMsg, Value, WriteOutcome, WriteRequest};
        let now = Instant::now();
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(endpoint.clone(), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.last_poll = now;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let read = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        let write = engine
            .send_write_with_readback(
                sid(1),
                Protocol::OpcUa,
                WriteRequest {
                    key: key(1),
                    value: Value::I64(1),
                    type_hint: None,
                },
                (),
            )
            .unwrap();
        let disconnect = engine
            .send(sid(1), Protocol::OpcUa, CommandKind::Disconnect, ())
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(disconnect),
                    kind: EventKind::Disconnected {
                        reason: BackendMsg::ConnectionClosed,
                    },
                },
                now,
            )
            .unwrap();
        let connect = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(connect),
                    kind: EventKind::Connected {
                        endpoint,
                        info: BackendMsg::raw("connected"),
                    },
                },
                now,
            )
            .unwrap();
        assert!(
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(write),
                        kind: EventKind::Wrote {
                            key: key(1),
                            result: Ok(WriteOutcome::from_ua(0, "good"))
                        }
                    },
                    now
                )
                .is_err()
        );
        assert!(
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(read),
                        kind: EventKind::Values { values: vec![] }
                    },
                    now
                )
                .is_err()
        );
        assert!(engine.sessions[&sid(1)].pending_demands.is_empty());
    }

    #[test]
    fn e06_active_subscription_cannot_release_reservation() {
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        engine.request_refresh(sid(1), key(1)).unwrap();
        let first = engine.due(now, Duration::from_secs(60), None);
        assert!(matches!(first.as_slice(), [Due::Read { .. }]));
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Session(rid(77)),
                    kind: EventKind::SubscriptionState {
                        state: SubscriptionState::Active,
                    },
                },
                now,
            )
            .unwrap();
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
    }

    #[test]
    fn unsupported_da_readback_fails_before_write_and_uncertain_never_queues() {
        use opclook_contract::model::{Value, WriteOutcome, WriteRequest};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::da("", "test"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let request = WriteRequest {
            key: key(1),
            value: Value::I64(1),
            type_hint: None,
        };
        if !supports_fresh_source(Protocol::OpcDa) {
            assert!(matches!(
                engine.request_refresh(sid(1), key(1)),
                Err(DemandError::UnsupportedFreshness)
            ));
            assert!(matches!(
                engine.send_write_with_readback(sid(1), Protocol::OpcDa, request.clone(), ()),
                Err(DemandWriteError::Demand(DemandError::UnsupportedFreshness))
            ));
            assert!(engine.requests.is_empty());
        }
        engine.sessions.get_mut(&sid(1)).unwrap().endpoint =
            EndpointSpec::ua("opc.tcp://localhost:4840");
        engine.sessions.get_mut(&sid(1)).unwrap().subscription = SubscriptionState::Active;
        let write = engine
            .send_write_with_readback(sid(1), Protocol::OpcUa, request, ())
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(write),
                    kind: EventKind::Wrote {
                        key: key(1),
                        result: Ok(WriteOutcome::from_ua(0x4000_0000, "uncertain")),
                    },
                },
                now,
            )
            .unwrap();
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
    }

    #[test]
    fn initial_snapshot_waits_for_inflight_read_and_coalesces() {
        use opclook_contract::model::{ItemInfo, ItemSpec};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let read = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        let add = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![
                        ItemSpec {
                            key: key(2),
                            item_id: "a".into(),
                        },
                        ItemSpec {
                            key: key(3),
                            item_id: "b".into(),
                        },
                    ],
                },
                (),
            )
            .unwrap();
        let info = ItemInfo {
            data_type: None,
            type_id: None,
            writable: false,
        };
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(add),
                kind: EventKind::AddItems {
                    results: vec![(key(2), Ok(info.clone())), (key(3), Ok(info))],
                },
            })
            .unwrap();
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(read),
                kind: EventKind::Values { values: vec![] },
            })
            .unwrap();
        let due = engine.due(now + Duration::from_secs(61), Duration::from_secs(60), None);
        assert!(
            matches!(due.as_slice(), [Due::Read { keys, .. }] if keys == &vec![key(2), key(3)])
        );
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key(1)] },
                ()
            ),
            Err(DispatchError::ReadBusy)
        ));
        let Due::Read { keys, permit, .. } = due.into_iter().next().unwrap() else {
            panic!("read due")
        };
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcDa, keys.clone(), permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        engine
            .send_due_read(sid(1), Protocol::OpcUa, keys, permit, ())
            .unwrap();
    }
    #[test]
    fn early_terminal_push_covers_initial_but_waiting_does_not() {
        use opclook_contract::model::{DataValue, ItemInfo, ItemSpec, Quality, Value};
        for (status, waiting) in [(0, false), (0x8000_0000, false), (0x8032_0000, true)] {
            let now = Instant::now();
            let mut engine = Engine::<()>::default();
            engine.test_skip_dispatch = true;
            let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
            view.state = SessionState::Connected;
            view.connection_id = Some(rid(77));
            view.subscription = SubscriptionState::Active;
            engine.sessions.insert(sid(1), view);
            let add = engine
                .send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::AddItems {
                        items: vec![ItemSpec {
                            key: key(2),
                            item_id: "a".into(),
                        }],
                    },
                    (),
                )
                .unwrap();
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Session(rid(77)),
                    kind: EventKind::Values {
                        values: vec![(
                            key(2),
                            DataValue {
                                value: Value::Empty,
                                quality: Quality::from_ua(status, "status"),
                                timestamp: None,
                                reason: None,
                                times: Default::default(),
                            },
                        )],
                    },
                })
                .unwrap();
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(add),
                    kind: EventKind::AddItems {
                        results: vec![(
                            key(2),
                            Ok(ItemInfo {
                                data_type: None,
                                type_id: None,
                                writable: false,
                            }),
                        )],
                    },
                })
                .unwrap();
            assert!(engine.due(now, Duration::from_secs(1), None).is_empty());
            let due = engine.due(now + Duration::from_secs(2), Duration::from_secs(1), None);
            assert_eq!(!due.is_empty(), waiting);
            assert!(matches!(
                engine.send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::AddItems {
                        items: vec![ItemSpec {
                            key: key(2),
                            item_id: "new".into()
                        }]
                    },
                    ()
                ),
                Err(DispatchError::ItemKeyReused)
            ));
        }
    }
    #[test]
    fn removed_item_invalidates_reserved_read_before_dispatch() {
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.force_poll = true;
        view.items.insert(key(1), false);
        view.pending_initial.insert(key(1), now);
        engine.sessions.insert(sid(1), view);
        let due = engine.due(now, Duration::from_secs(60), None);
        let Due::Read { keys, permit, .. } = due.into_iter().next().unwrap() else {
            panic!("read due")
        };
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::RemoveItems { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcUa, keys, permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        assert!(!engine.session(sid(1)).unwrap().read_in_flight);
        assert!(engine.due(now, Duration::from_secs(60), None).is_empty());
    }
    #[test]
    fn old_read_permit_cannot_claim_new_epoch_reservation() {
        use opclook_contract::model::{ItemInfo, ItemSpec};
        let now = Instant::now();
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(endpoint.clone(), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.force_poll = true;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let Due::Read {
            keys: old_keys,
            permit: old_permit,
            ..
        } = engine
            .due(now, Duration::ZERO, None)
            .into_iter()
            .next()
            .unwrap()
        else {
            panic!("read due")
        };
        let connect = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(connect),
                kind: EventKind::Connected {
                    endpoint,
                    info: BackendMsg::raw("connected"),
                },
            })
            .unwrap();
        let add = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: key(1),
                        item_id: "a".into(),
                    }],
                },
                (),
            )
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(add),
                kind: EventKind::AddItems {
                    results: vec![(
                        key(1),
                        Ok(ItemInfo {
                            data_type: None,
                            type_id: None,
                            writable: false,
                        }),
                    )],
                },
            })
            .unwrap();
        let Due::Read { keys, permit, .. } = engine
            .due(now, Duration::ZERO, None)
            .into_iter()
            .next()
            .unwrap()
        else {
            panic!("new read due")
        };
        assert!(matches!(
            engine.send_due_read(sid(1), Protocol::OpcUa, old_keys, old_permit, ()),
            Err(DispatchError::ReadBusy)
        ));
        assert!(engine.due(now, Duration::ZERO, None).is_empty());
        engine
            .send_due_read(sid(1), Protocol::OpcUa, keys, permit, ())
            .unwrap();
    }
    #[test]
    fn registration_identity_ledger_has_visible_limit() {
        use opclook_contract::model::ItemSpec;
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.used_keys
            .extend((1..=constants::MAX_ITEM_KEYS_PER_CONNECTION as u32).filter_map(ItemKey::new));
        engine.sessions.insert(sid(1), view);
        let item = ItemSpec {
            key: key(constants::MAX_ITEM_KEYS_PER_CONNECTION as u32 + 1),
            item_id: "a".into(),
        };
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::AddItems { items: vec![item] },
                ()
            ),
            Err(DispatchError::ItemKeyLimit)
        ));
    }
    #[test]
    fn legacy_failed_and_poll_only_continue_after_initial_read() {
        for state in [SubscriptionState::Failed, SubscriptionState::Active] {
            let now = Instant::now();
            let mut engine = Engine::<()>::default();
            engine.test_skip_dispatch = true;
            let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
            view.state = SessionState::Connected;
            view.connection_id = Some(rid(77));
            view.subscription = state;
            view.items.insert(key(1), true);
            view.pending_initial.insert(key(1), now);
            view.poll_defer_since.insert(key(1), now);
            engine.sessions.insert(sid(1), view);
            let Due::Read { keys, permit, .. } = engine
                .due(now + Duration::from_secs(2), Duration::from_secs(1), None)
                .into_iter()
                .next()
                .unwrap()
            else {
                panic!("initial read")
            };
            let id = engine
                .send_due_read(sid(1), Protocol::OpcUa, keys, permit, ())
                .unwrap();
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                })
                .unwrap();
            assert!(matches!(
                engine
                    .due(now + Duration::from_secs(4), Duration::from_secs(1), None)
                    .as_slice(),
                [Due::Read { .. }]
            ));
        }
    }
    #[test]
    fn read_error_does_not_erase_later_initial_demand() {
        use opclook_contract::model::{ItemInfo, ItemSpec};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        let old = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key(1)] },
                (),
            )
            .unwrap();
        let add = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: key(2),
                        item_id: "b".into(),
                    }],
                },
                (),
            )
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(add),
                kind: EventKind::AddItems {
                    results: vec![(
                        key(2),
                        Ok(ItemInfo {
                            data_type: None,
                            type_id: None,
                            writable: false,
                        }),
                    )],
                },
            })
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(old),
                kind: EventKind::Error {
                    operation: Operation::Read,
                    message: BackendMsg::raw("failed"),
                },
            })
            .unwrap();
        assert!(
            matches!(engine.due(now + Duration::from_secs(2), Duration::from_secs(1), None).as_slice(), [Due::Read { keys, .. }] if keys == &vec![key(2)])
        );
    }
    #[test]
    fn repeated_add_batches_do_not_starve_older_poll_keys() {
        use opclook_contract::model::{ItemInfo, ItemSpec};
        let now = Instant::now();
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Failed;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        for (n, millis) in [(2, 1500), (3, 2400)] {
            let add = engine
                .send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::AddItems {
                        items: vec![ItemSpec {
                            key: key(n),
                            item_id: n.to_string(),
                        }],
                    },
                    (),
                )
                .unwrap();
            engine
                .accept_at(
                    &Event {
                        owner: owner(1),
                        origin: Origin::Reply(add),
                        kind: EventKind::AddItems {
                            results: vec![(
                                key(n),
                                Ok(ItemInfo {
                                    data_type: None,
                                    type_id: None,
                                    writable: false,
                                }),
                            )],
                        },
                    },
                    now + Duration::from_millis(millis),
                )
                .unwrap();
            let due = engine.due(
                now + Duration::from_millis(millis),
                Duration::from_secs(1),
                None,
            );
            let Due::Read { keys, permit, .. } = due.into_iter().next().unwrap() else {
                panic!("poll due")
            };
            assert!(keys.contains(&key(1)));
            let id = engine
                .send_due_read(sid(1), Protocol::OpcUa, keys, permit, ())
                .unwrap();
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(id),
                    kind: EventKind::Values { values: vec![] },
                })
                .unwrap();
        }
    }
    #[test]
    fn add_results_install_only_confirmed_keys_and_preserve_early_poll_only() {
        let mut engine = pending_add_engine();
        engine
            .accept(&Event {
                origin: Origin::Session(rid(77)),
                owner: owner(1),
                kind: EventKind::PollOnly {
                    keys: vec![key(2), key(3)],
                },
            })
            .unwrap();
        let info = opclook_contract::model::ItemInfo {
            data_type: None,
            type_id: None,
            writable: false,
        };
        engine
            .accept(&Event {
                origin: Origin::Reply(rid(9)),
                owner: owner(1),
                kind: EventKind::AddItems {
                    results: vec![
                        (key(2), Ok(info)),
                        (key(3), Err(BackendMsg::raw("rejected"))),
                    ],
                },
            })
            .unwrap();
        assert_eq!(
            engine.session(sid(1)).unwrap().items.get(&key(1)),
            Some(&false)
        );
        assert_eq!(
            engine.session(sid(1)).unwrap().items.get(&key(2)),
            Some(&true)
        );
        assert!(!engine.session(sid(1)).unwrap().items.contains_key(&key(3)));
        let due = engine.due(Instant::now(), Duration::from_secs(1), None);
        assert!(matches!(due.as_slice(), [Due::Read { keys, .. }] if keys == &vec![key(2)]));
    }
    #[test]
    fn removing_a_key_before_add_reply_does_not_reinstall_it() {
        let mut engine = pending_add_engine();
        engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::RemoveItems { keys: vec![key(2)] },
                (),
            )
            .unwrap();
        let info = opclook_contract::model::ItemInfo {
            data_type: None,
            type_id: None,
            writable: false,
        };
        engine
            .accept(&Event {
                origin: Origin::Reply(rid(9)),
                owner: owner(1),
                kind: EventKind::AddItems {
                    results: vec![(key(2), Ok(info))],
                },
            })
            .unwrap();
        assert!(!engine.session(sid(1)).unwrap().items.contains_key(&key(2)));
    }
    #[test]
    fn reconnect_drops_old_confirmations_before_failed_readd() {
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:0");
        let mut view = SessionRuntime::new(endpoint.clone(), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.subscription = SubscriptionState::Active;
        view.items.insert(key(4), true);
        engine.sessions.insert(sid(1), view);
        let connect = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        assert!(engine.session(sid(1)).unwrap().items.is_empty());
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(connect),
                kind: EventKind::Connected {
                    endpoint,
                    info: BackendMsg::raw("connected"),
                },
            })
            .unwrap();
        engine.requests.insert(
            (owner(1), rid(50)),
            PendingRequest {
                operation: Operation::AddItems,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.pending_adds.insert(
            (owner(1), rid(50)),
            [(key(4), PendingAdd::default())].into_iter().collect(),
        );
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(rid(50)),
                kind: EventKind::AddItems {
                    results: vec![(key(4), Err(BackendMsg::raw("rejected")))],
                },
            })
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Session(connect),
                kind: EventKind::SubscriptionState {
                    state: SubscriptionState::Active,
                },
            })
            .unwrap();
        assert!(engine.session(sid(1)).unwrap().items.is_empty());
        assert!(engine.due(Instant::now(), Duration::ZERO, None).is_empty());
    }
    #[test]
    fn legacy_add_error_uses_correlated_operation_identity() {
        let mut engine = pending_add_engine();
        engine.route_events.push_back(Event {
            origin: Origin::Reply(rid(9)),
            owner: owner(1),
            kind: EventKind::Error {
                operation: Operation::Add,
                message: BackendMsg::raw("failed"),
            },
        });
        let event = engine.try_recv().unwrap();
        assert!(matches!(
            event.kind,
            EventKind::Error {
                operation: Operation::AddItems,
                ..
            }
        ));
        engine.accept(&event).unwrap();
        assert!(engine.pending_adds.is_empty());
    }
    #[test]
    fn failed_or_expired_add_never_installs_requested_items() {
        for timeout in [false, true] {
            let mut engine = pending_add_engine();
            let event = if timeout {
                engine.expire_requests(Instant::now(), 1);
                engine.route_events.pop_front().unwrap()
            } else {
                Event {
                    origin: Origin::Reply(rid(9)),
                    owner: owner(1),
                    kind: EventKind::Error {
                        operation: Operation::Add,
                        message: BackendMsg::raw("failed"),
                    },
                }
            };
            engine.accept(&event).unwrap();
            assert_eq!(engine.session(sid(1)).unwrap().items.len(), 1);
            assert!(engine.pending_adds.is_empty());
            assert!(
                engine
                    .due(Instant::now(), Duration::from_secs(1), None)
                    .is_empty()
            );
        }
    }
    #[test]
    fn duplicate_disconnect_does_not_advance_retry_but_failed_attempt_does() {
        let mut engine = Engine::<()>::default();
        engine.set_auto_reconnect(true);
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        engine.sessions.insert(sid(1), view);
        let disconnected = Event {
            origin: Origin::Session(rid(77)),
            owner: owner(1),
            kind: EventKind::Disconnected {
                reason: BackendMsg::raw("lost"),
            },
        };
        engine.accept(&disconnected).unwrap();
        let first = engine.session(sid(1)).unwrap().reconnect;
        assert!(matches!(first, ReconnectState::Waiting { attempt: 1, .. }));
        assert!(engine.accept(&disconnected).is_err());
        assert_eq!(engine.session(sid(1)).unwrap().reconnect, first);
        engine.sessions.get_mut(&sid(1)).unwrap().state = SessionState::Connecting;
        engine.requests.insert(
            (owner(1), rid(78)),
            PendingRequest {
                operation: Operation::Connect,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                origin: Origin::Reply(rid(78)),
                owner: owner(1),
                kind: EventKind::Error {
                    operation: Operation::Connect,
                    message: BackendMsg::raw("retry failed"),
                },
            })
            .unwrap();
        assert!(matches!(
            engine.session(sid(1)).unwrap().reconnect,
            ReconnectState::Waiting { attempt: 2, .. }
        ));
    }
    #[test]
    fn delayed_unsolicited_from_previous_connection_is_rejected() {
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
        let connect_a = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(connect_a),
                kind: EventKind::Connected {
                    endpoint: endpoint.clone(),
                    info: BackendMsg::raw("A"),
                },
            })
            .unwrap();
        let delayed = [
            EventKind::SubscriptionState {
                state: SubscriptionState::Active,
            },
            EventKind::Values {
                values: vec![(
                    key(1),
                    opclook_contract::model::DataValue::bad(BackendMsg::raw("old")),
                )],
            },
            EventKind::PollOnly { keys: vec![key(1)] },
            EventKind::Disconnected {
                reason: BackendMsg::ConnectionClosed,
            },
        ]
        .map(|kind| Event {
            owner: owner(1),
            origin: Origin::Session(connect_a),
            kind,
        });
        let disconnect = engine
            .send(sid(1), Protocol::OpcUa, CommandKind::Disconnect, ())
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(disconnect),
                kind: EventKind::Disconnected {
                    reason: BackendMsg::ConnectionClosed,
                },
            })
            .unwrap();
        let connect_b = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        engine
            .accept(&Event {
                owner: owner(1),
                origin: Origin::Reply(connect_b),
                kind: EventKind::Connected {
                    endpoint,
                    info: BackendMsg::raw("B"),
                },
            })
            .unwrap();
        engine
            .sessions
            .get_mut(&sid(1))
            .unwrap()
            .items
            .insert(key(1), false);
        for event in delayed {
            assert!(engine.accept(&event).is_err());
        }
        assert_eq!(
            engine.session(sid(1)).unwrap().items.get(&key(1)),
            Some(&false)
        );
        assert!(
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Unsolicited,
                    kind: EventKind::Values { values: vec![] }
                })
                .is_err()
        );
        assert_eq!(
            engine.session(sid(1)).unwrap().state,
            SessionState::Connected
        );
        assert_eq!(
            engine.session(sid(1)).unwrap().subscription,
            SubscriptionState::Inactive
        );
        assert!(
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Session(connect_b),
                    kind: EventKind::Values { values: vec![] }
                })
                .is_ok()
        );
    }
    #[test]
    fn completing_one_of_two_reads_keeps_poll_blocked() {
        for first_is_error in [false, true] {
            let now = Instant::now();
            let mut engine = Engine::<()>::default();
            engine.test_skip_dispatch = true;
            let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), now);
            view.state = SessionState::Connected;
            view.connection_id = Some(rid(77));
            view.force_poll = true;
            view.items.insert(ItemKey::FIRST, false);
            engine.sessions.insert(sid(1), view);
            let first = engine
                .send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::Read {
                        keys: vec![ItemKey::FIRST],
                    },
                    (),
                )
                .unwrap();
            assert!(matches!(
                engine.send(
                    sid(1),
                    Protocol::OpcUa,
                    CommandKind::Read {
                        keys: vec![ItemKey::FIRST]
                    },
                    ()
                ),
                Err(DispatchError::ReadBusy)
            ));
            // A legacy overlapping request may still be present during transition.
            let second = rid(99);
            engine.requests.insert(
                (owner(1), second),
                PendingRequest {
                    operation: Operation::Read,
                    sent: now,
                    read_demands: Vec::new(),
                    read_terminals: Vec::new(),
                    read_coverage: Vec::new(),
                    read_epoch: None,
                    write_readback_key: None,
                    ctx: (),
                },
            );
            let kind = if first_is_error {
                EventKind::Error {
                    operation: Operation::Read,
                    message: BackendMsg::raw("read failed"),
                }
            } else {
                EventKind::Values { values: vec![] }
            };
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(first),
                    kind,
                })
                .unwrap();
            assert!(engine.has_pending(sid(1), second));
            assert!(
                engine
                    .due(now + Duration::from_secs(10), Duration::from_secs(1), None)
                    .is_empty()
            );
            engine
                .accept(&Event {
                    owner: owner(1),
                    origin: Origin::Reply(second),
                    kind: EventKind::Values { values: vec![] },
                })
                .unwrap();
            assert!(matches!(
                engine
                    .due(now + Duration::from_secs(10), Duration::from_secs(1), None)
                    .as_slice(),
                [Due::Read { .. }]
            ));
        }
    }
    #[test]
    fn unsolicited_push_does_not_release_pending_read() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.force_poll = true;
        view.connection_id = Some(rid(77));
        view.items.insert(key(7), true);
        view.read_in_flight = true;
        view.last_poll = Instant::now() - Duration::from_secs(10);
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(22)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                origin: Origin::Session(rid(77)),
                owner: owner(1),
                kind: EventKind::Values { values: vec![] },
            })
            .unwrap();
        assert!(
            engine
                .due(Instant::now(), Duration::from_secs(1), None)
                .is_empty()
        );
        engine
            .accept(&Event {
                origin: Origin::Reply(rid(22)),
                owner: owner(1),
                kind: EventKind::Values { values: vec![] },
            })
            .unwrap();
        assert!(matches!(
            engine
                .due(Instant::now(), Duration::from_secs(1), None)
                .as_slice(),
            [Due::Read { .. }]
        ));
    }
    #[test]
    fn unsolicited_read_error_does_not_release_pending_read() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        view.items.insert(key(7), true);
        view.read_in_flight = true;
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(22)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                origin: Origin::Session(rid(77)),
                owner: owner(1),
                kind: EventKind::Error {
                    operation: Operation::Read,
                    message: BackendMsg::raw("late"),
                },
            })
            .unwrap();
        assert!(engine.session(sid(1)).unwrap().read_in_flight);
        assert!(engine.has_pending(owner(1), rid(22)));
    }
    #[test]
    fn cap_cannot_be_bypassed_by_reusing_da_id() {
        let mut engine = Engine::<()>::default();
        for raw in 1..=constants::MAX_UA_SESSIONS as u64 {
            let id = sid(raw);
            engine.sessions.insert(
                id,
                SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now()),
            );
        }
        engine.sessions.insert(
            sid(101),
            SessionRuntime::new(EndpointSpec::da("", "Srv"), Instant::now()),
        );
        let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
        assert!(matches!(
            engine.send(
                sid(101),
                Protocol::OpcUa,
                CommandKind::Connect {
                    endpoint: endpoint.clone()
                },
                ()
            ),
            Err(DispatchError::SessionLimit)
        ));
        engine.shutdown();
        assert!(matches!(
            engine.send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Connect { endpoint },
                ()
            ),
            Err(DispatchError::Closed {
                cause: SendError::GatewayClosed,
                ..
            })
        ));
    }
    #[test]
    fn closed_dispatch_invalidates_old_requests_and_releases_read() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.read_in_flight = true;
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(8)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                read_demands: Vec::new(),
                read_terminals: Vec::new(),
                read_coverage: Vec::new(),
                read_epoch: None,
                write_readback_key: None,
                ctx: (),
            },
        );
        engine.shutdown();
        assert!(
            engine
                .send(sid(1), Protocol::OpcUa, CommandKind::Disconnect, ())
                .is_err()
        );
        assert!(!engine.has_pending(owner(1), rid(8)));
        assert!(!engine.session(sid(1)).unwrap().read_in_flight);
        assert!(
            engine
                .accept(&Event {
                    origin: Origin::Reply(rid(8)),
                    owner: owner(1),
                    kind: EventKind::Values { values: vec![] }
                })
                .is_err()
        );
    }
    #[test]
    fn browse_reply_from_old_connection_does_not_replace_new_branch() {
        use opclook_contract::model::NodeClassFilter;
        let now = Instant::now();
        let endpoint = EndpointSpec::da("localhost", "Fixture.Server.1");
        let mut engine = Engine::<()>::default();
        engine.test_skip_dispatch = true;
        let mut view = SessionRuntime::new(endpoint.clone(), now);
        view.state = SessionState::Connected;
        view.connection_id = Some(rid(77));
        engine.sessions.insert(sid(1), view);
        let make_browse = || CommandKind::Browse {
            node_id: "Fixture".into(),
            filter: NodeClassFilter::All,
            view_id: None,
        };
        let old = engine
            .send(sid(1), Protocol::OpcDa, make_browse(), ())
            .unwrap();
        let disconnect = engine
            .send(sid(1), Protocol::OpcDa, CommandKind::Disconnect, ())
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(disconnect),
                    kind: EventKind::Disconnected {
                        reason: BackendMsg::ConnectionClosed,
                    },
                },
                now,
            )
            .unwrap();
        let connect = engine
            .send(
                sid(1),
                Protocol::OpcDa,
                CommandKind::Connect {
                    endpoint: endpoint.clone(),
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: owner(1),
                    origin: Origin::Reply(connect),
                    kind: EventKind::Connected {
                        endpoint,
                        info: BackendMsg::raw("connected"),
                    },
                },
                now,
            )
            .unwrap();
        let current = engine
            .send(sid(1), Protocol::OpcDa, make_browse(), ())
            .unwrap();
        let response = |id| Event {
            owner: owner(1),
            origin: Origin::Reply(id),
            kind: EventKind::Browse {
                node_id: "Fixture".into(),
                children: vec![],
            },
        };
        assert!(engine.accept_at(&response(old), now).is_err());
        assert!(engine.accept_at(&response(current), now).is_ok());
        assert_eq!(engine.connection_id(sid(1)), Some(connect));
    }
}

#[cfg(test)]
#[path = "acquisition_matrix_tests.rs"]
mod acquisition_matrix_tests;

#[cfg(test)]
#[path = "drop_lifecycle_tests.rs"]
mod drop_lifecycle_tests;
