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
    write_readback_key: Option<ItemKey>,
}
impl<C> PendingRequest<C> {
    pub fn new(operation: Operation, sent: Instant, ctx: C) -> Self {
        Self {
            operation,
            sent,
            ctx,
            read_demands: Vec::new(),
            write_readback_key: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReadDemandKind {
    ExplicitRefresh,
    WriteReadback,
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

#[derive(Clone, Debug)]
pub struct SessionRuntime {
    pub endpoint: EndpointSpec,
    pub state: SessionState,
    pub reconnect: ReconnectState,
    pub subscription: SubscriptionState,
    pub read_in_flight: bool,
    pub last_poll: Instant,
    pub last_test: Instant,
    pub items: HashMap<ItemKey, bool>,
    pending_initial: HashMap<ItemKey, Instant>,
    pending_demands: HashSet<ReadDemand>,
    reserved_demands: HashSet<ReadDemand>,
    poll_defer_since: HashMap<ItemKey, Instant>,
    used_keys: HashSet<ItemKey>,
    reserved_read: Option<(ReadPermit, Vec<ItemKey>)>,
    force_poll: bool,
    connection_id: Option<RequestId>,
}
impl SessionRuntime {
    pub fn new(endpoint: EndpointSpec, now: Instant) -> Self {
        Self {
            endpoint,
            state: SessionState::Connecting,
            reconnect: ReconnectState::Idle,
            subscription: SubscriptionState::Inactive,
            read_in_flight: false,
            last_poll: now.checked_sub(Duration::from_secs(10)).unwrap_or(now),
            last_test: now,
            items: HashMap::new(),
            pending_initial: HashMap::new(),
            pending_demands: HashSet::new(),
            reserved_demands: HashSet::new(),
            poll_defer_since: HashMap::new(),
            used_keys: HashSet::new(),
            reserved_read: None,
            force_poll: false,
            connection_id: None,
        }
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
#[derive(Clone, Copy, Default)]
struct PendingAdd {
    poll_only: bool,
    initial_seen: bool,
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
    #[cfg(test)]
    test_skip_dispatch: bool,
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
            #[cfg(test)]
            test_skip_dispatch: false,
            #[cfg(feature = "test-support")]
            test_dispatch_limit: std::cell::Cell::new(None),
            #[cfg(feature = "test-support")]
            test_dispatched: std::cell::RefCell::new(Vec::new()),
        }
    }
}
impl<C> Engine<C> {
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
            self.cancel_da_routes(session);
            self.requests.retain(|(sid, _), _| *sid != owner);
            self.pending_adds.retain(|(sid, _), _| *sid != owner);
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint.clone();
            view.connection_id = Some(id);
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.items.clear();
            view.pending_initial.clear();
            view.pending_demands.clear();
            view.reserved_demands.clear();
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
        self.cancel_da_routes(session);
        if let Some(view) = self.sessions.get_mut(&session) {
            view.connection_id = None;
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
                && (runtime.state != SessionState::Connected || runtime.connection_id.is_none()))
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
        if view.state != SessionState::Connected || view.connection_id.is_none() {
            return Err(DemandWriteError::Demand(DemandError::SessionUnavailable));
        }
        if !supports_fresh_source(view.endpoint.protocol) || view.endpoint.protocol != protocol {
            return Err(DemandWriteError::Demand(DemandError::UnsupportedFreshness));
        }
        if !view.items.contains_key(&request.key) {
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
        if view.state != SessionState::Connected || view.connection_id.is_none() {
            return Err(DemandError::SessionUnavailable);
        }
        if !supports_fresh_source(view.endpoint.protocol) {
            return Err(DemandError::UnsupportedFreshness);
        }
        if !view.items.contains_key(&key) {
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
        if let CommandKind::Connect { endpoint } = &kind {
            validate_acquisition(endpoint).map_err(DispatchError::InvalidAcquisition)?;
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
        let Some(id) = self.next else {
            if matches!(&kind, CommandKind::Read { .. }) {
                let pending_read = self.requests.iter().any(|(&(sid, _), pending)| {
                    sid == owner && pending.operation == Operation::Read
                });
                if let Some(view) = session.and_then(|session| self.sessions.get_mut(&session)) {
                    if permit.is_some() {
                        view.reserved_demands.clear();
                        view.reserved_read = None;
                    }
                    view.read_in_flight = pending_read || view.reserved_read.is_some();
                }
            }
            return Err(DispatchError::Exhausted);
        };
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
                    view.read_in_flight = false;
                    view.reserved_read = None;
                    return Err(DispatchError::ReadBusy);
                }
                covered_demands.extend(view.reserved_demands.drain());
                view.reserved_read = None;
            } else if view.read_in_flight {
                return Err(DispatchError::ReadBusy);
            } else {
                for key in keys {
                    view.pending_initial.remove(key);
                }
            }
        } else if permit.is_some() {
            return Err(DispatchError::ReadBusy);
        }
        if matches!(&kind, CommandKind::Connect { .. } | CommandKind::Disconnect) {
            self.cancel_da_routes(owner);
            self.timeout_queued.retain(|(id, _)| *id != owner);
            self.requests.retain(|(id, _), _| *id != owner);
            self.pending_adds.retain(|(id, _), _| *id != owner);
        }
        self.next = id.successor();
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
                    write_readback_key: profile_readback_key,
                    ctx,
                },
            );
        }
        if let Err(cause) = self.dispatch(Command {
            request_id: Some(id),
            owner,
            protocol,
            kind,
        }) {
            self.requests.remove(&(owner, id));
            for view in self
                .sessions
                .values_mut()
                .filter(|view| view.endpoint.protocol == protocol)
            {
                view.state = SessionState::Disconnected;
                view.reconnect = ReconnectState::Stopped;
                view.read_in_flight = false;
                view.reserved_read = None;
                view.subscription = SubscriptionState::Inactive;
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
                view.poll_defer_since.clear();
                view.reserved_read = None;
                view.read_in_flight = false;
            }
        }
        if let (Some(endpoint), Some(session)) = (endpoint, session) {
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint;
            view.connection_id = Some(id);
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.items.clear();
            view.pending_initial.clear();
            view.pending_demands.clear();
            view.reserved_demands.clear();
            view.poll_defer_since.clear();
            view.reserved_read = None;
            view.used_keys.clear();
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
        let pending = match event.origin {
            Origin::Unsolicited | Origin::Session(_) => None,
            Origin::Reply(id) => {
                let pending = self.requests.remove(&(event.owner, id)).ok_or(())?;
                self.timeout_queued.remove(&(event.owner, id));
                self.deferred_routes
                    .retain(|item| !(item.owner == event.owner && item.request_id == id));
                self.discovery_routes.remove(&id);
                Some(pending)
            }
        };
        let Owner::Session(session) = event.owner else {
            return Ok(pending);
        };
        // Session state changes require the identity of the Connect that created
        // the producer. Untagged session diagnostics remain visible.
        let affects_session = !matches!(event.kind, EventKind::Log { .. });
        if matches!(event.origin, Origin::Session(_))
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
        if failed_connect || disconnected {
            self.session_routes.remove(&session);
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
            Origin::Unsolicited | Origin::Session(_) => None,
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
        if let (Some(additions), EventKind::AddItems { results }) = (additions, &event.kind)
            && let Some(view) = self.sessions.get_mut(&session)
        {
            for (key, result) in results {
                if result.is_ok()
                    && let Some(addition) = additions.get(key)
                {
                    view.items
                        .entry(*key)
                        .and_modify(|flag| *flag |= addition.poll_only)
                        .or_insert(addition.poll_only);
                    if !addition.initial_seen {
                        view.pending_initial.insert(*key, now);
                    }
                    view.poll_defer_since.insert(*key, now);
                }
            }
        }
        if matches!(event.origin, Origin::Session(_))
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
                    }
                }
                view.pending_initial.remove(key);
            }
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
                    view.pending_initial.clear();
                    view.pending_demands.clear();
                    view.reserved_demands.clear();
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
                    view.pending_initial.clear();
                    view.pending_demands.clear();
                    view.reserved_demands.clear();
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
                    view.pending_initial.clear();
                    view.pending_demands.clear();
                    view.reserved_demands.clear();
                    view.poll_defer_since.clear();
                    view.reserved_read = None;
                }
                EventKind::Error {
                    operation: Operation::Read,
                    ..
                } if correlated_read => view.read_in_flight = read_pending,
                EventKind::Values { .. } if correlated_read => view.read_in_flight = read_pending,
                EventKind::SubscriptionState { state } => {
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
        for (&id, view) in &mut self.sessions {
            match view.state {
                SessionState::Connected => {
                    let profile = view.endpoint.acquisition.as_ref();
                    let interval = profile
                        .map(|c| Duration::from_millis(c.interval_ms))
                        .unwrap_or(interval);
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
                    if !view.read_in_flight && !busy {
                        let mut keys: Vec<_> = view
                            .pending_initial
                            .iter()
                            .filter_map(|(&key, &since)| {
                                let wait_for_push = configured_poll.map(|poll| !poll).unwrap_or(
                                    !view.force_poll
                                        && view.endpoint.protocol == Protocol::OpcUa
                                        && !view.items.get(&key).copied().unwrap_or(false),
                                );
                                (!wait_for_push || now.saturating_duration_since(since) >= interval)
                                    .then_some(key)
                            })
                            .collect();
                        keys.extend(view.pending_demands.iter().map(|d| d.key));
                        if now.saturating_duration_since(view.last_poll) >= interval {
                            keys.extend(view.items.iter().filter_map(|(&key, &poll_only)| {
                                let startup_wait = configured_poll.is_none()
                                    && !view.force_poll
                                    && view.endpoint.protocol == Protocol::OpcUa
                                    && view.poll_defer_since.get(&key).is_some_and(|since| {
                                        now.saturating_duration_since(*since) < interval
                                    });
                                ((!push || (configured_poll.is_none() && poll_only))
                                    && !startup_wait)
                                    .then_some(key)
                            }));
                        }
                        view.poll_defer_since
                            .retain(|_, since| now.saturating_duration_since(*since) < interval);
                        keys.sort_unstable();
                        keys.dedup();
                        if !keys.is_empty() {
                            let Some(raw_permit) = self.next_permit else {
                                for key in &keys {
                                    view.pending_initial.remove(key);
                                }
                                view.pending_demands.clear();
                                view.last_poll = now;
                                work.push(Due::ReadUnavailable { session: id });
                                continue;
                            };
                            self.next_permit = raw_permit.checked_add(1);
                            view.reserved_demands = std::mem::take(&mut view.pending_demands);
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
        work
    }
    pub fn has_pending(&self, owner: impl Into<Owner>, request: RequestId) -> bool {
        self.requests.contains_key(&(owner.into(), request))
    }
    pub fn discard_pending(&mut self, owner: impl Into<Owner>, request: RequestId) {
        let session = owner.into();
        self.timeout_queued.remove(&(session, request));
        self.pending_adds.remove(&(session, request));
        self.deferred_routes
            .retain(|item| !(item.owner == session && item.request_id == request));
        self.discovery_routes.remove(&request);
        self.requests.remove(&(session, request));
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
        self.deferred_routes.retain(|item| item.owner != owner);
        self.timeout_queued.retain(|(sid, _)| *sid != owner);
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
                    && !self.timeout_queued.contains(&(sid, id)))
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
        self.cancel_da_routes(id);
        self.invalidated.remove(&id);
        self.pending_adds
            .retain(|(sid, _), _| *sid != Owner::Session(id));
        self.sessions.remove(&id);
    }
    pub fn manual_disconnect(&mut self, id: SessionId) {
        if let Some(view) = self.sessions.get_mut(&id) {
            view.state = SessionState::Disconnected;
            view.reconnect = ReconnectState::Stopped;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.pending_initial.clear();
            view.pending_demands.clear();
            view.reserved_demands.clear();
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
    pub fn shutdown(&mut self) -> GatewayShutdown {
        self.deferred_routes.clear();
        self.session_routes.clear();
        self.discovery_routes.clear();
        self.route_events.clear();
        self.invalidated.clear();
        self.pending_adds.clear();
        self.timeout_queued.clear();
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
            session.pending_initial = old.pending_initial.clone();
            session.pending_demands = old.pending_demands.clone();
            session.reserved_demands = old.reserved_demands.clone();
            session.poll_defer_since = old.poll_defer_since.clone();
            session.reserved_read = old.reserved_read.clone();
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
        assert!(matches!(
            engine.request_refresh(sid(1), key(1)),
            Err(DemandError::UnsupportedFreshness)
        ));
        assert!(matches!(
            engine.send_write_with_readback(sid(1), Protocol::OpcDa, request.clone(), ()),
            Err(DemandWriteError::Demand(DemandError::UnsupportedFreshness))
        ));
        assert!(engine.requests.is_empty());
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
}
