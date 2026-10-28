//! OPC UA worker: a thin host router plus one actor per live session.
//!
//! * The **host** (one task) only routes commands and keeps the registry of live
//!   sessions. It never awaits network I/O, so no session, search, scan or
//!   connect attempt can delay another one.
//! * Searches, scans and `Connect` run as their own tasks, in parallel.
//! * A session enters the registry only after its `Connect` succeeded and the
//!   session itself reported ready; from then on it is an **actor**: its own
//!   task with its own command queue, in which control commands (browse, add,
//!   write, disconnect ...) overtake queued periodic reads.
//!
//! Invariants kept from the single-loop design: every command gets exactly one
//! `request_id`-correlated reply (`Disconnect` of a session that does not exist
//! has nothing to close and stays silent), a `Disconnect` is always fully
//! processed before a later `Connect` of the same session id starts (so the app
//! sees `Disconnected` before `Connected`), a timed-out request poisons the
//! session and closes it gracefully, and closing the command channel closes every
//! session on the wire.

use super::common::{reply_no_connection, send, send_error, send_notes};
use super::discovery::{search_network_servers, search_servers};
use super::io::{browse, browse_views, read, resolve_browse_path, test, write};
use super::scanner::{ScanRegistry, abort_all_scans, cancel_scan, start_scan_task};
use super::session::{UaSession, connect};
use crate::bff::model::{
    BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, Operation, Owner, SessionId,
};
use crate::constants::{UA_HOST_RUNTIME_THREADS, UA_SESSION_BACKLOG_LIMIT};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

pub(super) fn spawn(event_tx: Sender<Event>) -> crate::bff::WorkerHandle {
    let (cmd_tx, cmd_rx): (UnboundedSender<Command>, UnboundedReceiver<Command>) =
        unbounded_channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let spawned = std::thread::Builder::new()
        .name("opc-ua".to_string())
        .spawn(move || {
            // A multi-thread runtime: the CPU-bound parts of a secure-channel
            // handshake (RSA, key derivation) and the per-session actors must not
            // stall each other's I/O on a single thread.
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(UA_HOST_RUNTIME_THREADS)
                .thread_name("opc-ua-rt")
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    let _ = event_tx.send(Event {
                        origin: crate::bff::model::Origin::Unsolicited,
                        owner: Owner::Discovery,
                        kind: EventKind::Error {
                            operation: Operation::UaRuntime,
                            message: BackendMsg::UaRuntimeStartFailed {
                                detail: err.to_string(),
                            },
                        },
                    });
                    let _ = done_tx.send(());
                    return crate::bff::ShutdownOutcome::Joined;
                }
            };
            runtime.block_on(run_host(cmd_rx, event_tx));
            let _ = done_tx.send(());
            crate::bff::ShutdownOutcome::Joined
        });
    match spawned {
        Ok(join) => crate::bff::WorkerHandle::new(cmd_tx, join, done_rx),
        Err(_) => crate::bff::WorkerHandle::closed(done_rx),
    }
}

// ---------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------

/// Messages from session tasks to the host.
enum Internal {
    /// A connect attempt succeeded; the session is up and its actor is running.
    Ready(Box<Ready>),
    /// A connect attempt ended without a session. The HOST answers the `Connect`
    /// with the error, after it has released the slot: the application reacts to
    /// that reply (a retry), and the retry must not find a stale "connecting" slot.
    ConnectEnded {
        session: SessionId,
        generation: u64,
        request_id: Option<crate::bff::model::RequestId>,
        message: BackendMsg,
    },
    /// An actor has finished and released its session.
    Closed { session: SessionId, generation: u64 },
}

struct Ready {
    session: SessionId,
    generation: u64,
    request_id: Option<crate::bff::model::RequestId>,
    endpoint: EndpointSpec,
    info: BackendMsg,
    commands: UnboundedSender<ActorMsg>,
    join: JoinHandle<()>,
}

/// A session whose `Connect` is still running.
struct Connecting {
    generation: u64,
    cancel: CancellationToken,
    /// Commands that arrived before the session was ready. They are delivered in
    /// order once it is, or answered "not connected" if the connect fails.
    backlog: VecDeque<Command>,
    task: JoinHandle<()>,
}

/// A connected session: the handle of its actor.
struct Live {
    generation: u64,
    commands: UnboundedSender<ActorMsg>,
    join: JoinHandle<()>,
}

enum Slot {
    Connecting(Connecting),
    Live(Live),
}

struct Host {
    event_tx: Sender<Event>,
    internal_tx: UnboundedSender<Internal>,
    slots: HashMap<SessionId, Slot>,
    /// The last lifecycle task of a session id that may still be running after
    /// its slot was released (graceful close, cancelled connect). A new `Connect`
    /// of the same id first waits for it, which keeps `Disconnected` ahead of the
    /// next `Connected`.
    tails: HashMap<SessionId, JoinHandle<()>>,
    scans: ScanRegistry,
    /// Fire-and-forget tasks (server searches).
    tasks: JoinSet<()>,
    next_generation: u64,
}

async fn run_host(mut cmd_rx: UnboundedReceiver<Command>, event_tx: Sender<Event>) {
    let (internal_tx, mut internal_rx) = unbounded_channel::<Internal>();
    let mut host = Host {
        event_tx,
        internal_tx,
        slots: HashMap::new(),
        tails: HashMap::new(),
        scans: Arc::new(Mutex::new(HashMap::new())),
        tasks: JoinSet::new(),
        next_generation: 0,
    };
    loop {
        tokio::select! {
            biased;
            Some(message) = internal_rx.recv() => host.on_internal(message),
            Some(_) = host.tasks.join_next(), if !host.tasks.is_empty() => {}
            command = cmd_rx.recv() => match command {
                Some(command) => host.on_command(command),
                None => break,
            },
        }
    }
    host.shutdown(&mut internal_rx).await;
}

impl Host {
    fn on_command(&mut self, command: Command) {
        let request_id = command.request_id;
        let owner = command.owner;
        let is_discovery = matches!(
            &command.kind,
            CommandKind::Discover { .. }
                | CommandKind::SearchUaServers { .. }
                | CommandKind::SearchUaNetwork { .. }
                | CommandKind::ScanUaNetwork { .. }
                | CommandKind::CancelUaScan { .. }
        );
        if is_discovery != matches!(owner, Owner::Discovery) {
            send_error(
                &self.event_tx,
                request_id,
                owner,
                command.kind.operation(),
                BackendMsg::NoConnection,
            );
            return;
        }
        match command.kind {
            CommandKind::Discover { .. } => send_error(
                &self.event_tx,
                request_id,
                owner,
                Operation::Discover,
                BackendMsg::UaNeedsServerUrl,
            ),
            CommandKind::SearchUaServers { host, port, bind } => {
                let tx = self.event_tx.clone();
                self.tasks.spawn(async move {
                    match search_servers(&host, port, bind.as_ref()).await {
                        Ok(searched) => {
                            send_notes(&tx, owner, searched.notes);
                            send(
                                &tx,
                                Event {
                                    origin: request_id.into(),
                                    owner,
                                    kind: EventKind::UaServersFound {
                                        host,
                                        port,
                                        servers: searched.found,
                                    },
                                },
                            );
                        }
                        Err(message) => {
                            send_error(&tx, request_id, owner, Operation::SearchServers, message)
                        }
                    }
                });
            }
            CommandKind::SearchUaNetwork { bind, lds } => {
                let tx = self.event_tx.clone();
                self.tasks.spawn(async move {
                    match search_network_servers(bind.as_ref(), lds.as_ref()).await {
                        Ok(searched) => {
                            send_notes(&tx, owner, searched.notes);
                            send(
                                &tx,
                                Event {
                                    origin: request_id.into(),
                                    owner,
                                    kind: EventKind::UaNetworkFound {
                                        servers: searched.found,
                                    },
                                },
                            );
                        }
                        Err(message) => {
                            send_error(&tx, request_id, owner, Operation::SearchNetwork, message)
                        }
                    }
                });
            }
            CommandKind::ScanUaNetwork {
                scan_id,
                bind,
                subnet,
                local_ip,
            } => {
                start_scan_task(scan_id, bind, subnet, local_ip, &self.scans, &self.event_tx);
            }
            CommandKind::CancelUaScan { scan_id } => {
                cancel_scan(&self.scans, scan_id);
            }
            CommandKind::Connect { endpoint } => {
                if let Owner::Session(session) = owner {
                    self.start_connect(request_id, session, endpoint)
                }
            }
            CommandKind::Disconnect => self.disconnect(Command {
                request_id,
                owner,
                protocol: command.protocol,
                kind: CommandKind::Disconnect,
            }),
            _ => self.route(Command {
                request_id,
                owner,
                protocol: command.protocol,
                kind: command.kind,
            }),
        }
    }

    /// The live slot of `session`, if its actor still accepts commands. A slot
    /// whose actor already closed its inbox (it is finishing a graceful close)
    /// is released here and its task remembered as the session's tail.
    fn live(&mut self, session: SessionId) -> Option<&Live> {
        let closed = matches!(
            self.slots.get(&session),
            Some(Slot::Live(live)) if live.commands.is_closed()
        );
        if closed {
            if let Some(Slot::Live(live)) = self.slots.remove(&session) {
                self.push_tail(session, live.join);
            }
        }
        match self.slots.get(&session) {
            Some(Slot::Live(live)) => Some(live),
            _ => None,
        }
    }

    /// Remember `task` as the last lifecycle task of `session`, chained behind
    /// any earlier one so the order of closes is preserved.
    fn push_tail(&mut self, session: SessionId, task: JoinHandle<()>) {
        self.tails.retain(|_, tail| !tail.is_finished());
        let joined = match self.tails.remove(&session) {
            Some(previous) => tokio::spawn(async move {
                let _ = previous.await;
                let _ = task.await;
            }),
            None => task,
        };
        self.tails.insert(session, joined);
    }

    fn start_connect(
        &mut self,
        request_id: Option<crate::bff::model::RequestId>,
        session: SessionId,
        endpoint: EndpointSpec,
    ) {
        if self.live(session).is_some() {
            send_error(
                &self.event_tx,
                request_id,
                session,
                Operation::Connect,
                BackendMsg::SessionAlreadyConnected,
            );
            return;
        }
        if matches!(self.slots.get(&session), Some(Slot::Connecting(_))) {
            send_error(
                &self.event_tx,
                request_id,
                session,
                Operation::Connect,
                BackendMsg::ConnectInProgress,
            );
            return;
        }
        self.next_generation += 1;
        let generation = self.next_generation;
        let cancel = CancellationToken::new();
        let job = ConnectJob {
            request_id,
            session,
            generation,
            endpoint,
            prior: self.tails.remove(&session),
            cancel: cancel.clone(),
            event_tx: self.event_tx.clone(),
            internal_tx: self.internal_tx.clone(),
        };
        let task = tokio::spawn(job.run());
        self.slots.insert(
            session,
            Slot::Connecting(Connecting {
                generation,
                cancel,
                backlog: VecDeque::new(),
                task,
            }),
        );
    }

    fn disconnect(&mut self, command: Command) {
        let Owner::Session(session) = command.owner else {
            reply_no_connection(&self.event_tx, command);
            return;
        };
        let _ = self.live(session);
        match self.slots.remove(&session) {
            Some(Slot::Live(live)) => {
                // The actor closes the session on the wire and answers with
                // `Disconnected`. Later commands for this id find no slot.
                if live.commands.send(ActorMsg::Command(command)).is_err() {
                    // The actor is already gone: nothing is left to close.
                }
                self.push_tail(session, live.join);
            }
            Some(Slot::Connecting(connecting)) => {
                connecting.cancel.cancel();
                for queued in connecting.backlog {
                    reply_no_connection(&self.event_tx, queued);
                }
                self.push_tail(session, connecting.task);
            }
            None => {}
        }
    }

    fn route(&mut self, command: Command) {
        let Owner::Session(session) = command.owner else {
            reply_no_connection(&self.event_tx, command);
            return;
        };
        let _ = self.live(session);
        match self.slots.get_mut(&session) {
            Some(Slot::Live(live)) => {
                if let Err(rejected) = live.commands.send(ActorMsg::Command(command)) {
                    if let ActorMsg::Command(command) = rejected.0 {
                        reply_no_connection(&self.event_tx, command);
                    }
                }
            }
            Some(Slot::Connecting(connecting)) => {
                if connecting.backlog.len() >= UA_SESSION_BACKLOG_LIMIT {
                    reply_no_connection(&self.event_tx, command);
                } else {
                    connecting.backlog.push_back(command);
                }
            }
            None => reply_no_connection(&self.event_tx, command),
        }
    }

    fn on_internal(&mut self, message: Internal) {
        match message {
            Internal::Ready(ready) => self.on_ready(*ready),
            Internal::ConnectEnded {
                session,
                generation,
                request_id,
                message,
            } => {
                let current = matches!(
                    self.slots.get(&session),
                    Some(Slot::Connecting(c)) if c.generation == generation
                );
                if current {
                    if let Some(Slot::Connecting(connecting)) = self.slots.remove(&session) {
                        for queued in connecting.backlog {
                            reply_no_connection(&self.event_tx, queued);
                        }
                    }
                }
                send_error(
                    &self.event_tx,
                    request_id,
                    session,
                    Operation::Connect,
                    message,
                );
            }
            Internal::Closed {
                session,
                generation,
            } => {
                let current = matches!(
                    self.slots.get(&session),
                    Some(Slot::Live(l)) if l.generation == generation
                );
                if current {
                    self.slots.remove(&session);
                }
            }
        }
    }

    fn on_ready(&mut self, ready: Ready) {
        let Ready {
            session,
            generation,
            request_id,
            endpoint,
            info,
            commands,
            join,
        } = ready;
        let current = matches!(
            self.slots.get(&session),
            Some(Slot::Connecting(c)) if c.generation == generation
        );
        if !current {
            // The attempt was cancelled (Disconnect) after it had already
            // succeeded: close the fresh session quietly and answer the Connect.
            let _ = commands.send(ActorMsg::Shutdown);
            self.push_tail(session, join);
            send_error(
                &self.event_tx,
                request_id,
                session,
                Operation::Connect,
                BackendMsg::ConnectCancelled,
            );
            return;
        }
        let backlog = match self.slots.remove(&session) {
            Some(Slot::Connecting(connecting)) => connecting.backlog,
            _ => VecDeque::new(),
        };
        // Register first, announce second: once the app sees `Connected` and
        // sends its first command, the session is already routable.
        self.slots.insert(
            session,
            Slot::Live(Live {
                generation,
                commands: commands.clone(),
                join,
            }),
        );
        send(
            &self.event_tx,
            Event {
                origin: request_id.into(),
                owner: session.into(),
                kind: EventKind::Connected { endpoint, info },
            },
        );
        for queued in backlog {
            if let Err(rejected) = commands.send(ActorMsg::Command(queued)) {
                if let ActorMsg::Command(queued) = rejected.0 {
                    reply_no_connection(&self.event_tx, queued);
                }
            }
        }
    }

    /// A message from a connect/actor task while the host is shutting down: a
    /// session handed over late is closed at once, a failed connect is answered.
    fn settle_during_shutdown(&self, message: Internal, pending: &mut Vec<JoinHandle<()>>) {
        match message {
            Internal::Ready(ready) => {
                let _ = ready.commands.send(ActorMsg::Shutdown);
                send_error(
                    &self.event_tx,
                    ready.request_id,
                    ready.session,
                    Operation::Connect,
                    BackendMsg::ConnectCancelled,
                );
                pending.push(ready.join);
            }
            Internal::ConnectEnded {
                session,
                request_id,
                message,
                ..
            } => send_error(
                &self.event_tx,
                request_id,
                session,
                Operation::Connect,
                message,
            ),
            Internal::Closed { .. } => {}
        }
    }

    /// The command channel is closed (gateway shutdown): stop everything and
    /// close every session on the wire, in parallel, before the thread exits.
    async fn shutdown(&mut self, internal_rx: &mut UnboundedReceiver<Internal>) {
        abort_all_scans(&self.scans).await;
        self.tasks.shutdown().await;

        let mut pending: Vec<JoinHandle<()>> = self.tails.drain().map(|(_, task)| task).collect();
        let mut connecting: Vec<SessionId> = Vec::new();
        for (session, slot) in self.slots.drain() {
            match slot {
                Slot::Live(live) => {
                    let _ = live.commands.send(ActorMsg::Shutdown);
                    pending.push(live.join);
                }
                Slot::Connecting(c) => {
                    c.cancel.cancel();
                    for queued in c.backlog {
                        reply_no_connection(&self.event_tx, queued);
                    }
                    connecting.push(session);
                    pending.push(c.task);
                }
            }
        }
        // A connect that wins the race against the cancellation still hands over
        // a live session: close it right away instead of leaking it.
        while !pending.iter().all(|task| task.is_finished()) {
            tokio::select! {
                Some(message) = internal_rx.recv() => {
                    self.settle_during_shutdown(message, &mut pending);
                }
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
        }
        // Whatever a late connect handed over after the last check.
        while let Ok(message) = internal_rx.try_recv() {
            self.settle_during_shutdown(message, &mut pending);
        }
        for task in pending {
            let _ = task.await;
        }
    }
}

// ---------------------------------------------------------------------------
// Connect task
// ---------------------------------------------------------------------------

struct ConnectJob {
    request_id: Option<crate::bff::model::RequestId>,
    session: SessionId,
    generation: u64,
    endpoint: EndpointSpec,
    prior: Option<JoinHandle<()>>,
    cancel: CancellationToken,
    event_tx: Sender<Event>,
    internal_tx: UnboundedSender<Internal>,
}

impl ConnectJob {
    async fn run(self) {
        let ConnectJob {
            request_id,
            session,
            generation,
            endpoint,
            prior,
            cancel,
            event_tx,
            internal_tx,
        } = self;
        // The previous incarnation of this session id (still closing) must be
        // gone first so its `Disconnected` precedes our `Connected`.
        if let Some(prior) = prior {
            let _ = prior.await;
        }
        let outcome = if cancel.is_cancelled() {
            Err(BackendMsg::ConnectCancelled)
        } else {
            connect(session, request_id, endpoint.clone(), &cancel, &event_tx).await
        };
        match outcome {
            Ok(mut ua_session) => {
                if cancel.is_cancelled() {
                    ua_session.close().await;
                    let _ = internal_tx.send(Internal::ConnectEnded {
                        session,
                        generation,
                        request_id,
                        message: BackendMsg::ConnectCancelled,
                    });
                    return;
                }
                let info = ua_session.info.clone();
                let (commands, inbox) = unbounded_channel::<ActorMsg>();
                let actor = Actor {
                    id: session,
                    generation,
                    session: ua_session,
                    inbox,
                    queue: CommandQueue::default(),
                    event_tx: event_tx.clone(),
                    internal_tx: internal_tx.clone(),
                };
                let join = tokio::spawn(actor.run());
                let ready = Ready {
                    session,
                    generation,
                    request_id,
                    endpoint,
                    info,
                    commands,
                    join,
                };
                // If the host is already gone the message (and with it the
                // command sender) is dropped: the actor sees its inbox close and
                // shuts the session down on the wire by itself.
                let _ = internal_tx.send(Internal::Ready(Box::new(ready)));
            }
            Err(message) => {
                let _ = internal_tx.send(Internal::ConnectEnded {
                    session,
                    generation,
                    request_id,
                    message,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session actor
// ---------------------------------------------------------------------------

enum ActorMsg {
    Command(Command),
    /// Close the session on the wire and exit (gateway shutdown / stale connect).
    Shutdown,
}

enum Next {
    Command(Command),
    Shutdown,
}

/// What the actor does after a command.
enum Flow {
    Continue,
    /// The session is closed; the actor ends.
    Closed,
}

/// The actor's command queue. Control commands (browse, add, write, disconnect,
/// ...) are always taken before the periodic reads queued behind them; reads are
/// only re-ordered, never dropped.
#[derive(Default)]
pub(super) struct CommandQueue {
    controls: VecDeque<Next>,
    reads: VecDeque<Command>,
}

impl CommandQueue {
    fn push(&mut self, message: ActorMsg) {
        match message {
            ActorMsg::Shutdown => self.controls.push_back(Next::Shutdown),
            ActorMsg::Command(command) => self.push_command(command),
        }
    }

    pub(super) fn push_command(&mut self, command: Command) {
        if is_control_command(&command.kind) {
            self.controls.push_back(Next::Command(command));
        } else {
            self.reads.push_back(command);
        }
    }

    fn pop(&mut self) -> Option<Next> {
        self.controls
            .pop_front()
            .or_else(|| self.reads.pop_front().map(Next::Command))
    }

    pub(super) fn pop_command(&mut self) -> Option<Command> {
        loop {
            match self.pop()? {
                Next::Command(command) => return Some(command),
                Next::Shutdown => {}
            }
        }
    }
}

/// True when the command should jump ahead of queued periodic reads
/// (browse/refresh/reconnect latency). Reads are independent best-effort polls,
/// so they are safe to defer; they are re-ordered, never dropped.
pub(super) fn is_control_command(kind: &CommandKind) -> bool {
    !matches!(kind, CommandKind::Read { .. } | CommandKind::Test)
}

enum InitialLoad<T> {
    Finished(T),
    Disconnect(Command),
    Shutdown,
}

async fn wait_initial_load<F: std::future::Future>(
    future: F,
    inbox: &mut UnboundedReceiver<ActorMsg>,
    queue: &mut CommandQueue,
) -> InitialLoad<F::Output> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return InitialLoad::Finished(result),
            message = inbox.recv() => match message {
                Some(ActorMsg::Shutdown) | None => return InitialLoad::Shutdown,
                Some(ActorMsg::Command(command)) if matches!(command.kind, CommandKind::Disconnect) => return InitialLoad::Disconnect(command),
                Some(message) => queue.push(message),
            }
        }
    }
}

struct Actor {
    id: SessionId,
    generation: u64,
    session: UaSession,
    inbox: UnboundedReceiver<ActorMsg>,
    queue: CommandQueue,
    event_tx: Sender<Event>,
    internal_tx: UnboundedSender<Internal>,
}

impl Actor {
    async fn run(mut self) {
        // Initial type loading is optional, and must never hold Disconnect or
        // shutdown hostage. Other work stays queued until it finishes.
        let initial = wait_initial_load(
            self.session.load_struct_types(),
            &mut self.inbox,
            &mut self.queue,
        )
        .await;
        let mut shutdown = false;
        let mut already_closed = false;
        match initial {
            InitialLoad::Finished(Err(cause)) => {
                self.send_event(
                    None,
                    EventKind::Log {
                        message: BackendMsg::StructTypesUnavailable {
                            cause: Box::new(cause),
                        },
                    },
                );
                if self.session.timed_out {
                    self.inbox.close();
                    self.session.close().await;
                    self.send_event(
                        None,
                        EventKind::Disconnected {
                            reason: BackendMsg::SessionClosedAfterTimeout,
                        },
                    );
                    already_closed = true;
                }
            }
            InitialLoad::Finished(Ok(())) => {}
            InitialLoad::Disconnect(command) => {
                self.handle(command).await;
                already_closed = true;
            }
            InitialLoad::Shutdown => shutdown = true,
        }
        'serve: while !already_closed && !shutdown {
            // Take everything that is already waiting, so a control command
            // overtakes the reads queued before it.
            let mut inbox_closed = false;
            loop {
                match self.inbox.try_recv() {
                    Ok(message) => self.queue.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        inbox_closed = true;
                        break;
                    }
                }
            }
            let next = match self.queue.pop() {
                Some(next) => next,
                None if inbox_closed => {
                    shutdown = true;
                    break 'serve;
                }
                None => match self.inbox.recv().await {
                    Some(message) => {
                        self.queue.push(message);
                        continue 'serve;
                    }
                    None => {
                        shutdown = true;
                        break 'serve;
                    }
                },
            };
            match next {
                Next::Shutdown => {
                    shutdown = true;
                    break 'serve;
                }
                Next::Command(command) => {
                    if let Flow::Closed = self.handle(command).await {
                        break 'serve;
                    }
                }
            }
        }

        // No new command can be queued from here on; the host stops routing to
        // a closed inbox.
        self.inbox.close();
        if shutdown {
            self.session.teardown_subscription(&self.event_tx).await;
            self.session.close().await;
        }
        while let Ok(message) = self.inbox.try_recv() {
            self.queue.push(message);
        }
        while let Some(command) = self.queue.pop_command() {
            reply_no_connection(&self.event_tx, command);
        }
        let _ = self.internal_tx.send(Internal::Closed {
            session: self.id,
            generation: self.generation,
        });
    }

    fn send_error(
        &self,
        request_id: Option<crate::bff::model::RequestId>,
        operation: Operation,
        message: BackendMsg,
    ) {
        send_error(&self.event_tx, request_id, self.id, operation, message);
    }

    fn send_event(&self, request_id: Option<crate::bff::model::RequestId>, kind: EventKind) {
        send(
            &self.event_tx,
            Event {
                origin: request_id.map_or_else(
                    || {
                        self.session.connection_id.map_or(
                            crate::bff::model::Origin::Unsolicited,
                            crate::bff::model::Origin::Session,
                        )
                    },
                    crate::bff::model::Origin::Reply,
                ),
                owner: self.id.into(),
                kind,
            },
        );
    }

    async fn handle(&mut self, command: Command) -> Flow {
        let request_id = command.request_id;
        let tx = self.event_tx.clone();
        match command.kind {
            CommandKind::Disconnect => {
                self.inbox.close();
                self.session.teardown_subscription(&tx).await;
                // Graceful teardown: CloseSession + CloseSecureChannel on the
                // wire, then wait for the event loop to finish on its own. The
                // loop only returns once the channel is fully closed, so aborting
                // it first would leave the server mid-response (the 0x80310000
                // "no transport connection" errors). Abort only as a last resort
                // when the graceful close cannot complete (e.g. network already
                // gone) so a stuck session never hangs the Disconnect command.
                self.session.close().await;
                self.send_event(
                    request_id,
                    EventKind::Disconnected {
                        reason: BackendMsg::ConnectionClosed,
                    },
                );
                Flow::Closed
            }
            CommandKind::Browse {
                node_id,
                filter,
                view_id,
            } => {
                let result = browse(&mut self.session, &node_id, filter, view_id).await;
                self.settle(request_id, Operation::Browse, result, move |children| {
                    EventKind::Browse { node_id, children }
                })
                .await
            }
            CommandKind::BrowseViews => {
                let result = browse_views(&mut self.session).await;
                self.settle(request_id, Operation::BrowseViews, result, |children| {
                    EventKind::Views { children }
                })
                .await
            }
            CommandKind::ResolveBrowsePath { start_node, path } => {
                let result = resolve_browse_path(&mut self.session, start_node, &path).await;
                self.settle(
                    request_id,
                    Operation::ResolveBrowsePath,
                    result,
                    |targets| EventKind::PathResolved { targets },
                )
                .await
            }
            CommandKind::ConfigureUaSubscription {
                publishing_interval_ms,
                deadband,
            } => {
                let problems = self
                    .session
                    .apply_subscription_settings(publishing_interval_ms, deadband)
                    .await;
                for message in problems {
                    self.send_event(None, EventKind::Log { message });
                }
                if self.session.timed_out {
                    return self
                        .close_timed_out(
                            request_id,
                            Operation::Subscription,
                            BackendMsg::RequestTimedOutSessionClosed,
                        )
                        .await;
                }
                Flow::Continue
            }
            CommandKind::AddItems { items } => {
                let (results, warning) = self.session.add_items(&items, &tx).await;
                if let Some(message) = warning {
                    // A command may produce exactly one request-id-correlated
                    // reply; ancillary diagnostics are unsolicited (id 0) so
                    // they cannot consume the AddItems reply's pending entry.
                    self.send_event(None, EventKind::Log { message });
                }
                self.send_event(request_id, EventKind::AddItems { results });
                if self.session.timed_out {
                    return self
                        .close_timed_out(
                            request_id,
                            Operation::Add,
                            BackendMsg::RequestTimedOutSessionClosed,
                        )
                        .await;
                }
                Flow::Continue
            }
            CommandKind::RemoveItems { keys } => {
                for message in self.session.remove_items(&tx, &keys).await {
                    self.send_event(None, EventKind::Log { message });
                }
                self.send_event(request_id, EventKind::Removed { keys });
                if self.session.timed_out {
                    return self
                        .close_timed_out(
                            request_id,
                            Operation::Remove,
                            BackendMsg::RequestTimedOutSessionClosed,
                        )
                        .await;
                }
                Flow::Continue
            }
            CommandKind::Read { keys } => {
                let values = read(&mut self.session, &keys).await;
                self.send_event(request_id, EventKind::Values { values });
                if self.session.timed_out {
                    return self
                        .close_timed_out(
                            request_id,
                            Operation::Read,
                            BackendMsg::RequestTimedOutSessionClosed,
                        )
                        .await;
                }
                Flow::Continue
            }
            CommandKind::Write { request } => {
                let key = request.key;
                let result = write(&mut self.session, request).await;
                if self.session.timed_out {
                    let message = match &result {
                        Ok(()) => BackendMsg::RequestTimedOutSessionClosed,
                        Err(message) => message.clone(),
                    };
                    self.send_event(request_id, EventKind::Wrote { key, result });
                    return self
                        .close_timed_out(request_id, Operation::Write, message)
                        .await;
                }
                self.send_event(request_id, EventKind::Wrote { key, result });
                Flow::Continue
            }
            CommandKind::Test => {
                let result = test(&mut self.session).await;
                self.settle(request_id, Operation::Test, result, |message| {
                    EventKind::Log { message }
                })
                .await
            }
            // Never routed to a session; answered by the host.
            CommandKind::Connect { .. }
            | CommandKind::Discover { .. }
            | CommandKind::SearchUaServers { .. }
            | CommandKind::SearchUaNetwork { .. }
            | CommandKind::ScanUaNetwork { .. }
            | CommandKind::CancelUaScan { .. } => {
                self.send_error(
                    request_id,
                    command.kind.operation(),
                    BackendMsg::NoConnection,
                );
                Flow::Continue
            }
        }
    }

    /// Dispatches a session-operation result. A timed-out operation poisons the
    /// session: it is closed gracefully and an `Error` + `Disconnected` pair is
    /// emitted, so a possibly in-flight response is never abandoned on a channel
    /// that stays open.
    async fn settle<T>(
        &mut self,
        request_id: Option<crate::bff::model::RequestId>,
        operation: Operation,
        result: Result<T, BackendMsg>,
        on_success: impl FnOnce(T) -> EventKind,
    ) -> Flow {
        let timed_out = self.session.timed_out;
        match (result, timed_out) {
            (Ok(value), false) => {
                self.send_event(request_id, on_success(value));
                Flow::Continue
            }
            (Err(message), false) => {
                self.send_error(request_id, operation, message);
                Flow::Continue
            }
            (Ok(value), true) => {
                self.send_event(request_id, on_success(value));
                self.close_timed_out(
                    request_id,
                    operation,
                    BackendMsg::RequestTimedOutSessionClosed,
                )
                .await
            }
            (Err(message), true) => self.close_timed_out(request_id, operation, message).await,
        }
    }

    /// Gracefully closes a session whose last request timed out, then reports
    /// `Error` + `Disconnected`.
    ///
    /// A timed-out request may still have a response in flight on the secure
    /// channel. `Session::disconnect()` enqueues CloseSession/CloseSecureChannel
    /// behind it and waits, so the server is never mid-response when the
    /// transport goes away. The subscription is deliberately not deleted first
    /// (that would enqueue yet another request behind the abandoned one); closing
    /// the session removes its subscriptions on the server. `abort()` remains
    /// only the last resort when even the graceful close cannot finish in
    /// `GRACEFUL_CLOSE_TIMEOUT`.
    async fn close_timed_out(
        &mut self,
        request_id: Option<crate::bff::model::RequestId>,
        operation: Operation,
        message: BackendMsg,
    ) -> Flow {
        // Close the inbox first: from now on the host answers "not connected"
        // itself and treats a new Connect of this id as a fresh session.
        self.inbox.close();
        self.session.close().await;
        self.send_error(request_id, operation, message);
        // The Error above consumes the request id (when the caller already
        // emitted a success for the same id, that Error is dropped by the app);
        // the session loss itself is an unsolicited runtime event, always
        // accepted.
        self.send_event(
            None,
            EventKind::Disconnected {
                reason: BackendMsg::SessionClosedAfterTimeout,
            },
        );
        Flow::Closed
    }
}

#[cfg(test)]
mod initial_load_regression {
    use super::super::common::{RequestTimeout, timeout_drain};
    use super::*;
    use crate::bff::model::Protocol;

    #[test]
    fn timeout_drains_same_future_and_discards_late_result() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = completed.clone();
            let result = timeout_drain(
                async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    42
                },
                Duration::from_millis(5),
            )
            .await;
            assert_eq!(result, Err(RequestTimeout));
            assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
            assert_eq!(
                timeout_drain(async { 7 }, Duration::from_secs(1)).await,
                Ok(7)
            );
        });
    }

    #[test]
    fn rejects_commands_with_mismatched_owner() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut worker = spawn(tx);
        for (owner, kind) in [
            (
                Owner::Discovery,
                CommandKind::Connect {
                    endpoint: EndpointSpec::ua("opc.tcp://127.0.0.1:4840"),
                },
            ),
            (
                Owner::Session(SessionId::FIRST),
                CommandKind::SearchUaNetwork {
                    bind: None,
                    lds: None,
                },
            ),
        ] {
            let id = crate::bff::model::RequestId::FIRST;
            worker
                .tx
                .as_ref()
                .unwrap()
                .send(Command {
                    request_id: Some(id),
                    owner,
                    protocol: Protocol::OpcUa,
                    kind,
                })
                .unwrap();
            let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(event.owner, owner);
            assert_eq!(event.origin, crate::bff::model::Origin::Reply(id));
            assert!(matches!(
                event.kind,
                EventKind::Error {
                    message: BackendMsg::NoConnection,
                    ..
                }
            ));
        }
        worker.shutdown(Duration::from_secs(5));
    }

    #[test]
    fn disconnect_interrupts_pending_type_loading() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            tx.send(ActorMsg::Command(Command {
                request_id: crate::bff::model::RequestId::new(8),
                owner: crate::bff::model::SessionId::FIRST.into(),
                protocol: Protocol::OpcUa,
                kind: CommandKind::Disconnect,
            }))
            .unwrap();
            let mut queue = CommandQueue::default();
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                wait_initial_load(std::future::pending::<()>(), &mut rx, &mut queue),
            )
            .await;
            assert!(matches!(
                result,
                Ok(InitialLoad::Disconnect(Command { request_id: Some(id), .. })) if id.get() == 8
            ));
        });
    }
}
