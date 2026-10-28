//! GUI-independent session state, command correlation and scheduling.
pub mod constants;
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
    force_poll: bool,
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
            force_poll: false,
        }
    }
}
pub enum Due {
    Read {
        session: SessionId,
        protocol: Protocol,
        keys: Vec<ItemKey>,
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
    Exhausted,
    SessionLimit,
    Closed {
        request_id: RequestId,
        cause: SendError,
    },
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
pub struct Engine<C> {
    gateway: Gateway,
    next: Option<RequestId>,
    requests: HashMap<(Owner, RequestId), PendingRequest<C>>,
    pending_adds: HashMap<(Owner, RequestId), HashMap<ItemKey, bool>>,
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
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.items.clear();
        }
        self.requests.insert(
            (owner, id),
            PendingRequest {
                operation,
                sent: Instant::now(),
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
            origin: connect_request.into(),
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
        Ok(())
    }
    pub fn has_request_capacity(&self) -> bool {
        self.next.is_some()
    }
    pub fn send(
        &mut self,
        owner: impl Into<Owner>,
        protocol: Protocol,
        kind: CommandKind,
        ctx: C,
    ) -> Result<RequestId, DispatchError> {
        let owner = owner.into();
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
        let Some(id) = self.next else {
            if matches!(&kind, CommandKind::Read { .. }) {
                let pending_read = self.requests.iter().any(|(&(sid, _), pending)| {
                    sid == owner && pending.operation == Operation::Read
                });
                if let Some(view) = session.and_then(|session| self.sessions.get_mut(&session)) {
                    view.read_in_flight = pending_read;
                }
            }
            return Err(DispatchError::Exhausted);
        };
        if matches!(&kind, CommandKind::Connect { .. } | CommandKind::Disconnect) {
            self.cancel_da_routes(owner);
            self.timeout_queued.retain(|(id, _)| *id != owner);
            self.requests.retain(|(id, _), _| *id != owner);
            self.pending_adds.retain(|(id, _), _| *id != owner);
        }
        self.next = id.successor();
        let operation = kind.operation();
        if !matches!(
            &kind,
            CommandKind::ScanUaNetwork { .. } | CommandKind::CancelUaScan { .. }
        ) {
            self.requests.insert(
                (owner, id),
                PendingRequest {
                    operation,
                    sent: Instant::now(),
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
                view.subscription = SubscriptionState::Inactive;
            }
            return Err(DispatchError::Closed {
                request_id: id,
                cause,
            });
        }
        if let (Some(endpoint), Some(session)) = (endpoint, session) {
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint;
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
            view.subscription = SubscriptionState::Inactive;
            view.items.clear();
        }
        if !added.is_empty() {
            self.pending_adds.insert(
                (owner, id),
                added.into_iter().map(|key| (key, false)).collect(),
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
            }
            if operation == Operation::Read {
                view.read_in_flight = true;
                view.last_poll = Instant::now();
            }
        }
        Ok(id)
    }
    pub fn accept(&mut self, event: &Event) -> Result<Option<PendingRequest<C>>, ()> {
        let pending = match event.origin {
            Origin::Unsolicited => None,
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
        let additions = match event.origin {
            Origin::Unsolicited => None,
            Origin::Reply(id) => self.pending_adds.remove(&(event.owner, id)),
        };
        if let EventKind::PollOnly { keys } = &event.kind {
            for (&(sid, _), additions) in &mut self.pending_adds {
                if sid == event.owner {
                    for key in keys {
                        if let Some(flag) = additions.get_mut(key) {
                            *flag = true;
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
                    && let Some(&poll_only) = additions.get(key)
                {
                    view.items
                        .entry(*key)
                        .and_modify(|flag| *flag |= poll_only)
                        .or_insert(poll_only);
                }
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
                    view.endpoint = endpoint.clone();
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
                }
                EventKind::Error {
                    operation: Operation::Connect,
                    ..
                } => {
                    let was_connecting = view.state == SessionState::Connecting;
                    view.state = SessionState::Disconnected;
                    view.read_in_flight = false;
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
                }
                EventKind::Error {
                    operation: Operation::Read,
                    ..
                } if correlated_read => view.read_in_flight = false,
                EventKind::Values { .. } if correlated_read => view.read_in_flight = false,
                EventKind::SubscriptionState { state } => {
                    view.subscription = *state;
                    if *state == SubscriptionState::Active && !read_pending {
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
                    let busy = self.requests.iter().any(|(&(sid, _), p)| {
                        sid == Owner::Session(id) && p.operation == Operation::Browse
                    });
                    let push = !view.force_poll
                        && view.endpoint.protocol == Protocol::OpcUa
                        && view.subscription == SubscriptionState::Active;
                    if !view.read_in_flight
                        && !busy
                        && now.saturating_duration_since(view.last_poll) >= interval
                    {
                        let keys: Vec<_> = view
                            .items
                            .iter()
                            .filter_map(|(&key, &poll_only)| (!push || poll_only).then_some(key))
                            .collect();
                        if !keys.is_empty() {
                            view.read_in_flight = true;
                            view.last_poll = now;
                            work.push(Due::Read {
                                session: id,
                                protocol: view.endpoint.protocol,
                                keys,
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
    pub fn seed_session_for_test(&mut self, id: SessionId, session: SessionRuntime) {
        self.sessions.insert(id, session);
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
                ctx: (),
            },
        );
        engine.requests.insert(
            (Owner::Session(session), request),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
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
        engine.next = RequestId::new(u64::MAX);
        let id = engine
            .send(
                sid(1),
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![] },
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
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(7)),
            PendingRequest {
                operation: Operation::Write,
                sent: Instant::now(),
                ctx: (),
            },
        );
        engine.on_da_route_lost(sid(1), DaRouteError::HelperUnavailable("lost".into()));
        assert!(!engine.has_pending(owner(1), rid(7)));
        let event = engine.route_events.pop_front().unwrap();
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
        view.read_in_flight = true;
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(19)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now() - Duration::from_secs(100),
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
        view.subscription = SubscriptionState::Active;
        view.items.insert(key(1), false);
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(9)),
            PendingRequest {
                operation: Operation::AddItems,
                sent: Instant::now() - Duration::from_secs(100),
                ctx: (),
            },
        );
        engine.pending_adds.insert(
            (owner(1), rid(9)),
            [(key(2), false), (key(3), false)].into_iter().collect(),
        );
        engine
    }
    #[test]
    fn add_results_install_only_confirmed_keys_and_preserve_early_poll_only() {
        let mut engine = pending_add_engine();
        engine
            .accept(&Event {
                origin: Origin::Unsolicited,
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
                ctx: (),
            },
        );
        engine
            .pending_adds
            .insert((owner(1), rid(50)), [(key(4), false)].into_iter().collect());
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
                origin: Origin::Unsolicited,
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
        engine.sessions.insert(sid(1), view);
        let disconnected = Event {
            origin: Origin::Unsolicited,
            owner: owner(1),
            kind: EventKind::Disconnected {
                reason: BackendMsg::raw("lost"),
            },
        };
        engine.accept(&disconnected).unwrap();
        let first = engine.session(sid(1)).unwrap().reconnect;
        assert!(matches!(first, ReconnectState::Waiting { attempt: 1, .. }));
        engine.accept(&disconnected).unwrap();
        assert_eq!(engine.session(sid(1)).unwrap().reconnect, first);
        engine.sessions.get_mut(&sid(1)).unwrap().state = SessionState::Connecting;
        engine
            .accept(&Event {
                origin: Origin::Unsolicited,
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
    fn unsolicited_push_does_not_release_pending_read() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.items.insert(key(7), true);
        view.read_in_flight = true;
        view.last_poll = Instant::now() - Duration::from_secs(10);
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(22)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                origin: Origin::Unsolicited,
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
        view.items.insert(key(7), true);
        view.read_in_flight = true;
        engine.sessions.insert(sid(1), view);
        engine.requests.insert(
            (owner(1), rid(22)),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                origin: Origin::Unsolicited,
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
