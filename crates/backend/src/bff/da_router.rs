//! Command router of the OPC DA backend (D2): platform-neutral, no WinAPI.
//!
//! `da.rs` (COM) is Windows-only and cannot be compiled or tested on the Linux
//! check stand, so routing lives here: one worker thread per DA session plus
//! short-lived discovery threads, so a hung DCOM call blocks only its own
//! session. The COM side plugs in through the [`DaHost`] trait; tests use a
//! mock implementation.
#![cfg_attr(not(windows), allow(dead_code))]

use super::model::{
    BackendMsg, Command, CommandKind, DataValue, Event, EventKind, Operation, Owner, SessionId,
};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// Upper bound on simultaneously running discovery threads. A discovery against
/// an unreachable host can hang for minutes (DCOM timeout); the cap keeps the
/// process from piling up threads if the user keeps pressing "refresh".
pub const MAX_CONCURRENT_DISCOVERIES: usize = 8;

/// How long the router waits for session threads to finish their teardown
/// (`RemoveGroup`) once the command channel is closed. `Gateway::shutdown`
/// allows 10 s for the DA worker; the rest of that budget is left for the join
/// of the router thread itself and for the UA worker.
pub const SESSION_JOIN_BUDGET: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Command routing
// ---------------------------------------------------------------------------

/// What the router does with a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// Runs on a discovery thread, independent of every session.
    Discover,
    /// Never forwarded: the router answers with this operation and text.
    Unsupported(Operation, BackendMsg),
    /// Silently ignored (UA-only commands that DA has no equivalent for).
    Ignored,
    /// Creates a session thread.
    Connect,
    /// Detaches the session from the registry and tells its thread to finish.
    Disconnect,
    /// Forwarded to the session's thread (or answered "no connection").
    Session,
}

/// Classify a command. Exhaustive on purpose: a new `CommandKind` variant must
/// be assigned a route here.
pub fn route_of(kind: &CommandKind) -> Route {
    match kind {
        CommandKind::Discover { .. } => Route::Discover,
        CommandKind::SearchUaServers { .. } => {
            Route::Unsupported(Operation::SearchServers, BackendMsg::DaNoUaServerSearch)
        }
        CommandKind::SearchUaNetwork { .. } => {
            Route::Unsupported(Operation::SearchNetwork, BackendMsg::DaNoUaNetworkSearch)
        }
        CommandKind::BrowseViews => {
            Route::Unsupported(Operation::BrowseViews, BackendMsg::DaNoViews)
        }
        CommandKind::ResolveBrowsePath { .. } => {
            Route::Unsupported(Operation::ResolveBrowsePath, BackendMsg::DaNoBrowsePath)
        }
        // UA-only push subscription configuration; DA keeps periodic polling.
        CommandKind::ConfigureUaSubscription { .. } => Route::Ignored,
        // UA-only streaming subnet scan; DA has no equivalent command.
        CommandKind::ScanUaNetwork { .. } | CommandKind::CancelUaScan { .. } => Route::Ignored,
        CommandKind::Connect { .. } => Route::Connect,
        CommandKind::Disconnect => Route::Disconnect,
        CommandKind::Browse { .. }
        | CommandKind::AddItems { .. }
        | CommandKind::RemoveItems { .. }
        | CommandKind::Read { .. }
        | CommandKind::Write { .. }
        | CommandKind::Test => Route::Session,
    }
}

fn error_event(command: &Command, operation: Operation, message: BackendMsg) -> Event {
    Event {
        origin: command.request_id.into(),
        owner: command.owner,
        kind: EventKind::Error { operation, message },
    }
}

/// The reply to a command that has no connected session to run on, in the
/// shape the app expects for that command (per-item errors for `AddItems`,
/// bad values for `Read`, an `Error` event otherwise). `None` for commands that
/// are answered with silence (`Disconnect` of an unknown session, UA-only
/// commands).
pub fn no_connection_reply(command: &Command) -> Option<Event> {
    let kind = match &command.kind {
        CommandKind::AddItems { items } => EventKind::AddItems {
            results: items
                .iter()
                .map(|item| (item.key, Err(BackendMsg::NoConnection)))
                .collect(),
        },
        CommandKind::Read { keys } => EventKind::Values {
            values: keys
                .iter()
                .map(|key| (*key, DataValue::bad(BackendMsg::NoConnection)))
                .collect(),
        },
        CommandKind::RemoveItems { .. } => {
            return Some(error_event(
                command,
                Operation::Remove,
                BackendMsg::NoConnection,
            ));
        }
        CommandKind::Disconnect => return None,
        other => match route_of(other) {
            Route::Ignored => return None,
            Route::Unsupported(operation, text) => {
                return Some(error_event(command, operation, text));
            }
            _ => {
                return Some(error_event(
                    command,
                    other.operation(),
                    BackendMsg::NoConnection,
                ));
            }
        },
    };
    Some(Event {
        origin: command.request_id.into(),
        owner: command.owner,
        kind,
    })
}

/// The COM side of the router.
pub trait DaHost: Send + Sync + 'static {
    /// Body of a session thread. Called on a fresh thread; the first command
    /// received is always `Connect`. Returns when the session is over (failed
    /// connect, disconnect, or `commands` closed by the gateway shutdown). The
    /// implementation must have torn the server group down before returning.
    fn run_session(
        &self,
        session: SessionId,
        commands: &mut UnboundedReceiver<Command>,
        events: &Sender<Event>,
    );

    /// Body of a discovery thread: answers `command` (a `Discover`) with
    /// exactly one event.
    fn run_discovery(&self, command: Command, events: &Sender<Event>);
}

struct SessionHandle {
    tx: UnboundedSender<Command>,
    join: JoinHandle<()>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RouterShutdown {
    pub detached: usize,
    pub panicked: bool,
}

/// Router state: the registry of live sessions and the helper threads.
pub struct Router<H: DaHost> {
    host: Arc<H>,
    events: Sender<Event>,
    sessions: HashMap<SessionId, SessionHandle>,
    /// Threads of sessions that already received `Disconnect` (still tearing
    /// down) or ended on their own; joined at shutdown.
    retired: Vec<JoinHandle<()>>,
    discoveries: Vec<JoinHandle<()>>,
    panicked: bool,
}

impl<H: DaHost> Router<H> {
    pub fn new(host: Arc<H>, events: Sender<Event>) -> Self {
        Self {
            host,
            events,
            sessions: HashMap::new(),
            retired: Vec::new(),
            discoveries: Vec::new(),
            panicked: false,
        }
    }

    /// Number of sessions currently in the registry (tests).
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    /// Drop registry entries whose thread is over, and finished helper threads.
    fn reap(&mut self) {
        let finished: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, handle)| handle.join.is_finished() || handle.tx.is_closed())
            .map(|(id, _)| *id)
            .collect();
        for id in finished {
            if let Some(handle) = self.sessions.remove(&id) {
                self.retired.push(handle.join);
            }
        }
        for message in [
            reap_finished(&mut self.retired).then_some(BackendMsg::DaSessionThreadCrashed),
            reap_finished(&mut self.discoveries).then_some(BackendMsg::DaDiscoveryCrashed),
        ]
        .into_iter()
        .flatten()
        {
            self.send(Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner: Owner::Discovery,
                kind: EventKind::Log { message },
            });
            self.panicked = true;
        }
    }

    /// Route one command. Never blocks on a session or on COM.
    pub fn dispatch(&mut self, command: Command) {
        self.reap();
        let route = route_of(&command.kind);
        let is_discovery = matches!(
            &command.kind,
            CommandKind::Discover { .. }
                | CommandKind::SearchUaServers { .. }
                | CommandKind::SearchUaNetwork { .. }
                | CommandKind::ScanUaNetwork { .. }
                | CommandKind::CancelUaScan { .. }
        );
        let valid = is_discovery == matches!(command.owner, Owner::Discovery);
        if !valid {
            self.send(error_event(
                &command,
                command.kind.operation(),
                BackendMsg::NoConnection,
            ));
            return;
        }
        match route {
            Route::Ignored => {}
            Route::Unsupported(operation, text) => {
                self.send(error_event(&command, operation, text));
            }
            Route::Discover => self.start_discovery(command),
            Route::Connect => self.start_session(command),
            Route::Disconnect => {
                // Detach first: a following Connect for the same id gets a
                // fresh thread even while this one is still tearing down.
                if let Some(handle) = self.sessions.remove(&match command.owner {
                    Owner::Session(id) => id,
                    Owner::Discovery => return,
                }) {
                    // A dead thread gives no reply, like an unknown session.
                    let _ = handle.tx.send(command);
                    self.retired.push(handle.join);
                }
            }
            Route::Session => match self.sessions.get(&match command.owner {
                Owner::Session(id) => id,
                Owner::Discovery => return,
            }) {
                Some(handle) => {
                    if let Err(returned) = handle.tx.send(command) {
                        self.answer_no_connection(&returned.0);
                    }
                }
                None => self.answer_no_connection(&command),
            },
        }
    }

    fn answer_no_connection(&self, command: &Command) {
        if let Some(event) = no_connection_reply(command) {
            self.send(event);
        }
    }

    fn start_session(&mut self, command: Command) {
        let Owner::Session(id) = command.owner else {
            return;
        };
        if self.sessions.contains_key(&id) {
            // `reap` just removed dead entries, so this one is alive.
            self.send(error_event(
                &command,
                Operation::Connect,
                BackendMsg::SessionAlreadyConnected,
            ));
            return;
        }
        let (tx, mut rx) = unbounded_channel::<Command>();
        let host = Arc::clone(&self.host);
        let events = self.events.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("opc-da-session-{id}"))
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    host.run_session(id, &mut rx, &events);
                }));
                if outcome.is_err() {
                    let _ = events.send(Event {
                        origin: crate::bff::model::Origin::Unsolicited,
                        owner: id.into(),
                        kind: EventKind::Log {
                            message: BackendMsg::DaSessionThreadCrashed,
                        },
                    });
                }
                // Close first so that later sends fail (and the router answers
                // them), then answer what is already queued: exactly one reply
                // per command even when the session ended early.
                rx.close();
                while let Ok(queued) = rx.try_recv() {
                    if let Some(event) = no_connection_reply(&queued) {
                        let _ = events.send(event);
                    }
                }
            });
        let join = match spawned {
            Ok(join) => join,
            Err(e) => {
                self.send(error_event(
                    &command,
                    Operation::Connect,
                    BackendMsg::DaSessionThreadSpawnFailed {
                        detail: e.to_string(),
                    },
                ));
                return;
            }
        };
        if let Err(returned) = tx.send(command) {
            // The thread cannot have closed the channel yet; kept for safety.
            self.answer_no_connection(&returned.0);
            self.retired.push(join);
            return;
        }
        self.sessions.insert(id, SessionHandle { tx, join });
    }

    fn start_discovery(&mut self, command: Command) {
        if self.discoveries.len() >= MAX_CONCURRENT_DISCOVERIES {
            self.send(error_event(
                &command,
                Operation::Discover,
                BackendMsg::DaTooManyDiscoveries,
            ));
            return;
        }
        let host = Arc::clone(&self.host);
        let events = self.events.clone();
        let request_id = command.request_id;
        let owner = command.owner;
        let spawned = std::thread::Builder::new()
            .name("opc-da-discover".to_string())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    host.run_discovery(command, &events);
                }));
                if outcome.is_err() {
                    let _ = events.send(Event {
                        origin: request_id.into(),
                        owner,
                        kind: EventKind::Error {
                            operation: Operation::Discover,
                            message: BackendMsg::DaDiscoveryCrashed,
                        },
                    });
                }
            });
        match spawned {
            Ok(join) => self.discoveries.push(join),
            Err(e) => self.send(Event {
                origin: request_id.into(),
                owner,
                kind: EventKind::Error {
                    operation: Operation::Discover,
                    message: BackendMsg::DaDiscoverySpawnFailed {
                        detail: e.to_string(),
                    },
                },
            }),
        }
    }

    /// Gateway shutdown: close every session channel (each session thread then
    /// removes its group) and wait for the threads until `budget` runs out.
    /// Reports detached threads and whether any child panicked.
    pub fn shutdown(mut self, budget: Duration) -> RouterShutdown {
        let mut handles: Vec<JoinHandle<()>> = Vec::new();
        for (_, handle) in self.sessions.drain() {
            drop(handle.tx);
            handles.push(handle.join);
        }
        handles.append(&mut self.retired);
        handles.append(&mut self.discoveries);
        let detached = join_until(handles, Instant::now() + budget, &mut self.panicked);
        RouterShutdown {
            detached,
            panicked: self.panicked,
        }
    }
}

fn reap_finished(handles: &mut Vec<JoinHandle<()>>) -> bool {
    let mut keep = Vec::with_capacity(handles.len());
    let mut panicked = false;
    for handle in handles.drain(..) {
        if handle.is_finished() {
            panicked |= handle.join().is_err();
        } else {
            keep.push(handle);
        }
    }
    *handles = keep;
    panicked
}

fn join_until(mut handles: Vec<JoinHandle<()>>, deadline: Instant, panicked: &mut bool) -> usize {
    loop {
        *panicked |= reap_finished(&mut handles);
        if handles.is_empty() {
            return 0;
        }
        if Instant::now() >= deadline {
            return handles.len();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The router thread body: routes commands until the gateway closes the
/// channel, then tears the sessions down.
pub fn run_router<H: DaHost>(
    host: Arc<H>,
    mut commands: UnboundedReceiver<Command>,
    events: Sender<Event>,
    join_budget: Duration,
) -> super::ShutdownOutcome {
    let mut router = Router::new(host, events);
    while let Some(command) = commands.blocking_recv() {
        router.dispatch(command);
    }
    let report = router.shutdown(join_budget);
    if report.panicked {
        super::ShutdownOutcome::Panicked
    } else if report.detached > 0 {
        super::ShutdownOutcome::TimedOut
    } else {
        super::ShutdownOutcome::Joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{EndpointSpec, ItemKey, ItemSpec, Protocol};
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, channel};

    const WAIT: Duration = Duration::from_secs(5);

    fn sid(n: u64) -> SessionId {
        SessionId::new(n).unwrap()
    }

    fn command(request_id: u64, owner: Owner, kind: CommandKind) -> Command {
        Command {
            request_id: crate::bff::model::RequestId::new(request_id),
            owner,
            protocol: Protocol::OpcDa,
            kind,
        }
    }

    fn connect(request_id: u64, session: SessionId, progid: &str) -> Command {
        command(
            request_id,
            session.into(),
            CommandKind::Connect {
                endpoint: EndpointSpec::da("", progid),
            },
        )
    }

    /// A mock COM host. A session whose ProgID is "fail" fails to connect, one
    /// named "block" waits for `release` inside `Connect` (a hung server).
    struct MockHost {
        release: Mutex<Option<Receiver<()>>>,
        /// Sessions whose teardown ran (RemoveGroup stand-in).
        torn_down: Mutex<Vec<SessionId>>,
        /// Discovery with this host name waits for `release_discovery`.
        release_discovery: Mutex<Option<Receiver<()>>>,
    }

    impl MockHost {
        fn new() -> (Arc<Self>, Sender<()>, Sender<()>) {
            let (release_tx, release_rx) = channel();
            let (disc_tx, disc_rx) = channel();
            (
                Arc::new(Self {
                    release: Mutex::new(Some(release_rx)),
                    torn_down: Mutex::new(Vec::new()),
                    release_discovery: Mutex::new(Some(disc_rx)),
                }),
                release_tx,
                disc_tx,
            )
        }

        fn torn_down(&self) -> Vec<SessionId> {
            let mut v = self
                .torn_down
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            v.sort_unstable();
            v
        }
    }

    impl DaHost for MockHost {
        fn run_session(
            &self,
            session: SessionId,
            commands: &mut UnboundedReceiver<Command>,
            events: &Sender<Event>,
        ) {
            let mut connected = false;
            while let Some(cmd) = commands.blocking_recv() {
                let (request_id, sid) = (cmd.request_id, cmd.owner);
                let reply = |kind| {
                    let _ = events.send(Event {
                        origin: request_id.into(),
                        owner: sid,
                        kind,
                    });
                };
                match cmd.kind {
                    CommandKind::Connect { endpoint } => {
                        if endpoint.address == "block" {
                            let rx = self
                                .release
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .take();
                            if let Some(rx) = rx {
                                let _ = rx.recv();
                            }
                        }
                        if endpoint.address == "panic" {
                            panic!("mock session panic");
                        }
                        if endpoint.address == "fail" {
                            reply(EventKind::Error {
                                operation: Operation::Connect,
                                message: BackendMsg::raw("boom"),
                            });
                            return;
                        }
                        connected = true;
                        reply(EventKind::Connected {
                            endpoint,
                            info: BackendMsg::raw(""),
                        });
                    }
                    CommandKind::Read { keys } => reply(EventKind::Values {
                        values: keys
                            .into_iter()
                            .map(|k| (k, DataValue::bad(BackendMsg::raw("mock"))))
                            .collect(),
                    }),
                    CommandKind::Disconnect => {
                        if connected {
                            self.torn_down
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .push(session);
                        }
                        reply(EventKind::Disconnected {
                            reason: BackendMsg::raw("closed"),
                        });
                        return;
                    }
                    other => {
                        reply(EventKind::Error {
                            operation: other.operation(),
                            message: BackendMsg::raw("mock"),
                        });
                    }
                }
            }
            // Channel closed by the gateway shutdown.
            if connected {
                self.torn_down
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(session);
            }
        }

        fn run_discovery(&self, command: Command, events: &Sender<Event>) {
            if let CommandKind::Discover { host } = &command.kind {
                if host == "hang" {
                    let rx = self
                        .release_discovery
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();
                    if let Some(rx) = rx {
                        let _ = rx.recv();
                    }
                }
                if host == "panic" {
                    panic!("mock discovery panic");
                }
            }
            let _ = events.send(Event {
                origin: command.request_id.into(),
                owner: command.owner,
                kind: EventKind::Servers {
                    host: String::new(),
                    servers: Vec::new(),
                },
            });
        }
    }

    fn router_with(host: Arc<MockHost>) -> (Router<MockHost>, Receiver<Event>) {
        let (tx, rx) = channel();
        (Router::new(host, tx), rx)
    }

    fn next(rx: &Receiver<Event>) -> Event {
        rx.recv_timeout(WAIT).expect("event expected")
    }

    fn event_id(event: &Event) -> u64 {
        match event.origin {
            crate::bff::model::Origin::Reply(id) => id.get(),
            crate::bff::model::Origin::Unsolicited => 0,
        }
    }
    fn is_kind(event: &Event, request_id: u64) -> bool {
        event_id(event) == request_id
    }

    #[test]
    fn rejects_mismatched_owners_without_starting_a_session_or_discovery() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(command(
            1,
            Owner::Discovery,
            CommandKind::Connect {
                endpoint: EndpointSpec::da("", "ok"),
            },
        ));
        let event = next(&rx);
        assert_eq!(event.owner, Owner::Discovery);
        assert!(matches!(
            event.kind,
            EventKind::Error {
                message: BackendMsg::NoConnection,
                ..
            }
        ));
        router.dispatch(command(
            2,
            sid(1).into(),
            CommandKind::Discover { host: "x".into() },
        ));
        let event = next(&rx);
        assert_eq!(event.owner, Owner::Session(sid(1)));
        assert!(matches!(
            event.kind,
            EventKind::Error {
                message: BackendMsg::NoConnection,
                ..
            }
        ));
        assert_eq!(router.session_count(), 0);
    }

    #[test]
    fn hung_session_does_not_block_other_sessions() {
        // D2: before the fix one thread served every session, so a Connect that
        // never returns froze all of them.
        let (host, release, _disc) = MockHost::new();
        let (mut router, rx) = router_with(host);

        router.dispatch(connect(1, sid(1), "block"));
        router.dispatch(connect(2, sid(2), "ok"));
        router.dispatch(command(
            3,
            sid(2).into(),
            CommandKind::Read {
                keys: vec![ItemKey::new(7).unwrap()],
            },
        ));

        let a = next(&rx);
        assert!(is_kind(&a, 2), "session 2 must answer first: {a:?}");
        assert!(matches!(a.kind, EventKind::Connected { .. }));
        let b = next(&rx);
        assert!(is_kind(&b, 3) && matches!(b.kind, EventKind::Values { .. }));
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "session 1 is still blocked"
        );

        release.send(()).unwrap();
        let c = next(&rx);
        assert!(is_kind(&c, 1) && c.owner == sid(1).into());
        assert!(matches!(c.kind, EventKind::Connected { .. }));
    }

    #[test]
    fn commands_for_a_session_keep_their_order() {
        let (host, release, _disc) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(connect(1, sid(1), "block"));
        router.dispatch(command(
            2,
            sid(1).into(),
            CommandKind::Read {
                keys: vec![ItemKey::new(1).unwrap()],
            },
        ));
        router.dispatch(command(
            3,
            sid(1).into(),
            CommandKind::Read {
                keys: vec![ItemKey::new(2).unwrap()],
            },
        ));
        release.send(()).unwrap();
        let ids: Vec<u64> = (0..3).map(|_| event_id(&next(&rx))).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn unknown_session_gets_exactly_one_no_connection_reply_per_command() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(command(
            1,
            sid(9).into(),
            CommandKind::AddItems {
                items: vec![ItemSpec {
                    key: ItemKey::new(5).unwrap(),
                    item_id: "a".into(),
                }],
            },
        ));
        router.dispatch(command(
            2,
            sid(9).into(),
            CommandKind::Read {
                keys: vec![ItemKey::new(5).unwrap(), ItemKey::new(6).unwrap()],
            },
        ));
        router.dispatch(command(
            3,
            sid(9).into(),
            CommandKind::RemoveItems {
                keys: vec![ItemKey::new(5).unwrap()],
            },
        ));
        router.dispatch(command(4, sid(9).into(), CommandKind::Test));
        // Disconnect of an unknown session stays silent, like before.
        router.dispatch(command(5, sid(9).into(), CommandKind::Disconnect));

        let e1 = next(&rx);
        match e1.kind {
            EventKind::AddItems { results } => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].1.as_ref().err(), Some(&BackendMsg::NoConnection));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(next(&rx).kind, EventKind::Values { values } if values.len() == 2));
        match next(&rx).kind {
            EventKind::Error { operation, message } => {
                assert_eq!(operation, Operation::Remove);
                assert_eq!(message, BackendMsg::NoConnection);
            }
            other => panic!("{other:?}"),
        }
        match next(&rx).kind {
            EventKind::Error { operation, .. } => assert_eq!(operation, Operation::Test),
            other => panic!("{other:?}"),
        }
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn failed_connect_leaves_no_session_and_allows_a_retry() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(connect(1, sid(1), "fail"));
        let e = next(&rx);
        assert!(matches!(
            e.kind,
            EventKind::Error {
                operation: Operation::Connect,
                ..
            }
        ));

        // Wait until the thread is gone, then the registry must be empty.
        let deadline = Instant::now() + WAIT;
        loop {
            router.reap();
            if router.session_count() == 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "failed session stayed registered"
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        router.dispatch(command(2, sid(1).into(), CommandKind::Test));
        match next(&rx).kind {
            EventKind::Error { message, .. } => assert_eq!(message, BackendMsg::NoConnection),
            other => panic!("{other:?}"),
        }
        router.dispatch(connect(3, sid(1), "ok"));
        assert!(matches!(next(&rx).kind, EventKind::Connected { .. }));
    }

    #[test]
    fn commands_queued_behind_a_failed_connect_are_each_answered_once() {
        let (host, release, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        // "block" holds the thread inside Connect while the follow-ups queue
        // up; the session then fails because the mock fails after release.
        router.dispatch(connect(1, sid(1), "block"));
        release.send(()).unwrap();
        assert!(matches!(next(&rx).kind, EventKind::Connected { .. }));
        // Now a "fail" connect for another id with queued commands behind it.
        router.dispatch(connect(2, sid(2), "fail"));
        for id in 3..6 {
            router.dispatch(command(id, sid(2).into(), CommandKind::Test));
        }
        let mut seen = std::collections::HashMap::<u64, usize>::new();
        while let Ok(event) = rx.recv_timeout(Duration::from_millis(300)) {
            *seen.entry(event_id(&event)).or_default() += 1;
        }
        for id in 2..6 {
            assert_eq!(
                seen.get(&id),
                Some(&1),
                "request {id} answered once: {seen:?}"
            );
        }
    }

    #[test]
    fn second_connect_for_a_live_session_is_rejected() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(connect(1, sid(1), "ok"));
        assert!(matches!(next(&rx).kind, EventKind::Connected { .. }));
        router.dispatch(connect(2, sid(1), "ok"));
        let e = next(&rx);
        assert!(is_kind(&e, 2));
        assert!(matches!(
            e.kind,
            EventKind::Error {
                operation: Operation::Connect,
                ..
            }
        ));
    }

    #[test]
    fn disconnect_removes_the_group_and_allows_reconnect_of_the_same_id() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(Arc::clone(&host));
        router.dispatch(connect(1, sid(1), "ok"));
        assert!(matches!(next(&rx).kind, EventKind::Connected { .. }));
        router.dispatch(command(2, sid(1).into(), CommandKind::Disconnect));
        assert!(matches!(next(&rx).kind, EventKind::Disconnected { .. }));
        assert_eq!(router.session_count(), 0);
        router.dispatch(connect(3, sid(1), "ok"));
        assert!(matches!(next(&rx).kind, EventKind::Connected { .. }));
        assert_eq!(host.torn_down(), vec![sid(1)]);
    }

    #[test]
    fn shutdown_tears_down_every_session_and_joins_the_threads() {
        // The Gateway shutdown test relies on this: closing the channel ends
        // every session thread after its teardown.
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(Arc::clone(&host));
        for id in 1..=3 {
            router.dispatch(connect(id, sid(id), "ok"));
            assert!(matches!(next(&rx).kind, EventKind::Connected { .. }));
        }
        let detached = router.shutdown(WAIT);
        assert_eq!(
            detached,
            RouterShutdown {
                detached: 0,
                panicked: false
            }
        );
        assert_eq!(host.torn_down(), vec![sid(1), sid(2), sid(3)]);
    }

    #[test]
    fn shutdown_does_not_wait_forever_for_a_hung_session() {
        let (host, release, _d) = MockHost::new();
        let (mut router, _rx) = router_with(host);
        router.dispatch(connect(1, sid(1), "block"));
        let started = Instant::now();
        let detached = router.shutdown(Duration::from_millis(150));
        assert_eq!(
            detached,
            RouterShutdown {
                detached: 1,
                panicked: false
            }
        );
        assert!(started.elapsed() < Duration::from_secs(3));
        release.send(()).unwrap();
    }

    #[test]
    fn reaped_discovery_panic_is_reported_and_retained_for_shutdown() {
        let (host, _release, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        let thread = std::thread::spawn(|| panic!("discovery cleanup failed"));
        let deadline = Instant::now() + WAIT;
        while !thread.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        router.discoveries.push(thread);
        router.reap();
        assert!(matches!(
            next(&rx).kind,
            EventKind::Log {
                message: BackendMsg::DaDiscoveryCrashed
            }
        ));
        assert_eq!(
            router.shutdown(WAIT),
            RouterShutdown {
                detached: 0,
                panicked: true
            }
        );
    }

    #[test]
    fn router_propagates_child_panic_after_join() {
        let (host, _release, _d) = MockHost::new();
        let (mut router, _rx) = router_with(host);
        router
            .retired
            .push(std::thread::spawn(|| panic!("cleanup failed")));
        assert_eq!(
            router.shutdown(WAIT),
            RouterShutdown {
                detached: 0,
                panicked: true
            }
        );
    }

    #[test]
    fn run_router_ends_when_the_channel_closes() {
        let (host, _r, _d) = MockHost::new();
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let (ev_tx, ev_rx) = channel();
        let join = std::thread::spawn(move || run_router(host, cmd_rx, ev_tx, WAIT));
        cmd_tx.send(connect(1, sid(1), "ok")).unwrap();
        assert!(matches!(next(&ev_rx).kind, EventKind::Connected { .. }));
        drop(cmd_tx);
        let deadline = Instant::now() + WAIT;
        while !join.is_finished() {
            assert!(Instant::now() < deadline, "router did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        join.join().unwrap();
    }

    #[test]
    fn panicking_session_is_reported_and_commands_do_not_vanish() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(connect(1, sid(1), "panic"));
        router.dispatch(command(2, sid(1).into(), CommandKind::Test));
        let mut seen = std::collections::HashSet::new();
        while let Ok(event) = rx.recv_timeout(Duration::from_millis(500)) {
            seen.insert(event_id(&event));
            if let EventKind::Log { message } = &event.kind {
                assert!(!crate::i18n::backend_text(crate::i18n::Lang::En, &message).is_empty());
            }
        }
        // request 0 = the crash log line, 2 = the queued Test answered.
        assert!(seen.contains(&0), "crash must be journaled: {seen:?}");
        assert!(
            seen.contains(&2),
            "queued command must be answered: {seen:?}"
        );
    }

    #[test]
    fn hung_discovery_does_not_block_sessions_or_other_discoveries() {
        let (host, _r, disc_release) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(command(
            1,
            Owner::Discovery,
            CommandKind::Discover {
                host: "hang".into(),
            },
        ));
        router.dispatch(connect(2, sid(1), "ok"));
        router.dispatch(command(
            3,
            Owner::Discovery,
            CommandKind::Discover {
                host: "other".into(),
            },
        ));
        let mut got = [event_id(&next(&rx)), event_id(&next(&rx))];
        got.sort_unstable();
        assert_eq!(got, [2, 3]);
        disc_release.send(()).unwrap();
        assert_eq!(event_id(&next(&rx)), 1);
    }

    #[test]
    fn discovery_threads_are_capped_and_the_overflow_is_answered() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        // Fill the cap with threads that stay alive while `gate` is held.
        let gate = Arc::new(Mutex::new(()));
        let guard = gate.lock().unwrap();
        for _ in 0..MAX_CONCURRENT_DISCOVERIES {
            let gate = Arc::clone(&gate);
            router.discoveries.push(std::thread::spawn(move || {
                let _g = gate.lock();
            }));
        }
        router.dispatch(command(
            50,
            Owner::Discovery,
            CommandKind::Discover { host: "x".into() },
        ));
        let e = next(&rx);
        assert!(is_kind(&e, 50));
        assert!(matches!(
            e.kind,
            EventKind::Error {
                operation: Operation::Discover,
                ..
            }
        ));
        drop(guard);
    }

    #[test]
    fn panicking_discovery_is_answered_with_an_error() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(command(
            1,
            Owner::Discovery,
            CommandKind::Discover {
                host: "panic".into(),
            },
        ));
        let e = next(&rx);
        assert!(is_kind(&e, 1));
        assert!(matches!(
            e.kind,
            EventKind::Error {
                operation: Operation::Discover,
                ..
            }
        ));
    }

    #[test]
    fn ua_only_commands_get_the_documented_reply() {
        let (host, _r, _d) = MockHost::new();
        let (mut router, rx) = router_with(host);
        router.dispatch(command(1, sid(1).into(), CommandKind::BrowseViews));
        router.dispatch(command(
            2,
            Owner::Discovery,
            CommandKind::SearchUaNetwork {
                bind: None,
                lds: None,
            },
        ));
        router.dispatch(command(
            3,
            Owner::Discovery,
            CommandKind::CancelUaScan {
                scan_id: opclook_contract::model::ScanId::FIRST,
            },
        ));
        assert!(matches!(
            next(&rx).kind,
            EventKind::Error {
                operation: Operation::BrowseViews,
                ..
            }
        ));
        assert!(matches!(
            next(&rx).kind,
            EventKind::Error {
                operation: Operation::SearchNetwork,
                ..
            }
        ));
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}
