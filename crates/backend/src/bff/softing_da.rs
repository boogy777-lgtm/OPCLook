//! Experimental Gateway integration for the direct SDK ABI. This feature is
//! intentionally opt-in until the live gate and missing capabilities are done.
use super::da_clsid::{ClsidLookup, DiscoveredClsids, resolve_clsid};
use super::da_router::{DaHost, SESSION_JOIN_BUDGET, no_connection_reply, run_router};
use super::model::{
    BackendMsg, Command, CommandKind, DataValue, Event, EventKind, ItemInfo, ItemKey, Operation,
    Owner, SessionId, WriteOutcome,
};
use super::softing_runtime::{Error as SdkError, Guid, Host, Session, SharedHost};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, OnceLock, PoisonError,
    mpsc::{self, Sender},
};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, error::TryRecvError};

// The SDK entry is process-global and its init/termination are thread-affine.
// This owner does not execute OPC operations: SDK server workers do that. It
// retains/reuses the same entry while any detached session still references it.
type AcquireReply = Sender<Result<SharedHost, String>>;
enum HostRequest {
    Acquire(AcquireReply),
    Finish(Sender<Result<(), String>>),
}
// Feature-gated observations only; this does not retain a host or create sessions.
#[cfg(feature = "native-da-test-hooks")]
static HOST_ACQUISITIONS: Mutex<(Option<usize>, usize, bool)> = Mutex::new((None, 0, true));

/// Snapshot actual manager acquisitions in this test process, in manager order.
#[cfg(feature = "native-da-test-hooks")]
#[doc(hidden)]
pub fn test_host_acquisitions() -> (usize, bool) {
    let state = HOST_ACQUISITIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    (state.1, state.2)
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
                                #[cfg(feature = "native-da-test-hooks")]
                                if let Ok(ref shared) = result {
                                    let mut state = HOST_ACQUISITIONS
                                        .lock()
                                        .unwrap_or_else(PoisonError::into_inner);
                                    let identity = shared.test_identity();
                                    if let Some(first) = state.0 {
                                        state.2 &= first == identity;
                                    } else {
                                        state.0 = Some(identity);
                                    }
                                    state.1 = state.1.saturating_add(1);
                                }
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
    discovered: Mutex<DiscoveredClsids>,
}
fn guid_to_u128(g: Guid) -> u128 {
    (u128::from(g.data1) << 96)
        | (u128::from(g.data2) << 80)
        | (u128::from(g.data3) << 64)
        | u128::from(u64::from_be_bytes(g.data4))
}
fn guid_from_u128(v: u128) -> Guid {
    Guid {
        data1: (v >> 96) as u32,
        data2: (v >> 80) as u16,
        data3: (v >> 64) as u16,
        data4: (v as u64).to_be_bytes(),
    }
}
struct NativeClsidLookup<'a> {
    host: &'a NativeHost,
}
impl ClsidLookup for NativeClsidLookup<'_> {
    fn remote_enum(&mut self, node: &str, progid: &str) -> Result<u128, BackendMsg> {
        self.host
            .sdk
            .resolve(node, progid, true)
            .map(guid_to_u128)
            .map_err(|e| BackendMsg::DaOpcEnumUnavailable {
                host: node.to_string(),
                cause: Box::new(message(format!("{e:?}"))),
            })
    }
    fn discovered(&self, node: &str, progid: &str) -> Option<u128> {
        self.host
            .discovered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(node, progid)
    }
    fn local_registry(&mut self, progid: &str) -> Result<u128, BackendMsg> {
        self.host
            .sdk
            .resolve("", progid, false)
            .map(guid_to_u128)
            .map_err(|e| BackendMsg::DaProgIdLookupFailed {
                prog_id: progid.to_string(),
                detail: format!("{e:?}"),
            })
    }
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
fn drain_push(
    session: &mut Session,
    registrations: &HashMap<u64, ItemKey>,
    owner: Owner,
    epoch: super::model::RequestId,
    events: &Sender<Event>,
) -> Result<(), SdkError> {
    let (facts, overflow) = session.take_push()?;
    if overflow {
        send(
            events,
            owner,
            None,
            EventKind::Log {
                message: message(
                    "DA subscription queue overflow or invalid callback; updates were lost",
                ),
            },
        );
    }
    let mut values = Vec::new();
    for fact in facts {
        if fact.registration == 0 || fact.master_quality != 0 {
            send(
                events,
                owner,
                None,
                EventKind::Log {
                    message: message(format!(
                        "DA callback master quality HRESULT {:#x}, master error HRESULT {:#x}",
                        fact.master_quality, fact.value.service_result
                    )),
                },
            );
        }
        let Some(key) = registrations.get(&fact.registration) else {
            continue;
        };
        let value = if fact.adapter_result < 0 {
            DataValue::bad(message(format!(
                "DA callback adapter HRESULT {:#x}, service {:#x}, item {:#x}, quality {:#x}, FILETIME {:08x}:{:08x}, VT {:#x}",
                fact.adapter_result,
                fact.value.service_result,
                fact.value.item_result,
                fact.value.quality,
                fact.value.filetime_high,
                fact.value.filetime_low,
                fact.value.vartype
            )))
        } else {
            super::softing_scalar::decode((fact.value, fact.text)).unwrap_or_else(DataValue::bad)
        };
        values.push((*key, value));
    }
    if !values.is_empty() {
        let _ = events.send(Event {
            origin: super::model::Origin::Session(epoch),
            owner,
            kind: EventKind::Values { values },
        });
    }
    Ok(())
}
impl DaHost for NativeHost {
    fn run_discovery(&self, command: Command, events: &Sender<Event>) {
        let CommandKind::Discover { host } = &command.kind else {
            failure(
                events,
                &command,
                command.kind.operation(),
                BackendMsg::NoConnection,
            );
            return;
        };
        match self.sdk.discover(host) {
            Ok((records, fallback)) => {
                let mut servers = Vec::with_capacity(records.len());
                let mut missing_description = false;
                {
                    let mut cache = self
                        .discovered
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    for record in records {
                        cache.remember(host, &record.progid, guid_to_u128(record.clsid));
                        missing_description |= record.description_missing;
                        servers.push(super::model::ServerEntry {
                            id: record.progid,
                            description: record.description,
                        });
                    }
                }
                send(
                    events,
                    command.owner,
                    command.request_id,
                    EventKind::Servers {
                        host: host.clone(),
                        servers,
                    },
                );
                if fallback {
                    send(
                        events,
                        command.owner,
                        None,
                        EventKind::Log {
                            message: message(
                                "DA discovery used the SDK's local DA 1 registry fallback; newer-only servers may be absent",
                            ),
                        },
                    );
                }
                if missing_description {
                    send(
                        events,
                        command.owner,
                        None,
                        EventKind::Log {
                            message: message(
                                "DA discovery returned one or more servers without a description",
                            ),
                        },
                    );
                }
            }
            Err(error) => failure(
                events,
                &command,
                Operation::Discover,
                BackendMsg::DaServerListHostUnavailable {
                    host: host.clone(),
                    detail: format!("{error:?}"),
                },
            ),
        }
    }
    fn run_session(
        &self,
        id: SessionId,
        commands: &mut UnboundedReceiver<Command>,
        events: &Sender<Event>,
    ) {
        let mut session: Option<Session> = None;
        let mut items: HashMap<ItemKey, u64> = HashMap::new();
        let mut registrations: HashMap<u64, ItemKey> = HashMap::new();
        let mut subscription_epoch = None;
        loop {
            if let (Some(epoch), Some(s)) = (subscription_epoch, session.as_mut()) {
                if let Err(error) = drain_push(s, &registrations, id.into(), epoch, events) {
                    send(
                        events,
                        id.into(),
                        None,
                        EventKind::Log {
                            message: message(format!("DA callback queue: {error:?}")),
                        },
                    );
                    let _ = events.send(Event {
                        origin: super::model::Origin::Session(epoch),
                        owner: id.into(),
                        kind: EventKind::SubscriptionState {
                            state: super::model::SubscriptionState::Failed,
                        },
                    });
                    subscription_epoch = None;
                }
            }
            let command = if subscription_epoch.is_some() {
                match commands.try_recv() {
                    Ok(command) => command,
                    Err(TryRecvError::Empty) => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(TryRecvError::Disconnected) => break,
                }
            } else {
                let Some(command) = commands.blocking_recv() else {
                    break;
                };
                command
            };
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
                    let profile = endpoint.acquisition.as_ref();
                    let subscription = profile
                        .is_some_and(|p| p.update_mode == super::model::UpdateMode::Subscription);
                    let interval = profile.map(|p| p.interval_ms).unwrap_or(1000);
                    let epoch = if subscription { request_id } else { None };
                    let result = if subscription && epoch.is_none() {
                        Err(message(
                            "DA subscription requires a Connect request identity",
                        ))
                    } else {
                        u32::try_from(interval)
                            .map_err(|_| message("DA update interval out of range"))
                            .and_then(|interval_ms| {
                                guid(&endpoint.address)
                                    .map(Ok)
                                    .unwrap_or_else(|| {
                                        resolve_clsid(
                                            &endpoint.host,
                                            &endpoint.address,
                                            &mut NativeClsidLookup { host: self },
                                        )
                                        .map(guid_from_u128)
                                    })
                                    .and_then(|clsid| {
                                        self.sdk
                                            .open_config(
                                                "",
                                                &endpoint.host,
                                                &clsid,
                                                subscription,
                                                interval_ms,
                                            )
                                            .map_err(|e| message(format!("DA connect: {e:?}")))
                                    })
                            })
                    };
                    match result {
                        Ok((open, revised)) => {
                            session = Some(open);
                            send(
                                events,
                                owner,
                                request_id,
                                EventKind::Connected {
                                    endpoint: endpoint.clone(),
                                    info: message(format!(
                                        "DA connected; revised update rate {revised} ms"
                                    )),
                                },
                            );
                            if let Some(epoch) = epoch {
                                subscription_epoch = Some(epoch);
                                let _ = events.send(Event {
                                    origin: super::model::Origin::Session(epoch),
                                    owner,
                                    kind: EventKind::SubscriptionState {
                                        state: super::model::SubscriptionState::Active,
                                    },
                                });
                            }
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
                                    registrations.insert(reg, item.key);
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
                            registrations.remove(&reg);
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
                            (
                                super::model::Value::Bytes(_) | super::model::Value::Array(_),
                                hint,
                            ) => super::softing_scalar::encode_array(
                                &request.value,
                                hint.map(|v| v.0),
                            )
                            .map(|(vt, bytes)| write_outcome(s.write_array(*reg, vt, &bytes))),
                            (value, Some(hint)) if hint.0 & 0x2000 != 0 => {
                                super::softing_scalar::encode_array(value, Some(hint.0)).map(
                                    |(vt, bytes)| write_outcome(s.write_array(*reg, vt, &bytes)),
                                )
                            }
                            (value, hint)
                                if hint.is_some_and(|v| v.0 == 8)
                                    || (hint.is_none()
                                        && matches!(
                                            value,
                                            super::model::Value::Str(_)
                                                | super::model::Value::NodeId(_)
                                                | super::model::Value::QualifiedName { .. }
                                                | super::model::Value::LocalizedText { .. }
                                        )) =>
                            {
                                let text = match value {
                                    super::model::Value::Str(text) => text.clone(),
                                    other => other.to_string(),
                                };
                                if text.contains('\0') || text.len() > 65_536 {
                                    Err(message("DA string contains NUL or exceeds the SDK limit"))
                                } else {
                                    Ok(write_outcome(s.write_text(*reg, &text)))
                                }
                            }
                            _ => super::softing_scalar::encode(
                                &request.value,
                                request.type_hint.map(|v| v.0),
                            )
                            .map(|(vt, bits)| write_outcome(s.write_scalar(*reg, vt, bits))),
                        },
                    };
                    send(events, owner, request_id, EventKind::Wrote { key, result });
                }
                CommandKind::Browse { node_id, .. } => {
                    let result = session
                        .as_mut()
                        .ok_or(BackendMsg::NoConnection)
                        .and_then(|s| {
                            s.browse(node_id)
                                .map_err(|e| message(format!("DA browse: {e:?}")))
                        });
                    match result {
                        Ok(entries) => {
                            let children = entries
                                .into_iter()
                                .map(|entry| super::model::BrowseNode {
                                    node_id: entry.node_id,
                                    name: entry.name,
                                    kind: if entry.is_branch {
                                        super::model::NodeKind::Branch
                                    } else {
                                        super::model::NodeKind::Item
                                    },
                                    has_children: entry.is_branch,
                                })
                                .collect();
                            send(
                                events,
                                owner,
                                request_id,
                                EventKind::Browse {
                                    node_id: node_id.clone(),
                                    children,
                                },
                            );
                        }
                        Err(error) => failure(events, &command, Operation::Browse, error),
                    }
                }
                CommandKind::Test => {
                    let result = session
                        .as_mut()
                        .ok_or(BackendMsg::NoConnection)
                        .and_then(|s| {
                            s.test_status()
                                .map_err(|error| match error {
                                    SdkError::Native(hr) => BackendMsg::DaLinkLost {
                                        detail: format!("HRESULT 0x{:08X}", hr as u32),
                                    },
                                    other => BackendMsg::DaLinkLost {
                                        detail: format!("SDK status: {other:?}"),
                                    },
                                })
                                .and_then(super::da_status::check_server_state)
                        });
                    match result {
                        Ok(message) => send(events, owner, request_id, EventKind::Log { message }),
                        Err(error) => failure(events, &command, Operation::Test, error),
                    }
                }
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
                    Arc::new(NativeHost {
                        sdk,
                        discovered: Mutex::new(DiscoveredClsids::default()),
                    }),
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
    fn test_command_reports_sdk_state_and_native_error_through_router() {
        use super::super::model::Origin;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut worker = spawn(tx);
        let cmd = worker.tx.take().unwrap();
        let owner = Owner::Session(SessionId::new(301).unwrap());
        let request = |id, kind| {
            cmd.send(Command {
                protocol: Protocol::OpcDa,
                owner,
                request_id: Some(RequestId::new(id).unwrap()),
                kind,
            })
            .unwrap();
            let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(event.origin, Origin::Reply(RequestId::new(id).unwrap()));
            assert_eq!(event.owner, owner);
            event.kind
        };
        assert!(matches!(
            request(1, CommandKind::Test),
            EventKind::Error {
                operation: Operation::Test,
                message: BackendMsg::NoConnection,
            }
        ));
        for (index, (clsid, expected)) in [
            ("FEDC0001", 1),
            ("FEDC0002", 2),
            ("FEDC0003", 3),
            ("FEDC0004", 4),
            ("FEDC0005", 5),
            ("FEDC0006", 6),
            ("FEDC002A", 42),
            ("FEDC0000", 0),
            ("FEDCDEAD", i32::MIN + 5),
        ]
        .into_iter()
        .enumerate()
        {
            let base = 10 + index as u64 * 3;
            let endpoint =
                EndpointSpec::da("localhost", &format!("{clsid}-0000-0000-0000-000000000000"));
            assert!(matches!(
                request(base, CommandKind::Connect { endpoint }),
                EventKind::Connected { .. }
            ));
            let verdict = request(base + 1, CommandKind::Test);
            match expected {
                1 => assert!(matches!(
                    verdict,
                    EventKind::Log {
                        message: BackendMsg::DaStateOk
                    }
                )),
                2 => assert!(
                    matches!(verdict, EventKind::Error { operation: Operation::Test,
                    message: BackendMsg::DaNotReady { state: 2, ref name } } if name == "FAILED")
                ),
                3..=6 => assert!(
                    matches!(verdict, EventKind::Error { operation: Operation::Test,
                    message: BackendMsg::DaNotReady { state, .. } } if state == expected)
                ),
                42 | 0 => assert!(
                    matches!(verdict, EventKind::Error { operation: Operation::Test,
                    message: BackendMsg::DaUnknownState { state } } if state == expected)
                ),
                _ => assert!(
                    matches!(verdict, EventKind::Error { operation: Operation::Test,
                    message: BackendMsg::DaLinkLost { ref detail } } if detail.contains("80000005"))
                ),
            }
            assert!(matches!(
                request(base + 2, CommandKind::Disconnect),
                EventKind::Disconnected { .. }
            ));
        }
        drop(cmd);
        worker.join.take().unwrap().join().unwrap();
    }
    #[test]
    fn preview_contract_connect_add_read_write_remove() {
        #[cfg(feature = "native-da-test-hooks")]
        let before = test_host_acquisitions().0;
        let first_host = acquire_host().unwrap();
        let retained = first_host.clone();
        drop(first_host);
        let reused = acquire_host().unwrap();
        assert!(retained.same_host(&reused));
        #[cfg(feature = "native-da-test-hooks")]
        {
            let (count, all_same) = test_host_acquisitions();
            assert!(count >= before + 2 && all_same);
        }
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
        let recv = || loop {
            let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if matches!(event.origin, super::super::model::Origin::Reply(_)) {
                break event.kind;
            }
        };
        cmd.send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Discovery,
            request_id: Some(RequestId::FIRST),
            kind: CommandKind::Discover {
                host: "localhost".into(),
            },
        })
        .unwrap();
        assert!(
            matches!(recv(), EventKind::Servers { servers, .. } if servers.len() == 1 && servers[0].id == "OPCLook.NativeDaFixture.1")
        );
        send(CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        });
        assert!(matches!(recv(), EventKind::Connected { .. }));
        send(CommandKind::Browse {
            node_id: String::new(),
            filter: super::super::model::NodeClassFilter::All,
            view_id: None,
        });
        let branch = match recv() {
            EventKind::Browse { children, .. } => {
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].name, "Fixture");
                children[0].node_id.clone()
            }
            other => panic!("unexpected browse: {other:?}"),
        };
        send(CommandKind::Browse {
            node_id: branch,
            filter: super::super::model::NodeClassFilter::All,
            view_id: None,
        });
        assert!(
            matches!(recv(), EventKind::Browse { children, .. } if children.len() == 1 && children[0].node_id == "Fixture.Value")
        );
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
    #[test]
    fn subscription_discards_retired_registration_and_reports_overflow() {
        let guid = Guid {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0; 8],
        };
        let host = Host::new().unwrap();
        let shared = host.shared();
        let (mut session, _) = shared
            .open_config("server", "localhost", &guid, true, 100)
            .unwrap();
        let old = session.add("Fixture.Value").unwrap().0;
        session.inject_mock_push(old, 10, false);
        session.remove(old).unwrap();
        let fresh = session.add("Fixture.Value").unwrap().0;
        session.inject_mock_push(fresh, 20, true);
        let mut registrations = HashMap::new();
        registrations.insert(fresh, ItemKey::FIRST);
        let (tx, rx) = std::sync::mpsc::channel();
        let epoch = RequestId::new(7).unwrap();
        drain_push(
            &mut session,
            &registrations,
            Owner::Session(SessionId::FIRST),
            epoch,
            &tx,
        )
        .unwrap();
        assert!(matches!(rx.recv().unwrap().kind, EventKind::Log { .. }));
        let event = rx.recv().unwrap();
        assert_eq!(event.origin, super::super::model::Origin::Session(epoch));
        assert!(
            matches!(event.kind, EventKind::Values { values } if values.len() == 1 &&
            values[0].0 == ItemKey::FIRST && values[0].1.value == Value::I64(20))
        );
        assert!(rx.try_recv().is_err());
        session.close().unwrap();
        drop(shared);
        host.close().unwrap();
    }
}
