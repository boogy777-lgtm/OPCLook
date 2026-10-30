//! Experimental Gateway integration for the direct SDK ABI. This feature is
//! intentionally opt-in until the live gate and missing capabilities are done.
use super::da_router::{DaHost, SESSION_JOIN_BUDGET, no_connection_reply, run_router};
use super::model::{
    BackendMsg, Command, CommandKind, DataValue, Event, EventKind, ItemInfo, ItemKey, Operation,
    Owner, SessionId, WriteOutcome,
};
use super::softing_runtime::{Error as SdkError, Guid, Host, Session, SharedHost};
use std::collections::HashMap;
use std::sync::{
    Arc, OnceLock,
    mpsc::{self, Sender},
};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

// The SDK entry is process-global and its init/termination are thread-affine.
// This owner does not execute OPC operations: SDK server workers do that. It
// retains/reuses the same entry while any detached session still references it.
type AcquireReply = Sender<Result<SharedHost, String>>;
enum HostRequest {
    Acquire(AcquireReply),
    Finish(Sender<Result<(), String>>),
}
static HOST_MANAGER: OnceLock<Result<Sender<HostRequest>, String>> = OnceLock::new();
fn manager() -> Result<Sender<HostRequest>, String> {
    HOST_MANAGER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<HostRequest>();
            std::thread::Builder::new()
                .name("opc-da-sdk-owner".to_string())
                .spawn(move || {
                    let mut host: Option<Host> = None;
                    while let Ok(request) = rx.recv() {
                        match request {
                            HostRequest::Acquire(reply) => {
                                if host.is_none() {
                                    match Host::new() {
                                        Ok(created) => host = Some(created),
                                        Err(e) => {
                                            let _ = reply.send(Err(format!("{e:?}")));
                                            continue;
                                        }
                                    }
                                }
                                let result = host
                                    .as_ref()
                                    .map(Host::shared)
                                    .ok_or_else(|| "SDK owner unavailable".to_string());
                                let _ = reply.send(result);
                            }
                            HostRequest::Finish(reply) => {
                                if host.as_ref().is_some_and(|h| !h.is_idle()) {
                                    let _ =
                                        reply.send(Err("SDK sessions still running".to_string()));
                                    continue;
                                }
                                let result = host
                                    .take()
                                    .map(|h| h.close().map_err(|e| format!("SDK cleanup: {e:?}")))
                                    .unwrap_or(Ok(()));
                                let _ = reply.send(result);
                                break;
                            }
                        }
                    }
                })
                .map(|_| tx)
                .map_err(|e| e.to_string())
        })
        .clone()
}
fn acquire_host() -> Result<SharedHost, String> {
    let (reply, result) = mpsc::channel();
    manager()?
        .send(HostRequest::Acquire(reply))
        .map_err(|e| e.to_string())?;
    result.recv().map_err(|e| e.to_string())?
}
/// Final process shutdown only: the caller must have closed every Gateway.
/// A timeout bounds the caller even if SDK termination is stuck. A successful
/// finish is final: the process must not start another SDK Gateway afterward.
pub fn finish_native_sdk(timeout: Duration) -> Result<(), String> {
    let Some(manager) = HOST_MANAGER.get() else {
        return Ok(());
    };
    let manager = manager.as_ref().map_err(Clone::clone)?;
    let (reply, result) = mpsc::channel();
    manager
        .send(HostRequest::Finish(reply))
        .map_err(|e| e.to_string())?;
    result.recv_timeout(timeout).map_err(|e| e.to_string())?
}

struct NativeHost {
    sdk: SharedHost,
}
fn message(s: impl std::fmt::Display) -> BackendMsg {
    BackendMsg::raw(s.to_string())
}
fn write_outcome(result: Result<i32, SdkError>) -> WriteOutcome {
    match result {
        Ok(hr) => WriteOutcome::from_da(hr, format!("HRESULT {hr:#x}")),
        Err(SdkError::Native(hr)) => WriteOutcome::unknown_with_status(
            super::model::NativeWriteStatus::DaService(hr),
            format!("DA write HRESULT {hr:#x}"),
        ),
        Err(e) => WriteOutcome::unknown(format!("DA write: {e:?}")),
    }
}
fn send(tx: &Sender<Event>, owner: Owner, id: Option<super::model::RequestId>, kind: EventKind) {
    let _ = tx.send(Event {
        origin: id.into(),
        owner,
        kind,
    });
}
fn failure(tx: &Sender<Event>, command: &Command, operation: Operation, detail: BackendMsg) {
    send(
        tx,
        command.owner,
        command.request_id,
        EventKind::Error {
            operation,
            message: detail,
        },
    );
}
// A GUID endpoint is supported in this prototype; ProgID lookup and discovery
// will use the SDK browser and are explicitly unavailable until wired.
fn guid(input: &str) -> Option<Guid> {
    let s = input.trim().trim_start_matches('{').trim_end_matches('}');
    let p: Vec<&str> = s.split('-').collect();
    if p.len() != 5
        || p[0].len() != 8
        || p[1].len() != 4
        || p[2].len() != 4
        || p[3].len() != 4
        || p[4].len() != 12
    {
        return None;
    }
    let mut data4 = [0; 8];
    let tail = format!("{}{}", p[3], p[4]);
    for (i, byte) in data4.iter_mut().enumerate() {
        *byte = u8::from_str_radix(tail.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(Guid {
        data1: u32::from_str_radix(p[0], 16).ok()?,
        data2: u16::from_str_radix(p[1], 16).ok()?,
        data3: u16::from_str_radix(p[2], 16).ok()?,
        data4,
    })
}
impl DaHost for NativeHost {
    fn run_discovery(&self, command: Command, events: &Sender<Event>) {
        failure(
            events,
            &command,
            Operation::Discover,
            message("DA SDK discovery is not wired"),
        );
    }
    fn run_session(
        &self,
        id: SessionId,
        commands: &mut UnboundedReceiver<Command>,
        events: &Sender<Event>,
    ) {
        let mut session: Option<Session> = None;
        let mut items: HashMap<ItemKey, u64> = HashMap::new();
        while let Some(command) = commands.blocking_recv() {
            if command.owner != id.into() {
                if let Some(reply) = no_connection_reply(&command) {
                    let _ = events.send(reply);
                }
                continue;
            }
            let owner = command.owner;
            let request_id = command.request_id;
            let mut done = false;
            match &command.kind {
                CommandKind::Connect { endpoint } => {
                    let result = guid(&endpoint.address)
                        .ok_or_else(|| message("DA SDK preview requires a CLSID endpoint"))
                        .and_then(|clsid| {
                            self.sdk
                                .open("", &endpoint.host, &clsid)
                                .map_err(|e| message(format!("DA connect: {e:?}")))
                        });
                    match result {
                        Ok(open) => {
                            session = Some(open);
                            send(
                                events,
                                owner,
                                request_id,
                                EventKind::Connected {
                                    endpoint: endpoint.clone(),
                                    info: message("DA SDK connected"),
                                },
                            );
                        }
                        Err(error) => {
                            failure(events, &command, Operation::Connect, error);
                            done = true;
                        }
                    }
                }
                CommandKind::Disconnect => {
                    let reason = match session.take() {
                        Some(s) => match s.close() {
                            Ok(()) => message("DA disconnected"),
                            Err(e) => message(format!("DA close: {e:?}")),
                        },
                        None => BackendMsg::NoConnection,
                    };
                    send(
                        events,
                        owner,
                        request_id,
                        EventKind::Disconnected { reason },
                    );
                    done = true;
                }
                CommandKind::AddItems { items: requested } => {
                    let mut results = Vec::with_capacity(requested.len());
                    for item in requested {
                        let result = if items.contains_key(&item.key) {
                            Err(message("DA item already registered"))
                        } else if let Some(s) = session.as_mut() {
                            s.add(&item.item_id)
                                .map(|(reg, vt, rights)| {
                                    items.insert(item.key, reg);
                                    ItemInfo {
                                        data_type: Some(
                                            super::vt::vt_name(super::model::VarType(vt))
                                                .to_string(),
                                        ),
                                        type_id: Some(vt),
                                        writable: rights & 2 != 0,
                                    }
                                })
                                .map_err(|e| message(format!("DA add item: {e:?}")))
                        } else {
                            Err(BackendMsg::NoConnection)
                        };
                        results.push((item.key, result));
                    }
                    send(events, owner, request_id, EventKind::AddItems { results });
                }
                CommandKind::RemoveItems { keys } => {
                    let mut err = None;
                    for key in keys {
                        if let (Some(s), Some(reg)) = (session.as_mut(), items.remove(key)) {
                            if let Err(e) = s.remove(reg) {
                                err = Some(message(format!("DA remove: {e:?}")));
                            }
                        }
                    }
                    if let Some(detail) = err {
                        failure(events, &command, Operation::Remove, detail);
                    } else {
                        send(
                            events,
                            owner,
                            request_id,
                            EventKind::Removed { keys: keys.clone() },
                        );
                    }
                }
                CommandKind::Read { keys } => {
                    let values = keys
                        .iter()
                        .map(|key| {
                            let result = session
                                .as_mut()
                                .zip(items.get(key))
                                .ok_or(BackendMsg::SignalNotSubscribed)
                                .and_then(|(s, reg)| {
                                    s.read_scalar(*reg, u32::MAX)
                                        .map_err(|e| message(format!("DA read: {e:?}")))
                                })
                                .and_then(super::softing_scalar::decode);
                            (*key, result.unwrap_or_else(DataValue::bad))
                        })
                        .collect();
                    send(events, owner, request_id, EventKind::Values { values });
                }
                CommandKind::Write { request } => {
                    let key = request.key;
                    let result = match (session.as_mut(), items.get(&key)) {
                        (None, _) => Err(BackendMsg::NoConnection),
                        (_, None) => Err(BackendMsg::SignalNotSubscribed),
                        (Some(s), Some(reg)) => match (&request.value, request.type_hint) {
                            (super::model::Value::Str(text), hint) if hint.is_none_or(|v| v.0 == 8) => {
                                if text.contains('\0') || text.len() > 65_536 {
                                    Err(message("DA string contains NUL or exceeds the SDK limit"))
                                } else {
                                    Ok(write_outcome(s.write_text(*reg, text)))
                                }
                            }
                            _ => super::softing_scalar::encode(&request.value, request.type_hint.map(|v| v.0))
                                .map(|(vt, bits)| write_outcome(s.write_scalar(*reg, vt, bits))),
                        },
                    };
                    send(events, owner, request_id, EventKind::Wrote { key, result });
                }
                CommandKind::Browse { .. } => failure(
                    events,
                    &command,
                    Operation::Browse,
                    message("DA SDK browse is not wired"),
                ),
                CommandKind::Test => failure(
                    events,
                    &command,
                    Operation::Test,
                    message("DA SDK status is not wired"),
                ),
                _ => {
                    if let Some(reply) = no_connection_reply(&command) {
                        let _ = events.send(reply);
                    }
                }
            }
            if done {
                break;
            }
        }
        if let Some(s) = session {
            if let Err(e) = s.close() {
                send(
                    events,
                    id.into(),
                    None,
                    EventKind::Log {
                        message: message(format!("DA close: {e:?}")),
                    },
                );
            }
        }
    }
}

pub(crate) fn spawn(events: Sender<Event>) -> super::WorkerHandle {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Command>();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let join = std::thread::Builder::new()
        .name("opc-da-sdk".to_string())
        .spawn(move || {
            let outcome = match acquire_host() {
                Ok(sdk) => run_router(
                    Arc::new(NativeHost { sdk }),
                    rx,
                    events.clone(),
                    SESSION_JOIN_BUDGET,
                ),
                Err(e) => {
                    let mut rx = rx;
                    send(
                        &events,
                        Owner::Discovery,
                        None,
                        EventKind::Log {
                            message: message(format!("DA SDK initialization: {e}")),
                        },
                    );
                    while let Some(command) = rx.blocking_recv() {
                        if let Some(reply) = no_connection_reply(&command) {
                            let _ = events.send(reply);
                        }
                    }
                    super::ShutdownOutcome::Joined
                }
            };
            let _ = done_tx.send(());
            outcome
        });
    match join {
        Ok(join) => super::WorkerHandle::new(tx, join, done_rx),
        Err(_) => super::WorkerHandle::closed(done_rx),
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{EndpointSpec, ItemSpec, Protocol, RequestId, Value, WriteRequest};
    use super::*;
    use std::time::Duration;
    #[test]
    fn preview_contract_connect_add_read_write_remove() {
        let first_host = acquire_host().unwrap();
        let retained = first_host.clone();
        drop(first_host);
        let reused = acquire_host().unwrap();
        assert!(retained.same_host(&reused));
        drop(retained);
        drop(reused);
        let (tx, rx) = std::sync::mpsc::channel();
        let mut worker = spawn(tx);
        let cmd = worker.tx.take().unwrap();
        let owner = Owner::Session(SessionId::FIRST);
        let send = |kind| {
            cmd.send(Command {
                protocol: Protocol::OpcDa,
                owner,
                request_id: Some(RequestId::FIRST),
                kind,
            })
            .unwrap()
        };
        let recv = || rx.recv_timeout(Duration::from_secs(5)).unwrap().kind;
        send(CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "{7D4BD1B1-7422-48C0-AD58-601B701449C6}"),
        });
        assert!(matches!(recv(), EventKind::Connected { .. }));
        send(CommandKind::AddItems {
            items: vec![ItemSpec {
                key: ItemKey::FIRST,
                item_id: "Fixture.Value".into(),
            }],
        });
        assert!(
            matches!(recv(), EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
        );
        send(CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        });
        assert!(
            matches!(recv(), EventKind::Values { values } if values[0].1.value == Value::I64(42))
        );
        send(CommandKind::Write {
            request: WriteRequest {
                key: ItemKey::FIRST,
                value: Value::I64(7),
                type_hint: None,
            },
        });
        assert!(matches!(recv(), EventKind::Wrote { result: Ok(_), .. }));
        send(CommandKind::RemoveItems {
            keys: vec![ItemKey::FIRST],
        });
        assert!(matches!(recv(), EventKind::Removed { .. }));
        send(CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        });
        assert!(
            matches!(recv(), EventKind::Values { values } if values[0].1.reason == Some(BackendMsg::SignalNotSubscribed))
        );
        send(CommandKind::Disconnect);
        assert!(matches!(recv(), EventKind::Disconnected { .. }));
        drop(cmd);
        worker.join.take().unwrap().join().unwrap();
    }
}
