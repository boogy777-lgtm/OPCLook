//! GUI-independent session state, command correlation and scheduling.
pub mod constants;
use opclook_backend::bff::Gateway;
use opclook_backend::net::AdapterInfo;
use opclook_backend::route_service::{RouteService, RouteTicket};
use opclook_contract::da_route::DaRouteError;
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, Operation, Protocol, SessionId,
    SubscriptionState,
};
use std::collections::HashMap;
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
        base_secs: u64,
        max_secs: u64,
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
        let delay = reconnect_delay(attempt, base_secs, max_secs);
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
pub fn reconnect_delay(attempt: u32, base_secs: u64, max_secs: u64) -> Duration {
    Duration::from_secs(
        base_secs
            .checked_shl(attempt.saturating_sub(1))
            .unwrap_or(u64::MAX)
            .min(max_secs),
    )
}
pub fn request_timeout(operation: Operation, configured_secs: u64) -> Duration {
    let floor = match operation {
        Operation::Connect
        | Operation::Discover
        | Operation::SearchServers
        | Operation::SearchNetwork => constants::REQUEST_SLOW_MIN_SECS,
        _ => constants::REQUEST_MIN_SECS,
    };
    Duration::from_secs(configured_secs.max(floor))
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
    Closed { request_id: u64, message: String },
}
pub struct RouteOptions {
    pub host: String,
    pub adapter: Option<AdapterInfo>,
    pub local_addresses: Vec<IpAddr>,
    pub deadline: Instant,
}
struct DeferredRoute {
    session: SessionId,
    request_id: u64,
    kind: CommandKind,
    ticket: RouteTicket,
}
pub enum RouteIssue {
    Preparation {
        session: SessionId,
        request_id: u64,
        operation: Operation,
        error: DaRouteError,
    },
    Lost {
        session: SessionId,
        error: DaRouteError,
    },
}
pub struct Engine<C> {
    gateway: Gateway,
    next: Option<u64>,
    requests: HashMap<(u64, u64), PendingRequest<C>>,
    sessions: HashMap<SessionId, SessionRuntime>,
    auto_reconnect: bool,
    route_service: Option<RouteService>,
    deferred_routes: Vec<DeferredRoute>,
    session_routes: HashMap<SessionId, RouteTicket>,
    discovery_routes: HashMap<u64, RouteTicket>,
}
impl<C> Default for Engine<C> {
    fn default() -> Self {
        Self {
            gateway: Gateway::new(),
            next: Some(1),
            requests: HashMap::new(),
            sessions: HashMap::new(),
            auto_reconnect: false,
            route_service: None,
            deferred_routes: Vec::new(),
            session_routes: HashMap::new(),
            discovery_routes: HashMap::new(),
        }
    }
}
impl<C> Engine<C> {
    /// Enqueue DA discovery or connection without doing DNS or helper work on
    /// the caller thread. The engine owns the ticket through the operation.
    pub fn prepare_da(
        &mut self,
        session: SessionId,
        kind: CommandKind,
        ctx: C,
        options: RouteOptions,
    ) -> Result<u64, DaRouteError> {
        if !matches!(
            kind,
            CommandKind::Connect { .. } | CommandKind::Discover { .. }
        ) {
            return Err(DaRouteError::HelperUnavailable(
                "invalid route operation".into(),
            ));
        }
        if self.route_service.is_none() {
            self.route_service = Some(
                RouteService::new().map_err(|e| DaRouteError::HelperUnavailable(e.to_string()))?,
            );
        }
        let id = self.next.ok_or_else(|| {
            DaRouteError::HelperUnavailable("request identifiers exhausted".into())
        })?;
        let service = self
            .route_service
            .as_ref()
            .ok_or_else(|| DaRouteError::HelperUnavailable("route service unavailable".into()))?;
        let ticket = service.prepare(
            options.host,
            options.adapter,
            options.local_addresses,
            options.deadline,
        )?;
        self.next = id.checked_add(1);
        let operation = kind.operation();
        if let CommandKind::Connect { endpoint } = &kind {
            self.cancel_da_routes(session);
            self.requests.retain(|(sid, _), _| *sid != session);
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint.clone();
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
        }
        self.requests.insert(
            (session, id),
            PendingRequest {
                operation,
                sent: Instant::now(),
                ctx,
            },
        );
        self.deferred_routes.push(DeferredRoute {
            session,
            request_id: id,
            kind,
            ticket,
        });
        Ok(id)
    }
    /// Advance route preparation and return failures for the presentation layer
    /// to localize. All successful commands are dispatched by the engine.
    pub fn pump_da_routes(&mut self) -> Vec<RouteIssue> {
        let mut issues = Vec::new();
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
                        request_id: deferred.request_id,
                        session: deferred.session,
                        protocol: Protocol::OpcDa,
                        kind: deferred.kind,
                    });
                    match result {
                        Ok(()) if operation == Operation::Connect => {
                            self.session_routes
                                .insert(deferred.session, deferred.ticket);
                        }
                        Ok(()) => {
                            self.discovery_routes
                                .insert(deferred.request_id, deferred.ticket);
                        }
                        Err(message) => issues.push(RouteIssue::Preparation {
                            session: deferred.session,
                            request_id: deferred.request_id,
                            operation,
                            error: DaRouteError::HelperUnavailable(message),
                        }),
                    }
                }
                Err(error) => issues.push(RouteIssue::Preparation {
                    session: deferred.session,
                    request_id: deferred.request_id,
                    operation,
                    error,
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
            self.session_routes.remove(&session);
            let _ = self.gateway.send(Command {
                request_id: 0,
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::Disconnect,
            });
            issues.push(RouteIssue::Lost { session, error });
        }
        issues
    }
    pub fn cancel_da_routes(&mut self, session: SessionId) {
        self.deferred_routes.retain(|item| item.session != session);
        if session == 0 {
            self.discovery_routes.clear();
        } else {
            self.session_routes.remove(&session);
        }
    }
    pub fn has_da_route(&self, session: SessionId) -> bool {
        self.session_routes.contains_key(&session)
            || self
                .deferred_routes
                .iter()
                .any(|item| item.session == session)
    }
    pub fn send(
        &mut self,
        session: SessionId,
        protocol: Protocol,
        kind: CommandKind,
        ctx: C,
    ) -> Result<u64, DispatchError> {
        if matches!(&kind, CommandKind::Connect { endpoint } if endpoint.protocol == Protocol::OpcUa)
            && !self
                .sessions
                .get(&session)
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
        if matches!(&kind, CommandKind::Connect { .. } | CommandKind::Disconnect) {
            self.cancel_da_routes(session);
            self.requests.retain(|(id, _), _| *id != session);
        }
        let id = self.next.ok_or(DispatchError::Exhausted)?;
        self.next = id.checked_add(1);
        let operation = kind.operation();
        if !matches!(
            &kind,
            CommandKind::ScanUaNetwork { .. } | CommandKind::CancelUaScan { .. }
        ) {
            self.requests.insert(
                (session, id),
                PendingRequest {
                    operation,
                    sent: Instant::now(),
                    ctx,
                },
            );
        }
        if let Err(message) = self.gateway.send(Command {
            request_id: id,
            session,
            protocol,
            kind,
        }) {
            self.requests.remove(&(session, id));
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
                message,
            });
        }
        if let Some(endpoint) = endpoint {
            let view = self
                .sessions
                .entry(session)
                .or_insert_with(|| SessionRuntime::new(endpoint.clone(), Instant::now()));
            view.endpoint = endpoint;
            view.state = SessionState::Connecting;
            view.read_in_flight = false;
        }
        if let Some(view) = self.sessions.get_mut(&session) {
            for key in added {
                view.items.insert(key, false);
            }
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
        let pending = if event.request_id == 0 {
            None
        } else {
            Some(
                self.requests
                    .remove(&(event.session, event.request_id))
                    .ok_or(())?,
            )
        };
        if event.request_id != 0 {
            self.deferred_routes.retain(|item| {
                !(item.session == event.session && item.request_id == event.request_id)
            });
            self.discovery_routes.remove(&event.request_id);
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
                .get(&event.session)
                .is_some_and(|view| view.state != SessionState::Connecting);
        if failed_connect || disconnected {
            self.session_routes.remove(&event.session);
        }
        let correlated_read = pending
            .as_ref()
            .is_some_and(|p| p.operation == Operation::Read);
        let read_pending = self
            .requests
            .iter()
            .any(|(&(sid, _), p)| sid == event.session && p.operation == Operation::Read);
        let mut schedule = false;
        if let Some(view) = self.sessions.get_mut(&event.session) {
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
            self.schedule_reconnect(event.session, Instant::now());
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
                    let busy = self
                        .requests
                        .iter()
                        .any(|(&(sid, _), p)| sid == id && p.operation == Operation::Browse);
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
                    let test_pending = self
                        .requests
                        .iter()
                        .any(|(&(sid, _), p)| sid == id && p.operation == Operation::Test);
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
    pub fn has_pending(&self, session: SessionId, request: u64) -> bool {
        self.requests.contains_key(&(session, request))
    }
    pub fn discard_pending(&mut self, session: SessionId, request: u64) {
        self.deferred_routes
            .retain(|item| !(item.session == session && item.request_id == request));
        self.discovery_routes.remove(&request);
        self.requests.remove(&(session, request));
    }
    pub fn invalidate_session_requests(&mut self, session: SessionId) -> Vec<Operation> {
        self.cancel_da_routes(session);
        let keys: Vec<_> = self
            .requests
            .keys()
            .filter(|(sid, _)| *sid == session)
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|key| self.requests.remove(&key).map(|p| p.operation))
            .collect()
    }
    pub fn expired_requests(
        &self,
        now: Instant,
        configured: u64,
    ) -> Vec<(SessionId, u64, Operation, C, u64)>
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
            sid == session
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
                    constants::RECONNECT_BASE_DELAY_SECS,
                    constants::RECONNECT_MAX_DELAY_SECS,
                )
            })
            .unwrap_or(ReconnectScheduled::Unchanged)
    }
    pub fn try_recv(&self) -> Option<Event> {
        self.gateway.try_recv()
    }
    pub fn shutdown(&mut self) {
        self.deferred_routes.clear();
        self.session_routes.clear();
        self.discovery_routes.clear();
        self.route_service = None;
        self.gateway.shutdown();
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
    pub fn insert_pending_for_test(&mut self, sid: SessionId, id: u64, pending: PendingRequest<C>) {
        self.requests.insert((sid, id), pending);
    }
    #[cfg(feature = "test-support")]
    pub fn pending_ids_for_test(&self, sid: SessionId) -> Vec<u64> {
        let mut ids: Vec<_> = self
            .requests
            .keys()
            .filter_map(|(s, id)| (*s == sid).then_some(*id))
            .collect();
        ids.sort_unstable();
        ids
    }
    #[cfg(feature = "test-support")]
    pub fn age_requests_for_test(&mut self, sid: SessionId, age: Duration) {
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
    use opclook_contract::model::BackendMsg;
    #[test]
    fn cancelled_route_preparation_never_dispatches_late_connect() {
        let mut engine = Engine::<()>::default();
        let id = engine
            .prepare_da(
                1,
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("127.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    host: "127.0.0.1".into(),
                    adapter: None,
                    local_addresses: vec![],
                    deadline: Instant::now() + Duration::from_secs(2),
                },
            )
            .unwrap();
        engine
            .accept(&Event {
                request_id: id,
                session: 1,
                kind: EventKind::Error {
                    operation: Operation::Connect,
                    message: BackendMsg::raw("timed out"),
                },
            })
            .unwrap();
        assert!(!engine.has_da_route(1));
        assert!(engine.pump_da_routes().is_empty());
        assert!(!engine.has_pending(1, id));
    }
    #[test]
    fn route_preparation_dispatches_and_engine_owns_the_ticket() {
        let mut engine = Engine::<()>::default();
        let id = engine
            .prepare_da(
                1,
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("127.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    host: "127.0.0.1".into(),
                    adapter: None,
                    local_addresses: vec![],
                    deadline: Instant::now() + Duration::from_secs(2),
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !engine.session_routes.contains_key(&1) && Instant::now() < deadline {
            assert!(engine.pump_da_routes().is_empty());
            std::thread::yield_now();
        }
        assert!(engine.session_routes.contains_key(&1));
        assert!(engine.has_pending(1, id));
        engine.cancel_da_routes(1);
        assert!(!engine.has_da_route(1));
    }
    #[test]
    fn route_failure_keeps_request_for_correlated_error() {
        let mut engine = Engine::<()>::default();
        let id = engine
            .prepare_da(
                1,
                CommandKind::Connect {
                    endpoint: EndpointSpec::da("10.0.0.1", "Srv"),
                },
                (),
                RouteOptions {
                    host: "10.0.0.1".into(),
                    adapter: None,
                    local_addresses: vec![],
                    deadline: Instant::now() + Duration::from_secs(2),
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let issue = loop {
            if let Some(issue) = engine.pump_da_routes().into_iter().next() {
                break Some(issue);
            }
            if Instant::now() >= deadline {
                break None;
            }
            std::thread::yield_now();
        };
        assert!(
            matches!(issue, Some(RouteIssue::Preparation { request_id, error: DaRouteError::NoAdapter, .. }) if request_id == id)
        );
        assert!(engine.has_pending(1, id));
    }
    #[test]
    fn duplicate_disconnect_does_not_advance_retry_but_failed_attempt_does() {
        let mut engine = Engine::<()>::default();
        engine.set_auto_reconnect(true);
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        engine.sessions.insert(1, view);
        let disconnected = Event {
            request_id: 0,
            session: 1,
            kind: EventKind::Disconnected {
                reason: BackendMsg::raw("lost"),
            },
        };
        engine.accept(&disconnected).unwrap();
        let first = engine.session(1).unwrap().reconnect;
        assert!(matches!(first, ReconnectState::Waiting { attempt: 1, .. }));
        engine.accept(&disconnected).unwrap();
        assert_eq!(engine.session(1).unwrap().reconnect, first);
        engine.sessions.get_mut(&1).unwrap().state = SessionState::Connecting;
        engine
            .accept(&Event {
                request_id: 0,
                session: 1,
                kind: EventKind::Error {
                    operation: Operation::Connect,
                    message: BackendMsg::raw("retry failed"),
                },
            })
            .unwrap();
        assert!(matches!(
            engine.session(1).unwrap().reconnect,
            ReconnectState::Waiting { attempt: 2, .. }
        ));
    }
    #[test]
    fn unsolicited_push_does_not_release_pending_read() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.items.insert(7, true);
        view.read_in_flight = true;
        view.last_poll = Instant::now() - Duration::from_secs(10);
        engine.sessions.insert(1, view);
        engine.requests.insert(
            (1, 22),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                request_id: 0,
                session: 1,
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
                request_id: 22,
                session: 1,
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
        view.items.insert(7, true);
        view.read_in_flight = true;
        engine.sessions.insert(1, view);
        engine.requests.insert(
            (1, 22),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                ctx: (),
            },
        );
        engine
            .accept(&Event {
                request_id: 0,
                session: 1,
                kind: EventKind::Error {
                    operation: Operation::Read,
                    message: BackendMsg::raw("late"),
                },
            })
            .unwrap();
        assert!(engine.session(1).unwrap().read_in_flight);
        assert!(engine.has_pending(1, 22));
    }
    #[test]
    fn cap_cannot_be_bypassed_by_reusing_da_id() {
        let mut engine = Engine::<()>::default();
        for id in 1..=constants::MAX_UA_SESSIONS as u64 {
            engine.sessions.insert(
                id,
                SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now()),
            );
        }
        engine.sessions.insert(
            101,
            SessionRuntime::new(EndpointSpec::da("", "Srv"), Instant::now()),
        );
        let endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
        assert!(matches!(
            engine.send(
                101,
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
            engine.send(1, Protocol::OpcUa, CommandKind::Connect { endpoint }, ()),
            Err(DispatchError::Closed { .. })
        ));
    }
    #[test]
    fn closed_dispatch_invalidates_old_requests_and_releases_read() {
        let mut engine = Engine::<()>::default();
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://127.0.0.1:4840"), Instant::now());
        view.read_in_flight = true;
        engine.sessions.insert(1, view);
        engine.requests.insert(
            (1, 8),
            PendingRequest {
                operation: Operation::Read,
                sent: Instant::now(),
                ctx: (),
            },
        );
        engine.shutdown();
        assert!(
            engine
                .send(1, Protocol::OpcUa, CommandKind::Disconnect, ())
                .is_err()
        );
        assert!(!engine.has_pending(1, 8));
        assert!(!engine.session(1).unwrap().read_in_flight);
        assert!(
            engine
                .accept(&Event {
                    request_id: 8,
                    session: 1,
                    kind: EventKind::Values { values: vec![] }
                })
                .is_err()
        );
    }
}
