use super::discovery::{search_network_servers, search_servers};
use super::io::{browse, browse_views, read, resolve_browse_path, test, write};
use super::scanner::{ScanRegistry, abort_all_scans, start_scan_task};
use super::session::{UaSession, connect};
use crate::bff::model::{Command, CommandKind, DataValue, Event, EventKind, Operation, SessionId};
use crate::constants::GRACEFUL_CLOSE_TIMEOUT;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

pub(super) fn spawn(event_tx: Sender<Event>) -> crate::bff::WorkerHandle {
    let (cmd_tx, mut cmd_rx): (
        UnboundedSender<Command>,
        tokio::sync::mpsc::UnboundedReceiver<Command>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let join = std::thread::Builder::new()
        .name("opc-ua".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
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
                            message: format!("не удалось запустить tokio runtime: {err}"),
                        },
                    });
                    let _ = done_tx.send(());
                    return;
                }
            };
            runtime.block_on(async move {
                let mut sessions: HashMap<SessionId, UaSession> = HashMap::new();
                let mut queued: VecDeque<Command> = VecDeque::new();
                let scans: ScanRegistry = Arc::new(Mutex::new(HashMap::new()));
                while let Some(command) = cmd_rx.recv().await {
                    if is_control_command(&command.kind) {
                        // A control command must not wait behind queued periodic
                        // reads (browse/refresh/reconnect latency). Reads queued
                        // behind it are re-ordered to the back, never dropped, so
                        // a response-backed read is never lost.
                        queue_channel_drain(&mut queued, &mut cmd_rx);
                        let (controls, reads) = partition_commands(&mut queued);
                        handle_command(command, &mut sessions, &scans, &event_tx).await;
                        for next in controls {
                            handle_command(next, &mut sessions, &scans, &event_tx).await;
                        }
                        for back in reads {
                            handle_command(back, &mut sessions, &scans, &event_tx).await;
                        }
                    } else {
                        handle_command(command, &mut sessions, &scans, &event_tx).await;
                    }
                }
                // No scan may outlive the worker or its event channel.
                abort_all_scans(&scans).await;
                // Command channel closed (gateway shutdown): close every live
                // session on the wire before the worker thread exits.
                for (_, mut session) in sessions {
                    session.teardown_subscription(&event_tx).await;
                    let closed =
                        tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.session.disconnect())
                            .await;
                    let joined =
                        tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut session.event_loop).await;
                    if closed.is_err() || joined.is_err() {
                        session.event_loop.abort();
                    }
                }
            });
            let _ = done_tx.send(());
        })
        .expect("failed to spawn OPC UA worker");
    crate::bff::WorkerHandle::new(cmd_tx, join, done_rx)
}

/// True when the command should jump ahead of queued periodic reads
/// (browse/refresh/reconnect latency). Reads are independent best-effort polls,
/// so they are safe to defer; they are re-ordered, never dropped.
pub(super) fn is_control_command(kind: &CommandKind) -> bool {
    !matches!(kind, CommandKind::Read { .. } | CommandKind::Test)
}

/// Drains a tokio channel into a local queue so control commands can be
/// prioritized over a backlog of periodic reads.
fn queue_channel_drain(
    queue: &mut VecDeque<Command>,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Command>,
) {
    while let Ok(next) = rx.try_recv() {
        queue.push_back(next);
    }
}

/// Splits a queued command list into control commands (processed first) and
/// read/test commands (deferred). Reads are never lost, only re-ordered after
/// control commands so those jump the queue.
pub(super) fn partition_commands(
    queue: &mut VecDeque<Command>,
) -> (VecDeque<Command>, VecDeque<Command>) {
    let mut controls = VecDeque::new();
    let mut reads = VecDeque::new();
    while let Some(c) = queue.pop_front() {
        if is_control_command(&c.kind) {
            controls.push_back(c);
        } else {
            reads.push_back(c);
        }
    }
    (controls, reads)
}

async fn handle_command(
    command: Command,
    sessions: &mut HashMap<SessionId, UaSession>,
    scans: &ScanRegistry,
    tx: &Sender<Event>,
) {
    let request_id = command.request_id;
    let session_id = command.session;
    match command.kind {
        CommandKind::Discover { .. } => send_error(
            tx,
            request_id,
            session_id,
            Operation::Discover,
            "для OPC UA укажите URL сервера (opc.tcp://host:port)",
        ),
        CommandKind::SearchUaServers { host, port, bind } => {
            match search_servers(&host, port, bind.as_ref()).await {
                Ok(servers) => send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::UaServersFound {
                            host,
                            port,
                            servers,
                        },
                    },
                ),
                Err(message) => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::SearchServers,
                    message,
                ),
            }
        }
        CommandKind::SearchUaNetwork { bind } => {
            match search_network_servers(bind.as_ref()).await {
                Ok(servers) => send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::UaNetworkFound { servers },
                    },
                ),
                Err(message) => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::SearchNetwork,
                    message,
                ),
            }
        }
        CommandKind::ScanUaNetwork {
            scan_id,
            bind,
            subnet,
            local_ip,
        } => {
            start_scan_task(scan_id, bind, subnet, local_ip, scans, tx);
        }
        CommandKind::CancelUaScan { scan_id } => {
            if let Some(scan) = scans
                .lock()
                .ok()
                .and_then(|scans| scans.get(&scan_id).map(|scan| Arc::clone(&scan.cancel)))
            {
                scan.store(true, Ordering::Relaxed);
            }
        }
        CommandKind::Connect { endpoint } => {
            if sessions.contains_key(&session_id) {
                send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::Connect,
                    "сессия уже подключена",
                );
                return;
            }
            match connect(session_id, endpoint.clone()).await {
                Ok(session) => {
                    let info = session.info.clone();
                    sessions.insert(session_id, session);
                    send(
                        tx,
                        Event {
                            request_id,
                            session: session_id,
                            kind: EventKind::Connected { endpoint, info },
                        },
                    );
                }
                Err(message) => send_error(tx, request_id, session_id, Operation::Connect, message),
            }
        }
        CommandKind::Disconnect => {
            if let Some(mut session) = sessions.remove(&session_id) {
                session.teardown_subscription(tx).await;
                // Graceful teardown: CloseSession + CloseSecureChannel on the
                // wire, then wait for the event loop to finish on its own. The
                // loop only returns once the channel is fully closed, so aborting
                // it first would leave the server mid-response (the 0x80310000
                // "no transport connection" errors). Abort only as a last resort
                // when the graceful close cannot complete (e.g. network already
                // gone) so a stuck worker never hangs the Disconnect command.
                let closed =
                    tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.session.disconnect())
                        .await;
                let joined =
                    tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut session.event_loop).await;
                if closed.is_err() || joined.is_err() {
                    session.event_loop.abort();
                }
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Disconnected {
                            reason: "соединение закрыто".to_string(),
                        },
                    },
                );
            }
        }
        CommandKind::Browse {
            node_id,
            filter,
            view_id,
        } => {
            let outcome = match sessions.get_mut(&session_id) {
                Some(session) => {
                    let result = browse(session, &node_id, filter, view_id).await;
                    Some((result, session.timed_out))
                }
                None => None,
            };
            match outcome {
                Some((result, timed_out)) => {
                    settle(
                        sessions,
                        session_id,
                        tx,
                        request_id,
                        Operation::Browse,
                        result,
                        timed_out,
                        move |children| EventKind::Browse { node_id, children },
                    )
                    .await;
                }
                None => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::Browse,
                    "нет соединения",
                ),
            }
        }
        CommandKind::BrowseViews => {
            let outcome = match sessions.get_mut(&session_id) {
                Some(session) => {
                    let result = browse_views(session).await;
                    Some((result, session.timed_out))
                }
                None => None,
            };
            match outcome {
                Some((result, timed_out)) => {
                    settle(
                        sessions,
                        session_id,
                        tx,
                        request_id,
                        Operation::BrowseViews,
                        result,
                        timed_out,
                        |children| EventKind::Views { children },
                    )
                    .await;
                }
                None => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::BrowseViews,
                    "нет соединения",
                ),
            }
        }
        CommandKind::ResolveBrowsePath { start_node, path } => {
            let outcome = match sessions.get_mut(&session_id) {
                Some(session) => {
                    let result = resolve_browse_path(session, start_node, &path).await;
                    Some((result, session.timed_out))
                }
                None => None,
            };
            match outcome {
                Some((result, timed_out)) => {
                    settle(
                        sessions,
                        session_id,
                        tx,
                        request_id,
                        Operation::ResolveBrowsePath,
                        result,
                        timed_out,
                        |targets| EventKind::PathResolved { targets },
                    )
                    .await;
                }
                None => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::ResolveBrowsePath,
                    "нет соединения",
                ),
            }
        }
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms,
            deadband,
        } => match sessions.get_mut(&session_id) {
            Some(session) => {
                session.publishing_interval_ms = publishing_interval_ms.max(100);
                session.deadband = deadband.max(0.0);
            }
            None => send_error(
                tx,
                request_id,
                session_id,
                Operation::Subscription,
                "нет соединения",
            ),
        },
        CommandKind::AddItems { items } => match sessions.get_mut(&session_id) {
            Some(session) => {
                let (results, warning) = session.add_items(&items, tx).await;
                let timed_out = session.timed_out;
                if let Some(message) = warning {
                    // A command may produce exactly one request-id-correlated
                    // reply; ancillary diagnostics are unsolicited (id 0) so
                    // they cannot consume the AddItems reply's pending entry.
                    send(
                        tx,
                        Event {
                            request_id: 0,
                            session: session_id,
                            kind: EventKind::Log { message },
                        },
                    );
                }
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::AddItems {
                            results: results.into_iter().map(|(k, i)| (k, Ok(i))).collect(),
                        },
                    },
                );
                if timed_out {
                    close_timed_out_session(
                        sessions,
                        session_id,
                        tx,
                        request_id,
                        Operation::Add,
                        "таймаут запроса OPC UA: сессия закрыта".to_string(),
                    )
                    .await;
                }
            }
            None => send_error(tx, request_id, session_id, Operation::Add, "нет соединения"),
        },
        CommandKind::RemoveItems { keys } => {
            let timed_out = match sessions.get_mut(&session_id) {
                Some(session) => {
                    session.remove_items(tx, &keys).await;
                    session.timed_out
                }
                None => {
                    send_error(
                        tx,
                        request_id,
                        session_id,
                        Operation::Remove,
                        "нет соединения",
                    );
                    false
                }
            };
            if sessions.contains_key(&session_id) {
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Removed { keys },
                    },
                );
            }
            if timed_out {
                close_timed_out_session(
                    sessions,
                    session_id,
                    tx,
                    request_id,
                    Operation::Remove,
                    "таймаут запроса OPC UA: сессия закрыта".to_string(),
                )
                .await;
            }
        }
        CommandKind::Read { keys } => match sessions.get_mut(&session_id) {
            Some(session) => {
                let values = read(session, &keys).await;
                let timed_out = session.timed_out;
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Values { values },
                    },
                );
                if timed_out {
                    close_timed_out_session(
                        sessions,
                        session_id,
                        tx,
                        request_id,
                        Operation::Read,
                        "таймаут запроса OPC UA: сессия закрыта".to_string(),
                    )
                    .await;
                }
            }
            None => send(
                tx,
                Event {
                    request_id,
                    session: session_id,
                    kind: EventKind::Values {
                        values: keys
                            .into_iter()
                            .map(|key| (key, DataValue::bad("нет соединения")))
                            .collect(),
                    },
                },
            ),
        },
        CommandKind::Write { request } => {
            let key = request.key;
            let outcome = match sessions.get_mut(&session_id) {
                Some(session) => {
                    let result = write(session, request).await;
                    Some((result, session.timed_out))
                }
                None => None,
            };
            match outcome {
                Some((result, timed_out)) => {
                    if timed_out {
                        let message = match &result {
                            Ok(()) => "таймаут запроса OPC UA: сессия закрыта".to_string(),
                            Err(message) => message.clone(),
                        };
                        send(
                            tx,
                            Event {
                                request_id,
                                session: session_id,
                                kind: EventKind::Wrote { key, result },
                            },
                        );
                        close_timed_out_session(
                            sessions,
                            session_id,
                            tx,
                            request_id,
                            Operation::Write,
                            message,
                        )
                        .await;
                    } else {
                        send(
                            tx,
                            Event {
                                request_id,
                                session: session_id,
                                kind: EventKind::Wrote { key, result },
                            },
                        );
                    }
                }
                None => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::Write,
                    "нет соединения",
                ),
            }
        }
        CommandKind::Test => {
            let outcome = match sessions.get_mut(&session_id) {
                Some(session) => {
                    let result = test(session).await;
                    Some((result, session.timed_out))
                }
                None => None,
            };
            match outcome {
                Some((result, timed_out)) => {
                    settle(
                        sessions,
                        session_id,
                        tx,
                        request_id,
                        Operation::Test,
                        result,
                        timed_out,
                        |message| EventKind::Log { message },
                    )
                    .await;
                }
                None => send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::Test,
                    "нет соединения",
                ),
            }
        }
    }
}

pub(super) fn send(tx: &Sender<Event>, event: Event) {
    let _ = tx.send(event);
}

fn send_error(
    tx: &Sender<Event>,
    request_id: u64,
    session: SessionId,
    operation: Operation,
    message: impl Into<String>,
) {
    send(
        tx,
        Event {
            request_id,
            session,
            kind: EventKind::Error {
                operation,
                message: message.into(),
            },
        },
    );
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

/// Gracefully closes a session whose last request timed out, then reports
/// `Error` + `Disconnected`.
///
/// A timed-out request may still have a response in flight on the secure
/// channel. `Session::disconnect()` enqueues CloseSession/CloseSecureChannel
/// behind it and waits, so the server is never mid-response when the transport
/// goes away. The subscription is deliberately not deleted first (that would
/// enqueue yet another request behind the abandoned one); closing the session
/// removes its subscriptions on the server. `abort()` remains only the last
/// resort when even the graceful close cannot finish in `GRACEFUL_CLOSE_TIMEOUT`.
async fn close_timed_out_session(
    sessions: &mut HashMap<SessionId, UaSession>,
    session_id: SessionId,
    event_tx: &Sender<Event>,
    request_id: u64,
    operation: Operation,
    message: String,
) {
    if let Some(mut session) = sessions.remove(&session_id) {
        let closed =
            tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.session.disconnect()).await;
        let joined = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut session.event_loop).await;
        if closed.is_err() || joined.is_err() {
            session.event_loop.abort();
        }
    }
    send_error(event_tx, request_id, session_id, operation, message);
    // The Error above consumes the request id (when the caller already emitted
    // a success for the same id, that Error is dropped by the app); the session
    // loss itself is an unsolicited runtime event, always accepted.
    send(
        event_tx,
        Event {
            request_id: 0,
            session: session_id,
            kind: EventKind::Disconnected {
                reason: "сессия закрыта после таймаута запроса".to_string(),
            },
        },
    );
}

/// Dispatches a session-operation result. A timed-out operation poisons the
/// session: it is closed gracefully and an `Error` + `Disconnected` pair is
/// emitted, so a possibly in-flight response is never abandoned on a channel
/// that stays open.
async fn settle<T>(
    sessions: &mut HashMap<SessionId, UaSession>,
    session_id: SessionId,
    event_tx: &Sender<Event>,
    request_id: u64,
    operation: Operation,
    result: Result<T, String>,
    timed_out: bool,
    on_success: impl FnOnce(T) -> EventKind,
) {
    match (result, timed_out) {
        (Ok(value), false) => send(
            event_tx,
            Event {
                request_id,
                session: session_id,
                kind: on_success(value),
            },
        ),
        (Err(message), false) => send_error(event_tx, request_id, session_id, operation, message),
        (Ok(value), true) => {
            send(
                event_tx,
                Event {
                    request_id,
                    session: session_id,
                    kind: on_success(value),
                },
            );
            close_timed_out_session(
                sessions,
                session_id,
                event_tx,
                request_id,
                operation,
                "таймаут запроса OPC UA: сессия закрыта".to_string(),
            )
            .await;
        }
        (Err(message), true) => {
            close_timed_out_session(
                sessions, session_id, event_tx, request_id, operation, message,
            )
            .await;
        }
    }
}
