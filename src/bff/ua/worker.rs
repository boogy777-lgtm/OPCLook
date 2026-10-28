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

use super::discovery::{search_network_servers, search_servers};
use super::io::{browse, browse_views, read, resolve_browse_path, test, write};
use super::scanner::{ScanRegistry, abort_all_scans, start_scan_task};
use super::session::{UaSession, connect};
use crate::bff::model::{
    BackendMsg, Command, CommandKind, DataValue, EndpointSpec, Event, EventKind, Operation,
    SessionId,
};
use crate::constants::{GRACEFUL_CLOSE_TIMEOUT, UA_HOST_RUNTIME_THREADS, UA_SESSION_BACKLOG_LIMIT};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::atomic::Ordering;
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
                        request_id: 0,
                        session: 0,
                        kind: EventKind::Error {
                            operation: Operation::UaRuntime,
                            message: BackendMsg::UaRuntimeStartFailed {
                                detail: err.to_string(),
                            },
                        },
                    });
                    let _ = done_tx.send(());
                    return;
                }
            };
            runtime.block_on(run_host(cmd_rx, event_tx));
            let _ = done_tx.send(());
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
        request_id: u64,
        message: BackendMsg,
    },
    /// An actor has finished and released its session.
    Closed { session: SessionId, generation: u64 },
}

struct Ready {
    session: SessionId,
    generation: u64,
    request_id: u64,
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
        let session = command.session;
        match command.kind {
            CommandKind::Discover { .. } => send_error(
                &self.event_tx,
                request_id,
                session,
                Operation::Discover,
                BackendMsg::UaNeedsServerUrl,
            ),
            CommandKind::SearchUaServers { host, port, bind } => {
                let tx = self.event_tx.clone();
                self.tasks.spawn(async move {
                    match search_servers(&host, port, bind.as_ref()).await {
                        Ok(searched) => {
                            send_notes(&tx, session, searched.notes);
                            send(
                                &tx,
                                Event {
                                    request_id,
                                    session,
                                    kind: EventKind::UaServersFound {
                                        host,
                                        port,
                                        servers: searched.found,
                                    },
                                },
                            );
                        }
                        Err(message) => {
                            send_error(&tx, request_id, session, Operation::SearchServers, message)
                        }
                    }
                });
            }
            CommandKind::SearchUaNetwork { bind, lds } => {
                let tx = self.event_tx.clone();
                self.tasks.spawn(async move {
                    match search_network_servers(bind.as_ref(), lds.as_ref()).await {
                        Ok(searched) => {
                            send_notes(&tx, session, searched.notes);
                            send(
                                &tx,
                                Event {
                                    request_id,
                                    session,
                                    kind: EventKind::UaNetworkFound {
                                        servers: searched.found,
                                    },
                                },
                            );
                        }
                        Err(message) => {
                            send_error(&tx, request_id, session, Operation::SearchNetwork, message)
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
                if let Some(scan) = self
                    .scans
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&scan_id)
                    .map(|scan| Arc::clone(&scan.cancel))
                {
                    scan.store(true, Ordering::Relaxed);
                }
            }
            CommandKind::Connect { endpoint } => self.start_connect(request_id, session, endpoint),
            CommandKind::Disconnect => self.disconnect(command_for_disconnect(request_id, session)),
            _ => self.route(Command {
                request_id,
                session,
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

    fn start_connect(&mut self, request_id: u64, session: SessionId, endpoint: EndpointSpec) {
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
        let session = command.session;
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
        let session = command.session;
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
                request_id,
                session,
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

fn command_for_disconnect(request_id: u64, session: SessionId) -> Command {
    Command {
        request_id,
        session,
        protocol: crate::bff::model::Protocol::OpcUa,
        kind: CommandKind::Disconnect,
    }
}

// ---------------------------------------------------------------------------
// Connect task
// ---------------------------------------------------------------------------

struct ConnectJob {
    request_id: u64,
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
            connect(session, endpoint.clone(), &cancel).await
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
                    0,
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
                        0,
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

    fn send_error(&self, request_id: u64, operation: Operation, message: BackendMsg) {
        send_error(&self.event_tx, request_id, self.id, operation, message);
    }

    fn send_event(&self, request_id: u64, kind: EventKind) {
        send(
            &self.event_tx,
            Event {
                request_id,
                session: self.id,
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
                    self.send_event(0, EventKind::Log { message });
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
                    self.send_event(0, EventKind::Log { message });
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
                    self.send_event(0, EventKind::Log { message });
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
        request_id: u64,
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
        request_id: u64,
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
            0,
            EventKind::Disconnected {
                reason: BackendMsg::SessionClosedAfterTimeout,
            },
        );
        Flow::Closed
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub(super) fn send(tx: &Sender<Event>, event: Event) {
    let _ = tx.send(event);
}

/// Diagnostics of a search or scan as journal lines. They are unsolicited
/// (request id 0), so they never take the place of the request's own reply.
fn send_notes(tx: &Sender<Event>, session: SessionId, notes: Vec<BackendMsg>) {
    for message in notes {
        send(
            tx,
            Event {
                request_id: 0,
                session,
                kind: EventKind::Log { message },
            },
        );
    }
}

fn send_error(
    tx: &Sender<Event>,
    request_id: u64,
    session: SessionId,
    operation: Operation,
    message: BackendMsg,
) {
    send(
        tx,
        Event {
            request_id,
            session,
            kind: EventKind::Error { operation, message },
        },
    );
}

/// Answers a command addressed to a session that is not (or no longer)
/// connected. Exactly one reply per command; `Disconnect` has nothing to close
/// and stays silent.
pub(super) fn reply_no_connection(tx: &Sender<Event>, command: Command) {
    let Command {
        request_id,
        session,
        kind,
        ..
    } = command;
    let error = |operation: Operation| {
        send_error(tx, request_id, session, operation, BackendMsg::NoConnection);
    };
    match kind {
        CommandKind::Browse { .. } => error(Operation::Browse),
        CommandKind::BrowseViews => error(Operation::BrowseViews),
        CommandKind::ResolveBrowsePath { .. } => error(Operation::ResolveBrowsePath),
        CommandKind::ConfigureUaSubscription { .. } => error(Operation::Subscription),
        CommandKind::AddItems { .. } => error(Operation::Add),
        CommandKind::RemoveItems { .. } => error(Operation::Remove),
        CommandKind::Write { .. } => error(Operation::Write),
        CommandKind::Test => error(Operation::Test),
        CommandKind::Read { keys } => send(
            tx,
            Event {
                request_id,
                session,
                kind: EventKind::Values {
                    values: keys
                        .into_iter()
                        .map(|key| (key, DataValue::bad(BackendMsg::NoConnection)))
                        .collect(),
                },
            },
        ),
        CommandKind::Disconnect => {}
        CommandKind::Connect { .. } => error(Operation::Connect),
        other => error(other.operation()),
    }
}

/// Runs a service request with `budget`. If the budget expires, keeps polling
/// the SAME request future for one `GRACEFUL_CLOSE_TIMEOUT` instead of dropping
/// it, so a response that is already in flight is drained before the caller
/// closes the session. Returns the value if the future completed at any point
/// plus whether the caller's budget expired.
pub(super) async fn timeout_drain<T>(
    future: impl Future<Output = T>,
    budget: Duration,
) -> (Option<T>, bool) {
    tokio::pin!(future);
    match tokio::time::timeout(budget, &mut future).await {
        Ok(value) => (Some(value), false),
        Err(_) => match tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut future).await {
            Ok(value) => (Some(value), true),
            Err(_) => (None, true),
        },
    }
}

#[cfg(test)]
mod initial_load_regression {
    use super::*;
    use crate::bff::model::Protocol;

    #[test]
    fn disconnect_interrupts_pending_type_loading() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            tx.send(ActorMsg::Command(Command {
                request_id: 8,
                session: 1,
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
                Ok(InitialLoad::Disconnect(Command { request_id: 8, .. }))
            ));
        });
    }
}
