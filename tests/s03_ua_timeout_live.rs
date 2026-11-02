//! S03: real Engine/Gateway with an ordinary held UA Read on a sample server.
use opclook_contract::model::{
    AcquisitionConfig, BackendMsg, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec,
    Origin, PollFreshness, Protocol, ReadCancelReason, ReadNeed, ReadTerminal, ReadTerminalStatus,
    SessionId, UaSecurityMode, UaSecurityPolicy, UpdateMode, Value, WriteRequest,
};
use opclook_engine::{Due, Engine};
use opcua::server::ServerBuilder;
use opcua::server::address_space::{AccessLevel, VariableBuilder};
use opcua::server::diagnostics::NamespaceMetadata;
use opcua::server::node_manager::memory::{SimpleNodeManager, simple_node_manager};
use opcua::types::{DataTypeId, NodeId, ObjectId, Variant};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Server {
    rt: tokio::runtime::Runtime,
    handle: opcua::server::ServerHandle,
    ns: u16,
    port: u16,
    url: String,
    dir: std::path::PathBuf,
}
impl Server {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let dir =
            std::env::temp_dir().join(format!("opclook-capacity-{}-{port}", std::process::id()));
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let guard = rt.enter();
        let url = format!("opc.tcp://127.0.0.1:{port}/");
        let mut builder = ServerBuilder::new_sample();
        builder
            .limits_mut()
            .subscriptions
            .max_monitored_items_per_sub = 1;
        builder
            .limits_mut()
            .subscriptions
            .max_subscriptions_per_session = 1;
        let (server, handle) = builder
            .host("127.0.0.1")
            .port(port)
            .pki_dir(&dir)
            .trust_client_certs(true)
            .discovery_urls(vec![url.clone()])
            .with_node_manager(simple_node_manager(
                NamespaceMetadata {
                    namespace_uri: "urn:opclook:m09".into(),
                    ..Default::default()
                },
                "m09",
            ))
            .build()
            .unwrap();
        drop(guard);
        let ns = handle.get_namespace_index("urn:opclook:m09").unwrap();
        let nm = handle
            .node_managers()
            .get_of_type::<SimpleNodeManager>()
            .unwrap();
        {
            let space = nm.address_space();
            let mut space = space.write();
            let folder = NodeId::new(ns, "Plant");
            space.add_folder(&folder, "Plant", "Plant", &ObjectId::ObjectsFolder.into());
            let access = AccessLevel::CURRENT_READ | AccessLevel::CURRENT_WRITE;
            let vars = ["A", "B"]
                .into_iter()
                .map(|name| {
                    VariableBuilder::new(&NodeId::new(ns, name), name, name)
                        .data_type(DataTypeId::Int64)
                        .value(Variant::Int64(1))
                        .access_level(access)
                        .user_access_level(access)
                        .build()
                })
                .collect();
            space.add_variables(vars, &folder);
        }
        let running = rt.spawn(server.run());
        let deadline = Instant::now() + Duration::from_secs(90);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                !running.is_finished() && Instant::now() < deadline,
                "sample server did not start"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        Self {
            rt,
            handle,
            ns,
            port,
            url,
            dir,
        }
    }
    fn node(&self, name: &str) -> String {
        format!("ns={};s={name}", self.ns)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.handle.cancel();
        self.rt
            .block_on(async { tokio::time::sleep(Duration::from_millis(50)).await });
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct HeldBrowseProxy {
    url: String,
    armed: Arc<std::sync::atomic::AtomicBool>,
    hold_read: Arc<std::sync::atomic::AtomicBool>,
    read_target: Arc<std::sync::Mutex<Option<NodeId>>>,
    held: tokio::sync::mpsc::UnboundedReceiver<()>,
    release: Arc<tokio::sync::Notify>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl HeldBrowseProxy {
    fn start(server_port: u16) -> Self {
        use std::sync::atomic::AtomicBool;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let armed = Arc::new(AtomicBool::new(false));
        let hold_read = Arc::new(AtomicBool::new(false));
        let read_target = Arc::new(std::sync::Mutex::new(None::<NodeId>));
        let release = Arc::new(tokio::sync::Notify::new());
        let (held_tx, held) = tokio::sync::mpsc::unbounded_channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let proxy_armed = armed.clone();
        let proxy_hold_read = hold_read.clone();
        let proxy_read_target = read_target.clone();
        let proxy_release = release.clone();
        let thread = std::thread::spawn(move || {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let mut tasks = tokio::task::JoinSet::new();
                tokio::pin!(stopped);
                loop {
                    tokio::select! {
                        _ = &mut stopped => break,
                        accepted = listener.accept() => {
                            let Ok((client, _)) = accepted else { break };
                            let armed = proxy_armed.clone();
                            let hold_read = proxy_hold_read.clone();
                            let read_target = proxy_read_target.clone();
                            let release = proxy_release.clone();
                            let held_tx = held_tx.clone();
                            tasks.spawn(async move {
                                let Ok(server) = tokio::net::TcpStream::connect(("127.0.0.1", server_port)).await else { return };
                                let (mut cr, mut cw) = client.into_split();
                                let (mut sr, mut sw) = server.into_split();
                                let upstream = async move {
                                    loop {
                                        let mut header = [0u8; 8];
                                        if cr.read_exact(&mut header).await.is_err() { break; }
                                        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                                        if !(8..=1_048_576).contains(&len) { break; }
                                        let mut frame = vec![0; len]; frame[..8].copy_from_slice(&header);
                                        if cr.read_exact(&mut frame[8..]).await.is_err() { break; }
                                        if &header[..4] == b"MSGF" && len > 24 {
                                            use opcua::types::{BinaryDecodable, ContextOwned, ReadRequest};
                                            let context = ContextOwned::default();
                                            let mut body = std::io::Cursor::new(&frame[24..]);
                                            let is_read = hold_read.load(std::sync::atomic::Ordering::SeqCst);
                                            let id = NodeId::decode(&mut body, &context.context());
                                            let target = read_target.lock().unwrap().clone();
                                            let matching = id.is_ok_and(|id| id == NodeId::from(if is_read { ObjectId::ReadRequest_Encoding_DefaultBinary } else { ObjectId::BrowseRequest_Encoding_DefaultBinary }))
                                                && (!is_read || target.is_none() || ReadRequest::decode(&mut body, &context.context()).is_ok_and(|request| request.nodes_to_read.as_ref().is_some_and(|nodes| nodes.iter().any(|n| Some(&n.node_id) == target.as_ref()))));
                                            if matching && armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                                                let _ = held_tx.send(());
                                                release.notified().await;
                                            }
                                        }
                                        if sw.write_all(&frame).await.is_err() { break; }
                                    }
                                };
                                let downstream = async move { let _ = tokio::io::copy(&mut sr, &mut cw).await; };
                                tokio::select! { _ = upstream => {}, _ = downstream => {} }
                            });
                        }
                    }
                }
                tasks.abort_all(); while tasks.join_next().await.is_some() {}
            });
        });
        Self {
            url: format!("opc.tcp://127.0.0.1:{port}/"),
            armed,
            hold_read,
            read_target,
            held,
            release,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}
impl Drop for HeldBrowseProxy {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn wait_event(
    engine: &mut Engine<()>,
    timeout: Duration,
    predicate: impl Fn(&Event) -> bool,
) -> Event {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(event) = engine.try_recv() {
            let done = predicate(&event);
            let accepted = engine.accept(&event);
            if done {
                assert!(accepted.is_ok(), "matching event rejected: {event:?}");
                return event;
            }
        } else {
            assert!(Instant::now() < deadline, "event wait timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn engine_gateway_timeout_quarantines_pending_readback_and_reconnects() {
    use std::sync::atomic::Ordering;
    opclook_engine::testing::use_private_pki_dir();
    let server = Server::start();
    let mut proxy = HeldBrowseProxy::start(server.port);
    let mut engine = Engine::<()>::default();
    let terminals = Arc::new(Mutex::new(Vec::<ReadTerminal>::new()));
    let sink = terminals.clone();
    engine.set_read_terminal_sink(Some(Box::new(move |_, _, terminal| {
        sink.lock().unwrap().push(terminal.clone())
    })));
    let sid = SessionId::FIRST;
    let key = ItemKey::FIRST;
    let mut endpoint = EndpointSpec::ua(&proxy.url);
    endpoint.ua_security_mode = UaSecurityMode::None;
    endpoint.ua_security_policy = UaSecurityPolicy::None;
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Poll;
    profile.poll_freshness = PollFreshness::CacheAllowed;
    profile.interval_ms = 60_000;
    profile.read_after_write = true;
    endpoint.acquisition = Some(profile);
    let connect = engine
        .send(
            sid,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
            (),
        )
        .unwrap();
    assert!(matches!(
        wait_event(&mut engine, Duration::from_secs(45), |e| e.origin
            == Origin::Reply(connect))
        .kind,
        EventKind::Connected { .. }
    ));
    let add = engine
        .send(
            sid,
            Protocol::OpcUa,
            CommandKind::AddItems {
                items: vec![ItemSpec {
                    key,
                    item_id: server.node("A"),
                }],
            },
            (),
        )
        .unwrap();
    assert!(
        matches!(wait_event(&mut engine, Duration::from_secs(45), |e| e.origin == Origin::Reply(add)).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    // Prime OperationLimits and clear Initial before arming the held Read.
    let prime = engine
        .send(
            sid,
            Protocol::OpcUa,
            CommandKind::Read { keys: vec![key] },
            (),
        )
        .unwrap();
    assert!(
        matches!(wait_event(&mut engine, Duration::from_secs(45), |e| e.origin == Origin::Reply(prime)).kind, EventKind::Values { values } if values.len() == 1 && values[0].1.reason.is_none())
    );
    terminals.lock().unwrap().clear();
    proxy.hold_read.store(true, Ordering::SeqCst);
    *proxy.read_target.lock().unwrap() = Some(NodeId::new(server.ns, "A"));
    proxy.armed.store(true, Ordering::SeqCst);
    // Worker executes this Write before Read, while Engine consumes its reply only once Read is held.
    let write = engine
        .send(
            sid,
            Protocol::OpcUa,
            CommandKind::Write {
                request: WriteRequest {
                    key,
                    value: Value::I64(42),
                    type_hint: None,
                },
            },
            (),
        )
        .unwrap();
    engine.request_refresh(sid, key).unwrap();
    let due = engine.due(Instant::now(), Duration::from_secs(60), None);
    let [
        Due::Read {
            session,
            protocol,
            keys,
            permit,
        },
    ] = due.as_slice()
    else {
        panic!("expected R1");
    };
    let read = engine
        .send_due_read(*session, *protocol, keys.clone(), *permit, ())
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), proxy.held.recv())
            .await
            .unwrap()
            .unwrap();
    });
    assert!(
        matches!(wait_event(&mut engine, Duration::from_secs(20), |e| e.origin == Origin::Reply(write)).kind, EventKind::Wrote { result: Ok(ref result), .. } if result.accepted())
    );
    assert!(
        engine
            .due(Instant::now(), Duration::from_secs(60), None)
            .is_empty()
    );
    let timed = wait_event(&mut engine, Duration::from_secs(45), |e| {
        e.origin == Origin::Reply(read)
    });
    assert!(
        matches!(timed.kind, EventKind::Values { values } if values.len() == 1 && matches!(&values[0].1.reason, Some(BackendMsg::Context { cause, .. }) if matches!(cause.as_ref(), BackendMsg::TimeoutRead)))
    );
    // Do not consume the following Disconnected until the scheduler is checked.
    assert!(
        engine
            .due(Instant::now(), Duration::from_secs(60), None)
            .is_empty(),
        "R2 issued while timed-out UA session closes"
    );
    wait_event(&mut engine, Duration::from_secs(30), |e| {
        matches!(
            e.kind,
            EventKind::Disconnected {
                reason: BackendMsg::SessionClosedAfterTimeout
            }
        )
    });
    let seen = terminals.lock().unwrap();
    assert_eq!(seen.len(), 2, "one terminal for each demand");
    assert_eq!(seen.iter().filter(|t| t.key == key && t.request_id == Some(read) && t.needs.contains(&ReadNeed::ExplicitRefresh) && matches!(&t.status, ReadTerminalStatus::Received(v) if matches!(&v.reason, Some(BackendMsg::Context { cause, .. }) if matches!(cause.as_ref(), BackendMsg::TimeoutRead)))).count(), 1);
    assert_eq!(
        seen.iter()
            .filter(|t| t.key == key
                && t.request_id.is_none()
                && t.needs.contains(&ReadNeed::WriteReadback)
                && matches!(
                    t.status,
                    ReadTerminalStatus::Cancelled(ReadCancelReason::Disconnected)
                ))
            .count(),
        1
    );
    drop(seen);
    proxy.release.notify_one();
    endpoint.address = server.url.clone();
    let connect = engine
        .send(sid, Protocol::OpcUa, CommandKind::Connect { endpoint }, ())
        .unwrap();
    assert!(matches!(
        wait_event(&mut engine, Duration::from_secs(45), |e| e.origin
            == Origin::Reply(connect))
        .kind,
        EventKind::Connected { .. }
    ));
    let add = engine
        .send(
            sid,
            Protocol::OpcUa,
            CommandKind::AddItems {
                items: vec![ItemSpec {
                    key,
                    item_id: server.node("A"),
                }],
            },
            (),
        )
        .unwrap();
    assert!(
        matches!(wait_event(&mut engine, Duration::from_secs(45), |e| e.origin == Origin::Reply(add)).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    let due = engine.due(
        Instant::now() + Duration::from_secs(65),
        Duration::from_secs(60),
        None,
    );
    let [
        Due::Read {
            session,
            protocol,
            keys,
            permit,
        },
    ] = due.as_slice()
    else {
        panic!("expected new registration snapshot")
    };
    let snapshot = engine
        .send_due_read(*session, *protocol, keys.clone(), *permit, ())
        .unwrap();
    assert!(
        matches!(wait_event(&mut engine, Duration::from_secs(45), |e| e.origin == Origin::Reply(snapshot)).kind, EventKind::Values { values } if values.len() == 1 && values[0].1.reason.is_none())
    );
    assert!(
        terminals
            .lock()
            .unwrap()
            .iter()
            .any(|t| t.request_id == Some(snapshot)
                && t.needs.contains(&ReadNeed::Initial)
                && matches!(&t.status, ReadTerminalStatus::Received(v) if v.reason.is_none()))
    );
    let shutdown = engine.shutdown();
    assert_eq!(shutdown.ua, opclook_backend::bff::ShutdownOutcome::Joined);
    assert_eq!(shutdown.da, opclook_backend::bff::ShutdownOutcome::Joined);
}
