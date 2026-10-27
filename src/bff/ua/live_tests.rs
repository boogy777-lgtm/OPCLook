//! Live backend tests: the real UA worker against an in-process `async-opcua`
//! sample server. Ports 48600-48699 are reserved for this stream; every test
//! uses its own port so the tests can run in parallel.
//!
//! The worker is driven exactly as the application does: `Command`s in,
//! `Event`s out, matched by `request_id`.

use super::session::TEST_PKI_DIR;
use super::worker::spawn;
use crate::bff::WorkerHandle;
use crate::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemInfo, ItemKey, ItemSpec, Operation,
    Protocol, SessionId, UaSecurityMode, UaSecurityPolicy, Value,
};
use opcua::server::ServerBuilder;
use opcua::server::ServerHandle;
use opcua::server::address_space::{AccessLevel, VariableBuilder};
use opcua::server::diagnostics::NamespaceMetadata;
use opcua::server::node_manager::memory::{SimpleNodeManager, simple_node_manager};
use opcua::types::{DataTypeId, DataValue, NodeId, ObjectId, Variant};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

const NS_URI: &str = "urn:opclook:live-tests";

/// Shared scratch PKI store of the client side (keypair generated once, so
/// parallel tests never race on creating it).
fn init_pki() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("opclook-ua-client-pki-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = TEST_PKI_DIR.set(dir);
        // Building the discovery client creates the sample keypair.
        let _ = super::discovery::build_discovery_client();
    });
}

/// An in-process sample server with a small `Plant` folder:
/// `Level` (Double, 1.0), `Counter` (Int32, 0), `Flag` (Boolean), `Label` (String).
pub(super) struct TestServer {
    rt: tokio::runtime::Runtime,
    pub handle: ServerHandle,
    nm: Arc<SimpleNodeManager>,
    pub url: String,
    #[allow(dead_code)]
    pub port: u16,
    pub ns: u16,
    dir: PathBuf,
}

impl TestServer {
    pub fn start(port: u16) -> Self {
        init_pki();
        let dir =
            std::env::temp_dir().join(format!("opclook-ua-server-{}-{port}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let url = format!("opc.tcp://127.0.0.1:{port}/");
        // `build()` spawns background samplers: it needs the runtime context.
        let guard = rt.enter();
        let (server, handle) = ServerBuilder::new_sample()
            .host("127.0.0.1")
            .port(port)
            .pki_dir(&dir)
            .trust_client_certs(true)
            .discovery_urls(vec![url.clone()])
            .with_node_manager(simple_node_manager(
                NamespaceMetadata {
                    namespace_uri: NS_URI.to_string(),
                    ..Default::default()
                },
                "opclook-live-tests",
            ))
            .build()
            .expect("sample server builds");
        drop(guard);
        rt.spawn(server.run());

        let ns = handle
            .get_namespace_index(NS_URI)
            .expect("test namespace registered");
        let nm = handle
            .node_managers()
            .get_of_type::<SimpleNodeManager>()
            .expect("simple node manager");
        {
            let space = nm.address_space();
            let mut space = space.write();
            let folder = NodeId::new(ns, "Plant");
            space.add_folder(&folder, "Plant", "Plant", &ObjectId::ObjectsFolder.into());
            let rw = AccessLevel::CURRENT_READ | AccessLevel::CURRENT_WRITE;
            let var = |name: &str, ty: DataTypeId, value: Variant| {
                VariableBuilder::new(&NodeId::new(ns, name), name, name)
                    .data_type(ty)
                    .value(value)
                    .access_level(rw)
                    .user_access_level(rw)
                    .build()
            };
            space.add_variables(
                vec![
                    var("Level", DataTypeId::Double, Variant::Double(1.0)),
                    var("Counter", DataTypeId::Int32, Variant::Int32(0)),
                    var("Flag", DataTypeId::Boolean, Variant::Boolean(false)),
                    var("Label", DataTypeId::String, Variant::from("idle")),
                ],
                &folder,
            );
        }

        // Wait until the listener accepts connections.
        let deadline = Instant::now() + Duration::from_secs(10);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(Instant::now() < deadline, "test server did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
        Self {
            rt,
            handle,
            nm,
            url,
            port,
            ns,
            dir,
        }
    }

    /// NodeId text of a `Plant` variable, as the application writes it.
    pub fn node(&self, name: &str) -> String {
        format!("ns={};s={name}", self.ns)
    }

    /// Server-side value change (notifies subscriptions).
    pub fn set(&self, name: &str, value: Variant) {
        self.nm
            .set_value(
                self.handle.subscriptions(),
                &NodeId::new(self.ns, name),
                None,
                DataValue::new_now(value),
            )
            .expect("server value set");
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.cancel();
        let _ = &self.rt;
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The application side of the worker: sends commands, collects events.
pub(super) struct Harness {
    worker: WorkerHandle,
    events: Receiver<Event>,
    seen: Vec<(Event, bool)>,
    next_request: u64,
}

impl Harness {
    pub fn new() -> Self {
        init_pki();
        let (tx, events) = channel();
        Self {
            worker: spawn(tx),
            events,
            seen: Vec::new(),
            next_request: 0,
        }
    }

    pub fn send(&mut self, session: SessionId, kind: CommandKind) -> u64 {
        self.next_request += 1;
        let request_id = self.next_request;
        let command = Command {
            request_id,
            session,
            protocol: Protocol::OpcUa,
            kind,
        };
        self.worker
            .tx
            .as_ref()
            .expect("worker is running")
            .send(command)
            .expect("worker accepts commands");
        request_id
    }

    /// Waits for the first not yet consumed event accepted by `pick`.
    pub fn wait<T>(
        &mut self,
        timeout: Duration,
        mut pick: impl FnMut(&Event) -> Option<T>,
    ) -> Option<T> {
        let deadline = Instant::now() + timeout;
        let mut index = 0;
        loop {
            while index < self.seen.len() {
                if !self.seen[index].1 {
                    if let Some(found) = pick(&self.seen[index].0) {
                        self.seen[index].1 = true;
                        return Some(found);
                    }
                }
                index += 1;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match self.events.recv_timeout(remaining) {
                Ok(event) => self.seen.push((event, false)),
                Err(_) => return None,
            }
        }
    }

    /// The reply (`request_id`-correlated) to a command.
    pub fn reply(&mut self, request_id: u64, timeout: Duration) -> Option<Event> {
        self.wait(timeout, |event| {
            (event.request_id == request_id).then(|| event.clone())
        })
    }

    pub fn endpoint(url: &str) -> EndpointSpec {
        let mut endpoint = EndpointSpec::ua(url);
        endpoint.ua_security_policy = Some(UaSecurityPolicy::None);
        endpoint.ua_security_mode = Some(UaSecurityMode::None);
        endpoint
    }

    /// Connects and waits for `Connected`; returns the session info line.
    pub fn connect(
        &mut self,
        session: SessionId,
        endpoint: EndpointSpec,
    ) -> Result<String, String> {
        let id = self.send(session, CommandKind::Connect { endpoint });
        match self.reply(id, Duration::from_secs(60)) {
            Some(Event {
                kind: EventKind::Connected { info, .. },
                ..
            }) => Ok(info),
            Some(Event {
                kind: EventKind::Error { message, .. },
                ..
            }) => Err(message),
            other => Err(format!("unexpected connect reply: {other:?}")),
        }
    }

    pub fn add_items(
        &mut self,
        session: SessionId,
        items: &[(ItemKey, String)],
    ) -> Vec<(ItemKey, Result<ItemInfo, String>)> {
        let id = self.send(
            session,
            CommandKind::AddItems {
                items: items
                    .iter()
                    .map(|(key, item_id)| ItemSpec {
                        key: *key,
                        item_id: item_id.clone(),
                    })
                    .collect(),
            },
        );
        match self.reply(id, Duration::from_secs(30)) {
            Some(Event {
                kind: EventKind::AddItems { results },
                ..
            }) => results,
            other => panic!("unexpected AddItems reply: {other:?}"),
        }
    }

    /// Waits for a pushed value of `key` that satisfies `accept`.
    pub fn value_of(
        &mut self,
        key: ItemKey,
        timeout: Duration,
        accept: impl Fn(&Value) -> bool,
    ) -> Option<Value> {
        self.wait(timeout, |event| match &event.kind {
            EventKind::Values { values } => values
                .iter()
                .find(|(k, v)| *k == key && accept(&v.value))
                .map(|(_, v)| v.value.clone()),
            _ => None,
        })
    }

    #[allow(dead_code)]
    pub fn disconnect(&mut self, session: SessionId) {
        let _ = self.send(session, CommandKind::Disconnect);
        let _ = self.wait(Duration::from_secs(20), |event| {
            (event.session == session && matches!(event.kind, EventKind::Disconnected { .. }))
                .then_some(())
        });
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.worker.shutdown(Duration::from_secs(20));
    }
}

// ---------------------------------------------------------------------------
// U2: per-item status of CreateMonitoredItems
// ---------------------------------------------------------------------------

#[test]
fn u2_unknown_node_is_reported_as_error_and_valid_node_delivers_values() {
    let server = TestServer::start(48601);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");

    let results = app.add_items(
        1,
        &[
            (1, server.node("Level")),
            (2, format!("ns={};s=NoSuchTag", server.ns)),
        ],
    );
    assert_eq!(results.len(), 2);
    assert!(
        results.iter().any(|(k, r)| *k == 1 && r.is_ok()),
        "valid node must be Ok: {results:?}"
    );
    let bad = results
        .iter()
        .find(|(k, _)| *k == 2)
        .and_then(|(_, r)| r.as_ref().err())
        .expect("unknown node must be an Err");
    assert!(
        bad.contains("BadNodeIdUnknown"),
        "message must carry the server status, got {bad:?}"
    );

    server.set("Level", Variant::Double(42.5));
    let value = app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(42.5));
    assert!(value.is_some(), "valid item must push its value");
}

#[test]
fn u2_malformed_node_id_is_an_error_not_a_silent_ok() {
    let server = TestServer::start(48602);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    let results = app.add_items(1, &[(1, "это не NodeId".to_string())]);
    assert!(
        matches!(results.first(), Some((1, Err(_)))),
        "malformed NodeId must be rejected: {results:?}"
    );
}

#[test]
fn u2_no_monitored_item_means_no_active_subscription() {
    use crate::bff::model::SubscriptionState;
    let server = TestServer::start(48603);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    let results = app.add_items(1, &[(1, format!("ns={};s=Nope", server.ns))]);
    assert!(matches!(results.first(), Some((1, Err(_)))));
    let active = app.wait(Duration::from_secs(1), |event| match event.kind {
        EventKind::SubscriptionState {
            state: SubscriptionState::Active,
        } => Some(()),
        _ => None,
    });
    assert!(
        active.is_none(),
        "nothing is monitored, so the app must keep polling"
    );
}

#[test]
fn u2_removing_one_item_keeps_the_other_subscribed() {
    // Items are deleted by the server's monitored item id, not by the client
    // handle: deleting the wrong ids would silence the item that stays.
    let server = TestServer::start(48604);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    // Keys far from the server's own id sequence (1, 2, ...).
    let results = app.add_items(
        1,
        &[(501, server.node("Level")), (502, server.node("Counter"))],
    );
    assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}");

    let removed = app.send(1, CommandKind::RemoveItems { keys: vec![501] });
    assert!(matches!(
        app.reply(removed, PROMPT_LONG).expect("removed").kind,
        EventKind::Removed { .. }
    ));

    server.set("Level", Variant::Double(77.0));
    server.set("Counter", Variant::Int32(5));
    assert!(
        app.value_of(502, Duration::from_secs(10), |v| *v == Value::I64(5))
            .is_some(),
        "the remaining item must keep pushing"
    );
    assert!(
        app.value_of(501, Duration::from_secs(2), |v| *v == Value::F64(77.0))
            .is_none(),
        "the removed item must stop pushing"
    );
}

const PROMPT_LONG: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// U1: sessions must not block each other
// ---------------------------------------------------------------------------

/// A TCP listener that accepts connections and never answers: a host that is up
/// at the TCP level but never completes the OPC UA handshake. `Connect` to it
/// hangs for as long as the client's own timeouts allow.
struct SilentServer {
    port: u16,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SilentServer {
    fn start(port: u16) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind stub");
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut held = Vec::new();
            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => held.push(stream),
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        });
        Self {
            port,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("opc.tcp://127.0.0.1:{}/", self.port)
    }
}

impl Drop for SilentServer {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Generous bound for an operation that takes milliseconds when nothing blocks
/// it and 10+ seconds behind a hung connect (the client's discovery timeout).
const PROMPT: Duration = Duration::from_secs(4);

#[test]
fn u1_hung_connect_does_not_block_other_sessions() {
    let server = TestServer::start(48611);
    let silent = SilentServer::start(48612);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect to the live server");
    let added = app.add_items(1, &[(1, server.node("Level"))]);
    assert!(matches!(added.first(), Some((1, Ok(_)))));

    // Session 2 connects to a host that never answers.
    let hung = app.send(
        2,
        CommandKind::Connect {
            endpoint: Harness::endpoint(&silent.url()),
        },
    );
    std::thread::sleep(Duration::from_millis(500));

    let started = Instant::now();
    let browse = app.send(
        1,
        CommandKind::Browse {
            node_id: "i=85".to_string(),
            filter: None,
            view_id: None,
        },
    );
    let reply = app.reply(browse, PROMPT);
    assert!(
        matches!(
            reply,
            Some(Event {
                kind: EventKind::Browse { .. },
                ..
            })
        ),
        "Browse of a live session must answer while another Connect hangs: {reply:?} after {:?}",
        started.elapsed()
    );

    let read = app.send(1, CommandKind::Read { keys: vec![1] });
    let reply = app.reply(read, PROMPT);
    assert!(
        matches!(
            reply,
            Some(Event {
                kind: EventKind::Values { .. },
                ..
            })
        ),
        "Read of a live session must answer while another Connect hangs: {reply:?}"
    );

    // A third session can connect to a healthy server in the meantime.
    app.connect(3, Harness::endpoint(&server.url))
        .expect("a second healthy connect must not wait for the hung one");

    // The hung connect has not produced a reply yet (it is still running).
    assert!(
        app.reply(hung, Duration::from_millis(50)).is_none(),
        "the silent host cannot have answered"
    );
}

#[test]
fn u1_disconnect_cancels_a_pending_connect_with_one_error_reply() {
    let silent = SilentServer::start(48613);
    let mut app = Harness::new();
    let connect = app.send(
        7,
        CommandKind::Connect {
            endpoint: Harness::endpoint(&silent.url()),
        },
    );
    std::thread::sleep(Duration::from_millis(300));
    let _ = app.send(7, CommandKind::Disconnect);

    let reply = app
        .reply(connect, PROMPT)
        .expect("Connect must be answered");
    assert!(
        matches!(
            reply.kind,
            EventKind::Error {
                operation: Operation::Connect,
                ..
            }
        ),
        "a cancelled connect is reported as a Connect error: {reply:?}"
    );
    assert!(
        app.reply(connect, Duration::from_millis(300)).is_none(),
        "exactly one reply per command"
    );
}

#[test]
fn u1_commands_for_a_session_without_connection_get_exactly_one_error_each() {
    let mut app = Harness::new();
    let browse = app.send(
        5,
        CommandKind::Browse {
            node_id: String::new(),
            filter: None,
            view_id: None,
        },
    );
    let read = app.send(5, CommandKind::Read { keys: vec![1, 2] });
    let write = app.send(
        5,
        CommandKind::Write {
            request: crate::bff::model::WriteRequest {
                key: 1,
                value: Value::I64(1),
                type_hint: None,
            },
        },
    );
    let add = app.send(5, CommandKind::AddItems { items: Vec::new() });
    let test = app.send(5, CommandKind::Test);

    let browse_reply = app.reply(browse, PROMPT).expect("browse");
    assert!(matches!(
        browse_reply.kind,
        EventKind::Error {
            operation: Operation::Browse,
            ..
        }
    ));
    let read_reply = app.reply(read, PROMPT).expect("read");
    match read_reply.kind {
        EventKind::Values { values } => {
            assert_eq!(values.len(), 2);
            assert!(values.iter().all(|(_, v)| v.quality.code == 0));
        }
        other => panic!("Read of a missing session answers with Bad values: {other:?}"),
    }
    assert!(matches!(
        app.reply(write, PROMPT).expect("write").kind,
        EventKind::Error {
            operation: Operation::Write,
            ..
        }
    ));
    assert!(matches!(
        app.reply(add, PROMPT).expect("add").kind,
        EventKind::Error {
            operation: Operation::Add,
            ..
        }
    ));
    assert!(matches!(
        app.reply(test, PROMPT).expect("test").kind,
        EventKind::Error {
            operation: Operation::Test,
            ..
        }
    ));
    for id in [browse, read, write, add, test] {
        assert!(
            app.reply(id, Duration::from_millis(100)).is_none(),
            "request {id} answered twice"
        );
    }
}

#[test]
fn u1_disconnect_is_reported_before_the_next_connect_of_the_same_session() {
    // The application reconnects with `Disconnect` immediately followed by
    // `Connect`. Its state machine relies on `Disconnected` coming first.
    let server = TestServer::start(48614);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("first connect");

    let disconnect = app.send(1, CommandKind::Disconnect);
    let connect = app.send(
        1,
        CommandKind::Connect {
            endpoint: Harness::endpoint(&server.url),
        },
    );
    let connected_at = |app: &Harness| {
        app.seen.iter().position(|(e, _)| {
            matches!(e.kind, EventKind::Connected { .. }) && e.request_id == connect
        })
    };
    let reply = app
        .reply(connect, Duration::from_secs(60))
        .expect("reconnect");
    assert!(
        matches!(reply.kind, EventKind::Connected { .. }),
        "{reply:?}"
    );
    let disconnected_at = app
        .seen
        .iter()
        .position(|(e, _)| {
            matches!(e.kind, EventKind::Disconnected { .. }) && e.request_id == disconnect
        })
        .expect("Disconnected was reported");
    let connected_at = connected_at(&app).expect("Connected was reported");
    assert!(
        disconnected_at < connected_at,
        "Disconnected ({disconnected_at}) must precede Connected ({connected_at})"
    );
}

#[test]
fn u1_commands_sent_while_connecting_are_delivered_after_the_connect() {
    let server = TestServer::start(48615);
    let mut app = Harness::new();
    let connect = app.send(
        1,
        CommandKind::Connect {
            endpoint: Harness::endpoint(&server.url),
        },
    );
    // Sent before the session is ready: must wait, not fail with "no connection".
    let browse = app.send(
        1,
        CommandKind::Browse {
            node_id: "i=85".to_string(),
            filter: None,
            view_id: None,
        },
    );
    assert!(matches!(
        app.reply(connect, Duration::from_secs(60))
            .expect("connect")
            .kind,
        EventKind::Connected { .. }
    ));
    let reply = app.reply(browse, PROMPT).expect("browse reply");
    assert!(
        matches!(reply.kind, EventKind::Browse { .. }),
        "a command queued during Connect is served by the new session: {reply:?}"
    );
}

#[test]
fn u1_a_failed_connect_answers_the_commands_queued_behind_it() {
    let mut app = Harness::new();
    // Nothing listens on this port: the connect fails.
    let connect = app.send(
        1,
        CommandKind::Connect {
            endpoint: Harness::endpoint("opc.tcp://127.0.0.1:48619/"),
        },
    );
    let browse = app.send(
        1,
        CommandKind::Browse {
            node_id: String::new(),
            filter: None,
            view_id: None,
        },
    );
    assert!(matches!(
        app.reply(connect, Duration::from_secs(60))
            .expect("connect")
            .kind,
        EventKind::Error {
            operation: Operation::Connect,
            ..
        }
    ));
    assert!(matches!(
        app.reply(browse, PROMPT).expect("browse").kind,
        EventKind::Error {
            operation: Operation::Browse,
            ..
        }
    ));
}

#[test]
fn u1_shutdown_closes_live_sessions_and_pending_connects_promptly() {
    let server = TestServer::start(48616);
    let silent = SilentServer::start(48617);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    app.connect(2, Harness::endpoint(&server.url))
        .expect("connect");
    let _ = app.send(
        3,
        CommandKind::Connect {
            endpoint: Harness::endpoint(&silent.url()),
        },
    );
    std::thread::sleep(Duration::from_millis(300));

    let started = Instant::now();
    assert!(
        app.worker.shutdown(Duration::from_secs(10)),
        "worker must finish within the shutdown budget"
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "shutdown took {:?}",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// U4: ConfigureUaSubscription reaches the live subscription
// ---------------------------------------------------------------------------

#[test]
fn u4_deadband_change_is_applied_to_a_live_subscription() {
    let server = TestServer::start(48621);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    let added = app.add_items(1, &[(1, server.node("Level"))]);
    assert!(matches!(added.first(), Some((1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(1.0))
            .is_some(),
        "the initial value is pushed"
    );

    // The subscription is already running: a new deadband must reach it.
    let _ = app.send(
        1,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 5.0,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    server.set("Level", Variant::Double(2.0));
    assert!(
        app.value_of(1, Duration::from_secs(2), |v| *v == Value::F64(2.0))
            .is_none(),
        "a change inside the deadband must be filtered out"
    );
    server.set("Level", Variant::Double(10.0));
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(10.0))
            .is_some(),
        "a change beyond the deadband is delivered"
    );

    // And switching the deadband off again also reaches the live items.
    let _ = app.send(
        1,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 0.0,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    server.set("Level", Variant::Double(10.5));
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(10.5))
            .is_some(),
        "without a deadband every change is delivered"
    );
}

#[test]
fn u4_publishing_interval_change_is_applied_to_a_live_subscription() {
    let server = TestServer::start(48622);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    let added = app.add_items(1, &[(1, server.node("Level"))]);
    assert!(matches!(added.first(), Some((1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(1.0))
            .is_some()
    );

    // An hour between publishes: a change must not arrive any time soon.
    let _ = app.send(
        1,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 3_600_000,
            deadband: 0.0,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    server.set("Level", Variant::Double(3.0));
    assert!(
        app.value_of(1, Duration::from_secs(3), |v| *v == Value::F64(3.0))
            .is_none(),
        "the subscription still publishes at the old, fast interval"
    );

    // Back to a fast interval: the pending change is delivered.
    let _ = app.send(
        1,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 0.0,
        },
    );
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(3.0))
            .is_some(),
        "the restored interval publishes the change"
    );
}

#[test]
fn u4_configure_before_the_first_item_is_used_when_the_subscription_is_created() {
    let server = TestServer::start(48623);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");
    let _ = app.send(
        1,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 5.0,
        },
    );
    let added = app.add_items(1, &[(1, server.node("Level"))]);
    assert!(matches!(added.first(), Some((1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(1.0))
            .is_some()
    );
    server.set("Level", Variant::Double(2.0));
    assert!(
        app.value_of(1, Duration::from_secs(2), |v| *v == Value::F64(2.0))
            .is_none(),
        "the stored deadband applies to items created later"
    );
}

// ---------------------------------------------------------------------------
// U5: the "variables only" browse filter keeps folders
// ---------------------------------------------------------------------------

#[test]
fn u5_variables_filter_still_lists_folders_so_the_tree_can_be_walked() {
    use crate::bff::model::NodeKind;
    let server = TestServer::start(48631);
    let mut app = Harness::new();
    app.connect(1, Harness::endpoint(&server.url))
        .expect("connect");

    let browse = |app: &mut Harness, node: &str| {
        let id = app.send(
            1,
            CommandKind::Browse {
                node_id: node.to_string(),
                filter: Some(2),
                view_id: None,
            },
        );
        match app.reply(id, Duration::from_secs(20)) {
            Some(Event {
                kind: EventKind::Browse { children, .. },
                ..
            }) => children,
            other => panic!("unexpected Browse reply: {other:?}"),
        }
    };
    let top = browse(&mut app, "i=85");
    let plant = top
        .iter()
        .find(|node| node.node_id == server.node("Plant"))
        .unwrap_or_else(|| panic!("the Plant folder must stay visible: {top:?}"));
    assert_eq!(plant.kind, NodeKind::Branch);

    let inside = browse(&mut app, &plant.node_id.clone());
    assert!(
        inside
            .iter()
            .any(|node| node.node_id == server.node("Level") && node.kind == NodeKind::Item),
        "variables inside the folder are listed: {inside:?}"
    );
}

// ---------------------------------------------------------------------------
// U8: Aes128_Sha256_RsaOaep / Aes256_Sha256_RsaPss
// ---------------------------------------------------------------------------

fn secure_connect_and_read(port: u16, policy: UaSecurityPolicy) {
    let server = TestServer::start(port);
    let mut app = Harness::new();
    let mut endpoint = Harness::endpoint(&server.url);
    endpoint.ua_security_policy = Some(policy);
    endpoint.ua_security_mode = Some(UaSecurityMode::SignAndEncrypt);
    endpoint.ua_trust_server_cert = true;
    let info = app
        .connect(1, endpoint)
        .unwrap_or_else(|e| panic!("{policy} / SignAndEncrypt must connect: {e}"));
    assert!(
        info.contains(policy.as_str()),
        "the session info names the policy: {info:?}"
    );
    let added = app.add_items(1, &[(1, server.node("Level"))]);
    assert!(matches!(added.first(), Some((1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(1, Duration::from_secs(10), |v| *v == Value::F64(1.0))
            .is_some(),
        "values flow over the {policy} channel"
    );
}

#[test]
fn u8_aes256_sha256_rsapss_sign_and_encrypt_connects() {
    secure_connect_and_read(48641, UaSecurityPolicy::Aes256Sha256RsaPss);
}

#[test]
fn u8_aes128_sha256_rsaoaep_sign_and_encrypt_connects() {
    secure_connect_and_read(48642, UaSecurityPolicy::Aes128Sha256RsaOaep);
}

// ---------------------------------------------------------------------------
// U12: the session info says how the channel is protected
// ---------------------------------------------------------------------------

#[test]
fn u12_auto_connect_reports_the_unencrypted_channel_and_the_secure_alternative() {
    let server = TestServer::start(48651);
    let mut app = Harness::new();
    // Auto/Auto, as a user who never touched the security fields.
    let info = app
        .connect(1, EndpointSpec::ua(&server.url))
        .expect("connect");
    assert!(info.contains("без шифрования"), "{info:?}");
    assert!(
        info.contains("предлагает защищённое"),
        "the sample server also offers secure endpoints: {info:?}"
    );
}
