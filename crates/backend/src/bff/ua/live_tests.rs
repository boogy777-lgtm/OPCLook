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
    BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, ItemInfo, ItemKey, ItemSpec,
    Operation, Protocol, SessionId, UaSecurityMode, UaSecurityPolicy, Value,
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
        Self::start_limited(port, |_| {})
    }

    /// Like [`Self::start`] with server limits adjusted by `limits`.
    pub fn start_limited(port: u16, limits: impl Fn(&mut opcua::server::Limits)) -> Self {
        // The fixed test ports lie in Linux's ephemeral range (32768-60999):
        // an outgoing connection of a parallel test may already hold the port
        // as its local end, and the server's bind then fails. Such a start is
        // retried on a port the OS picks as free; tests read `server.port`.
        let mut port = port;
        for _ in 0..4 {
            match Self::try_start(port, &limits) {
                Ok(server) => return server,
                Err(taken) => {
                    eprintln!("test server port {taken} taken, retrying on a free port");
                    port = free_port();
                }
            }
        }
        panic!("test server could not start on any port");
    }

    /// One start attempt; `Err(port)` when the server task ended before it
    /// listened (the port is taken).
    fn try_start(port: u16, limits: &impl Fn(&mut opcua::server::Limits)) -> Result<Self, u16> {
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
        let mut builder = ServerBuilder::new_sample();
        limits(builder.limits_mut());
        let (server, handle) = builder
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
        let running = rt.spawn(server.run());

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
            add_pump_structure(&mut space, ns);
        }

        // Wait until the listener accepts connections. The sample server
        // generates its RSA keypair on start; in a debug build on a busy 2-core
        // CI runner, with many tests starting servers in parallel, that alone
        // can take well over 10 s, so the budget is generous. A server task
        // that already ended (e.g. the port is taken) fails at once instead.
        let deadline = Instant::now() + Duration::from_secs(90);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            if running.is_finished() {
                handle.cancel();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(port);
            }
            assert!(
                Instant::now() < deadline,
                "test server on port {port} did not start within 90 s"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        // Something else may answer on the port while our bind failed: the
        // server task then ends right away. Give it a moment to show that.
        let settle = Instant::now() + Duration::from_millis(300);
        while Instant::now() < settle {
            if running.is_finished() {
                handle.cancel();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(port);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(Self {
            rt,
            handle,
            nm,
            url,
            port,
            ns,
            dir,
        })
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

/// V3: a custom structure type `PumpType { Speed: Double, Running: Boolean,
/// Name: String }` described the OPC UA 1.04 way (DataTypeDefinition +
/// "Default Binary" encoding node), and a variable `Pump` holding one value.
fn add_pump_structure(space: &mut opcua::server::address_space::AddressSpace, ns: u16) {
    use opcua::nodes::{DataTypeBuilder, ObjectBuilder, ReferenceDirection};
    use opcua::types::custom::{DataTypeTree, DynamicStructure, EncodingIds, ParentIds, TypeInfo};
    use opcua::types::{
        DataTypeDefinition, ExtensionObject, LocalizedText, ReferenceTypeId, StructureDefinition,
        StructureField, StructureType,
    };

    let type_id = NodeId::new(ns, "PumpType");
    let encoding_id = NodeId::new(ns, "PumpType_DefaultBinary");
    let field = |name: &str, data_type: DataTypeId| StructureField {
        name: name.into(),
        description: LocalizedText::null(),
        data_type: data_type.into(),
        value_rank: -1,
        array_dimensions: None,
        max_string_length: 0,
        is_optional: false,
    };
    let definition = StructureDefinition {
        default_encoding_id: encoding_id.clone(),
        base_data_type: DataTypeId::Structure.into(),
        structure_type: StructureType::Structure,
        fields: Some(vec![
            field("Speed", DataTypeId::Double),
            field("Running", DataTypeId::Boolean),
            field("Name", DataTypeId::String),
        ]),
    };
    DataTypeBuilder::new(&type_id, "PumpType", "PumpType")
        .subtype_of(DataTypeId::Structure)
        .data_type_definition(DataTypeDefinition::Structure(definition.clone()))
        .insert(space);
    ObjectBuilder::new(&encoding_id, "Default Binary", "Default Binary")
        .reference(
            type_id.clone(),
            ReferenceTypeId::HasEncoding,
            ReferenceDirection::Inverse,
        )
        .insert(space);

    // The server encodes the value from the same description.
    let mut parents = ParentIds::new();
    parents.add_type(type_id.clone(), DataTypeId::Structure.into());
    let info = TypeInfo::from_type_definition(
        DataTypeDefinition::Structure(definition),
        "PumpType".to_string(),
        Some(EncodingIds {
            binary_id: encoding_id,
            json_id: NodeId::null(),
            xml_id: NodeId::null(),
        }),
        false,
        &type_id,
        &parents,
    )
    .expect("PumpType description");
    let mut tree = DataTypeTree::new(parents);
    tree.add_type(type_id.clone(), info);
    let tree = Arc::new(tree);
    let type_def = tree
        .get_struct_type(&type_id)
        .cloned()
        .expect("PumpType registered");
    let value = DynamicStructure::new_struct(
        type_def,
        tree.clone(),
        vec![
            Variant::Double(12.5),
            Variant::Boolean(true),
            Variant::from("P-101"),
        ],
    )
    .expect("PumpType value");
    let rw = AccessLevel::CURRENT_READ;
    let pump = VariableBuilder::new(&NodeId::new(ns, "Pump"), "Pump", "Pump")
        .data_type(type_id)
        .value(Variant::ExtensionObject(ExtensionObject::from_message(
            value,
        )))
        .access_level(rw)
        .user_access_level(rw)
        .build();
    space.add_variables(vec![pump], &ObjectId::ObjectsFolder.into());
}

/// A loopback port the OS reports free right now.
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("a free loopback port")
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

    pub fn send(&mut self, owner: crate::bff::model::Owner, kind: CommandKind) -> u64 {
        self.next_request += 1;
        let request_id = self.next_request;
        let command = Command {
            request_id: crate::bff::model::RequestId::new(request_id),
            owner,
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
            (event.origin
                == crate::bff::model::Origin::Reply(
                    crate::bff::model::RequestId::new(request_id).unwrap(),
                ))
            .then(|| event.clone())
        })
    }

    pub fn endpoint(url: &str) -> EndpointSpec {
        let mut endpoint = EndpointSpec::ua(url);
        endpoint.ua_security_policy = UaSecurityPolicy::None;
        endpoint.ua_security_mode = UaSecurityMode::None;
        endpoint
    }

    /// Connects and waits for `Connected`; returns the session info line.
    pub fn connect(
        &mut self,
        session: SessionId,
        endpoint: EndpointSpec,
    ) -> Result<String, String> {
        let id = self.send(session.into(), CommandKind::Connect { endpoint });
        match self.reply(id, Duration::from_secs(60)) {
            Some(Event {
                kind: EventKind::Connected { info, .. },
                ..
            }) => Ok(crate::i18n::backend_text(crate::i18n::Lang::En, &info)),
            Some(Event {
                kind: EventKind::Error { message, .. },
                ..
            }) => Err(crate::i18n::backend_text(crate::i18n::Lang::En, &message)),
            other => Err(format!("unexpected connect reply: {other:?}")),
        }
    }

    pub fn add_items(
        &mut self,
        session: SessionId,
        items: &[(ItemKey, String)],
    ) -> Vec<(ItemKey, Result<ItemInfo, BackendMsg>)> {
        let id = self.send(
            session.into(),
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
        let _ = self.send(session.into(), CommandKind::Disconnect);
        let _ = self.wait(Duration::from_secs(20), |event| {
            (event.owner == session.into() && matches!(event.kind, EventKind::Disconnected { .. }))
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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");

    let results = app.add_items(
        SessionId::new(1).unwrap(),
        &[
            (ItemKey::new(1).unwrap(), server.node("Level")),
            (
                ItemKey::new(2).unwrap(),
                format!("ns={};s=NoSuchTag", server.ns),
            ),
        ],
    );
    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .any(|(k, r)| *k == ItemKey::new(1).unwrap() && r.is_ok()),
        "valid node must be Ok: {results:?}"
    );
    let bad = results
        .iter()
        .find(|(k, _)| *k == ItemKey::new(2).unwrap())
        .and_then(|(_, r)| r.as_ref().err())
        .expect("unknown node must be an Err");
    assert!(
        crate::i18n::backend_text(crate::i18n::Lang::En, bad).contains("BadNodeIdUnknown"),
        "message must carry the server status, got {bad:?}"
    );

    server.set("Level", Variant::Double(42.5));
    let value = app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| {
        *v == Value::F64(42.5)
    });
    assert!(value.is_some(), "valid item must push its value");
}

#[test]
fn u2_malformed_node_id_is_an_error_not_a_silent_ok() {
    let server = TestServer::start(48602);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let results = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), "это не NodeId".to_string())],
    );
    assert!(
        matches!(results.first(), Some((KEY_1, Err(_)))),
        "malformed NodeId must be rejected: {results:?}"
    );
}

#[test]
fn added_node_is_reused_for_reads_and_writes() {
    let server = TestServer::start(48698);
    let mut app = Harness::new();
    let session = SessionId::FIRST;
    let key = ItemKey::FIRST;
    app.connect(session, Harness::endpoint(&server.url))
        .expect("connect");
    let results = app.add_items(
        session,
        &[
            (key, format!("  {}  ", server.node("Level"))),
            (ItemKey::new(2).unwrap(), "invalid".into()),
        ],
    );
    assert!(matches!(&results[0].1, Ok(_)), "{results:?}");
    assert!(
        matches!(&results[1].1, Err(BackendMsg::InvalidNodeId { .. })),
        "{results:?}"
    );
    // Preserve error priority: an unknown item is reported before payload
    // conversion; a subscribed item gets the shape error as a Wrote reply.
    for (key, expected_missing) in [(ItemKey::new(2).unwrap(), true), (key, false)] {
        let request = app.send(
            session.into(),
            CommandKind::Write {
                request: crate::bff::model::WriteRequest {
                    key,
                    value: Value::Array(vec![Value::Array(vec![Value::I64(1)])]),
                    type_hint: None,
                },
            },
        );
        let reply = app
            .reply(request, Duration::from_secs(10))
            .expect("write error reply");
        match reply.kind {
            EventKind::Wrote {
                result: Err(BackendMsg::NodeNotSubscribed),
                ..
            } if expected_missing => {}
            EventKind::Wrote {
                result: Err(BackendMsg::WriteMultiDim),
                ..
            } if !expected_missing => {}
            other => panic!("unexpected write error: {other:?}"),
        }
    }
    for value in [7.5, 9.5] {
        server.set("Level", Variant::Double(value));
        let id = app.send(
            session.into(),
            CommandKind::Read {
                keys: vec![key, ItemKey::new(2).unwrap()],
            },
        );
        let reply = app.reply(id, Duration::from_secs(10)).expect("read");
        let EventKind::Values { values } = reply.kind else {
            panic!("unexpected read reply")
        };
        assert_eq!(values[0].1.value, Value::F64(value));
        assert!(matches!(
            values[1].1.reason,
            Some(BackendMsg::NodeNotSubscribed)
        ));
    }
    let id = app.send(
        session.into(),
        CommandKind::Write {
            request: crate::bff::model::WriteRequest {
                key,
                value: Value::F64(12.5),
                type_hint: None,
            },
        },
    );
    let reply = app.reply(id, Duration::from_secs(10)).expect("write");
    assert!(
        matches!(reply.kind, EventKind::Wrote { result: Ok(ref outcome), .. } if outcome.accepted()),
        "{reply:?}"
    );
    let id = app.send(session.into(), CommandKind::Read { keys: vec![key] });
    let reply = app
        .reply(id, Duration::from_secs(10))
        .expect("read after write");
    assert!(
        matches!(reply.kind, EventKind::Values { values } if values[0].1.value == Value::F64(12.5))
    );
}

#[test]
fn subscription_push_carries_the_connect_request_across_reconnect() {
    use crate::bff::model::{Origin, RequestId};
    let server = TestServer::start(free_port());
    let mut app = Harness::new();
    let session = SessionId::new(1).unwrap();
    let key = ItemKey::new(1).unwrap();
    let mut captured = Vec::new();
    for _ in 0..2 {
        app.connect(session, Harness::endpoint(&server.url))
            .expect("connect");
        let epoch = RequestId::new(app.next_request).unwrap();
        let results = app.add_items(session, &[(key, server.node("Level"))]);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
        let event = app.wait(Duration::from_secs(20), |event| {
            (event.origin == Origin::Session(epoch) && matches!(&event.kind, EventKind::Values { values } if values.iter().any(|(id, _)| *id == key))).then(|| event.clone())
        }).expect("subscription push for connection");
        captured.push(event);
        app.disconnect(session);
    }
    assert_ne!(captured[0].origin, captured[1].origin);
}

#[test]
fn u2_no_monitored_item_means_no_active_subscription() {
    use crate::bff::model::SubscriptionState;
    let server = TestServer::start(48603);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let results = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), format!("ns={};s=Nope", server.ns))],
    );
    assert!(matches!(results.first(), Some((KEY_1, Err(_)))));
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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    // Keys far from the server's own id sequence (1, 2, ...).
    let results = app.add_items(
        SessionId::new(1).unwrap(),
        &[
            (ItemKey::new(501).unwrap(), server.node("Level")),
            (ItemKey::new(502).unwrap(), server.node("Counter")),
        ],
    );
    assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}");

    let removed = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::RemoveItems {
            keys: vec![ItemKey::new(501).unwrap()],
        },
    );
    assert!(matches!(
        app.reply(removed, PROMPT_LONG).expect("removed").kind,
        EventKind::Removed { .. }
    ));

    server.set("Level", Variant::Double(77.0));
    server.set("Counter", Variant::Int32(5));
    assert!(
        app.value_of(ItemKey::new(502).unwrap(), Duration::from_secs(10), |v| *v
            == Value::I64(5))
            .is_some(),
        "the remaining item must keep pushing"
    );
    assert!(
        app.value_of(ItemKey::new(501).unwrap(), Duration::from_secs(2), |v| *v
            == Value::F64(77.0))
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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect to the live server");
    let added = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), server.node("Level"))],
    );
    assert!(matches!(added.first(), Some((KEY_1, Ok(_)))));

    // Session 2 connects to a host that never answers.
    let hung = app.send(
        SessionId::new(2).unwrap().into(),
        CommandKind::Connect {
            endpoint: Harness::endpoint(&silent.url()),
        },
    );
    std::thread::sleep(Duration::from_millis(500));

    let started = Instant::now();
    let browse = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::Browse {
            node_id: "i=85".to_string(),
            filter: crate::bff::model::NodeClassFilter::All,
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

    let read = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::Read {
            keys: vec![ItemKey::new(1).unwrap()],
        },
    );
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
    app.connect(SessionId::new(3).unwrap(), Harness::endpoint(&server.url))
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
        SessionId::new(7).unwrap().into(),
        CommandKind::Connect {
            endpoint: Harness::endpoint(&silent.url()),
        },
    );
    std::thread::sleep(Duration::from_millis(300));
    let _ = app.send(SessionId::new(7).unwrap().into(), CommandKind::Disconnect);

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
        SessionId::new(5).unwrap().into(),
        CommandKind::Browse {
            node_id: String::new(),
            filter: crate::bff::model::NodeClassFilter::All,
            view_id: None,
        },
    );
    let read = app.send(
        SessionId::new(5).unwrap().into(),
        CommandKind::Read {
            keys: vec![ItemKey::new(1).unwrap(), ItemKey::new(2).unwrap()],
        },
    );
    let write = app.send(
        SessionId::new(5).unwrap().into(),
        CommandKind::Write {
            request: crate::bff::model::WriteRequest {
                key: ItemKey::new(1).unwrap(),
                value: Value::I64(1),
                type_hint: None,
            },
        },
    );
    let add = app.send(
        SessionId::new(5).unwrap().into(),
        CommandKind::AddItems { items: Vec::new() },
    );
    let test = app.send(SessionId::new(5).unwrap().into(), CommandKind::Test);

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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("first connect");

    let disconnect = app.send(SessionId::new(1).unwrap().into(), CommandKind::Disconnect);
    let connect = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::Connect {
            endpoint: Harness::endpoint(&server.url),
        },
    );
    let connected_at = |app: &Harness| {
        app.seen.iter().position(|(e, _)| {
            matches!(e.kind, EventKind::Connected { .. })
                && e.origin
                    == crate::bff::model::Origin::Reply(
                        crate::bff::model::RequestId::new(connect).unwrap(),
                    )
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
            matches!(e.kind, EventKind::Disconnected { .. })
                && e.origin
                    == crate::bff::model::Origin::Reply(
                        crate::bff::model::RequestId::new(disconnect).unwrap(),
                    )
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
        SessionId::new(1).unwrap().into(),
        CommandKind::Connect {
            endpoint: Harness::endpoint(&server.url),
        },
    );
    // Sent before the session is ready: must wait, not fail with "no connection".
    let browse = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::Browse {
            node_id: "i=85".to_string(),
            filter: crate::bff::model::NodeClassFilter::All,
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
        SessionId::new(1).unwrap().into(),
        CommandKind::Connect {
            endpoint: Harness::endpoint("opc.tcp://127.0.0.1:48619/"),
        },
    );
    let browse = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::Browse {
            node_id: String::new(),
            filter: crate::bff::model::NodeClassFilter::All,
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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    app.connect(SessionId::new(2).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let _ = app.send(
        SessionId::new(3).unwrap().into(),
        CommandKind::Connect {
            endpoint: Harness::endpoint(&silent.url()),
        },
    );
    std::thread::sleep(Duration::from_millis(300));

    let started = Instant::now();
    assert_eq!(
        app.worker.shutdown(Duration::from_secs(10)),
        crate::bff::ShutdownOutcome::Joined,
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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let added = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), server.node("Level"))],
    );
    assert!(matches!(added.first(), Some((KEY_1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(1.0))
            .is_some(),
        "the initial value is pushed"
    );

    // The subscription is already running: a new deadband must reach it.
    let _ = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 5.0,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    server.set("Level", Variant::Double(2.0));
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(2), |v| *v
            == Value::F64(2.0))
            .is_none(),
        "a change inside the deadband must be filtered out"
    );
    server.set("Level", Variant::Double(10.0));
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(10.0))
            .is_some(),
        "a change beyond the deadband is delivered"
    );

    // And switching the deadband off again also reaches the live items.
    let _ = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 0.0,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    server.set("Level", Variant::Double(10.5));
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(10.5))
            .is_some(),
        "without a deadband every change is delivered"
    );
}

#[test]
fn u4_publishing_interval_change_is_applied_to_a_live_subscription() {
    let server = TestServer::start(48622);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let added = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), server.node("Level"))],
    );
    assert!(matches!(added.first(), Some((KEY_1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(1.0))
            .is_some()
    );

    // An hour between publishes: a change must not arrive any time soon.
    let _ = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 3_600_000,
            deadband: 0.0,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    server.set("Level", Variant::Double(3.0));
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(3), |v| *v
            == Value::F64(3.0))
            .is_none(),
        "the subscription still publishes at the old, fast interval"
    );

    // Back to a fast interval: the pending change is delivered.
    let _ = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 0.0,
        },
    );
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(3.0))
            .is_some(),
        "the restored interval publishes the change"
    );
}

#[test]
fn u4_configure_before_the_first_item_is_used_when_the_subscription_is_created() {
    let server = TestServer::start(48623);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let _ = app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 100,
            deadband: 5.0,
        },
    );
    let added = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), server.node("Level"))],
    );
    assert!(matches!(added.first(), Some((KEY_1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(1.0))
            .is_some()
    );
    server.set("Level", Variant::Double(2.0));
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(2), |v| *v
            == Value::F64(2.0))
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
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");

    let browse = |app: &mut Harness, node: &str| {
        let id = app.send(
            SessionId::new(1).unwrap().into(),
            CommandKind::Browse {
                node_id: node.to_string(),
                filter: crate::bff::model::NodeClassFilter::Variables,
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
    endpoint.ua_security_policy = policy;
    endpoint.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    endpoint.ua_trust_server_cert = true;
    let info = app
        .connect(SessionId::new(1).unwrap(), endpoint)
        .unwrap_or_else(|e| panic!("{policy} / SignAndEncrypt must connect: {e}"));
    assert!(
        info.contains(policy.as_str()),
        "the session info names the policy: {info:?}"
    );
    let added = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), server.node("Level"))],
    );
    assert!(matches!(added.first(), Some((KEY_1, Ok(_)))), "{added:?}");
    assert!(
        app.value_of(ItemKey::new(1).unwrap(), Duration::from_secs(10), |v| *v
            == Value::F64(1.0))
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
        .connect(SessionId::new(1).unwrap(), EndpointSpec::ua(&server.url))
        .expect("connect");
    assert!(info.contains("not encrypted"), "{info:?}");
    assert!(
        info.contains("offers a secure connection"),
        "the sample server also offers secure endpoints: {info:?}"
    );
}

// ---------------------------------------------------------------------------
// U13: "trust the certificate" lasts one session
// ---------------------------------------------------------------------------

/// Where the client keeps the server's certificate while it is trusted.
fn trusted_copy_of_server_cert(server: &TestServer) -> PathBuf {
    use opcua::crypto::{CertificateStore, X509};
    use opcua::types::ByteString;
    let der = std::fs::read(server.dir.join("own").join("cert.der")).expect("server certificate");
    let cert = X509::from_byte_string(&ByteString::from(der)).expect("parse server certificate");
    TEST_PKI_DIR
        .get()
        .expect("client pki")
        .join("trusted")
        .join(CertificateStore::cert_file_name(&cert))
}

fn secure_endpoint(server: &TestServer, trust: bool) -> EndpointSpec {
    let mut endpoint = Harness::endpoint(&server.url);
    endpoint.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
    endpoint.ua_security_mode = UaSecurityMode::SignAndEncrypt;
    endpoint.ua_trust_server_cert = trust;
    endpoint
}

#[test]
fn u13_trusting_a_certificate_lasts_only_for_the_session() {
    let server = TestServer::start(48661);
    let mut app = Harness::new();
    let trusted = trusted_copy_of_server_cert(&server);

    assert!(
        app.connect(SessionId::new(1).unwrap(), secure_endpoint(&server, false))
            .is_err(),
        "an unknown server certificate is refused without the trust choice"
    );
    assert!(!trusted.exists());

    app.connect(SessionId::new(1).unwrap(), secure_endpoint(&server, true))
        .expect("connects when the user trusts the certificate");
    assert!(trusted.exists(), "trusted while the session lives");
    let _ = app.send(SessionId::new(1).unwrap().into(), CommandKind::Disconnect);
    assert!(
        app.wait(Duration::from_secs(20), |event| matches!(
            event.kind,
            EventKind::Disconnected { .. }
        )
        .then_some(()))
            .is_some()
    );
    assert!(
        !trusted.exists(),
        "the certificate must not stay trusted after the session"
    );

    assert!(
        app.connect(SessionId::new(1).unwrap(), secure_endpoint(&server, false))
            .is_err(),
        "the earlier session's trust must not carry over"
    );
}

// ---------------------------------------------------------------------------
// U15 / A9: searches report through the journal, not stderr
// ---------------------------------------------------------------------------

#[test]
fn u15_host_search_on_loopback_finds_the_server_without_lds_noise() {
    let server = TestServer::start(48671);
    let mut app = Harness::new();
    let id = app.send(
        crate::bff::model::Owner::Discovery,
        CommandKind::SearchUaServers {
            host: "127.0.0.1".to_string(),
            port: server.port,
            bind: None,
        },
    );
    match app.reply(id, Duration::from_secs(30)) {
        Some(Event {
            kind: EventKind::UaServersFound { servers, .. },
            ..
        }) => assert!(
            servers.iter().any(|s| s
                .endpoints
                .iter()
                .any(|e| e.url.contains(&server.port.to_string()))),
            "{servers:?}"
        ),
        other => panic!("unexpected search reply: {other:?}"),
    }
    let noise = app.wait(Duration::from_millis(300), |event| match &event.kind {
        EventKind::Log { message }
            if crate::i18n::backend_text(crate::i18n::Lang::En, &message).contains("LDS") =>
        {
            Some(crate::i18n::backend_text(crate::i18n::Lang::En, &message))
        }
        _ => None,
    });
    assert!(
        noise.is_none(),
        "loopback is not asked for an LDS: {noise:?}"
    );
}

#[test]
fn a9_network_search_reports_its_problems_as_journal_lines() {
    let mut app = Harness::new();
    let id = app.send(
        crate::bff::model::Owner::Discovery,
        CommandKind::SearchUaNetwork {
            bind: None,
            lds: None,
        },
    );
    assert!(matches!(
        app.reply(id, Duration::from_secs(60)),
        Some(Event {
            kind: EventKind::UaNetworkFound { .. },
            ..
        })
    ));
    let mut lines = Vec::new();
    while let Some(message) = app.wait(Duration::from_millis(200), |event| match &event.kind {
        EventKind::Log { message } if event.origin == crate::bff::model::Origin::Unsolicited => {
            Some(crate::i18n::backend_text(crate::i18n::Lang::En, &message))
        }
        _ => None,
    }) {
        lines.push(message);
    }
    // Off Windows there is no mDNS backend: that must be a journal line, not a
    // stderr line. On Windows mDNS really runs and may legitimately say nothing.
    #[cfg(not(windows))]
    assert!(
        lines.iter().any(|line| line.starts_with("mDNS")),
        "the skipped mDNS step is reported: {lines:?}"
    );
    assert!(
        lines.iter().filter(|line| line.starts_with("LDS")).count() <= 1,
        "the loopback LDS is reported at most once: {lines:?}"
    );
}

/// U15: an explicitly named LDS host is the one that is asked (not the
/// machine's standard 4840 endpoints), with its own port.
#[test]
fn u15_the_named_lds_host_is_the_one_that_is_asked() {
    use crate::bff::model::LdsTarget;
    use crate::bff::net::NetworkBinding;
    let server = TestServer::start(48672);
    let mut app = Harness::new();
    let target = |port: u16| LdsTarget {
        host: "127.0.0.1".to_string(),
        port,
        bind: NetworkBinding::loopback(),
    };

    // Nothing listens on the named port: the journal names exactly that URL.
    let id = app.send(
        crate::bff::model::Owner::Discovery,
        CommandKind::SearchUaNetwork {
            bind: None,
            lds: Some(target(48673)),
        },
    );
    assert!(matches!(
        app.reply(id, Duration::from_secs(60)),
        Some(Event {
            kind: EventKind::UaNetworkFound { .. },
            ..
        })
    ));
    let mut lines = Vec::new();
    while let Some(line) = app.wait(Duration::from_millis(200), |event| match &event.kind {
        EventKind::Log { message } if event.origin == crate::bff::model::Origin::Unsolicited => {
            Some(crate::i18n::backend_text(crate::i18n::Lang::En, &message))
        }
        _ => None,
    }) {
        lines.push(line);
    }
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("LDS") && l.contains(":48673")),
        "the named host is reported: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| !l.contains(":4840")),
        "the standard endpoints are not asked when a host is named: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| !l.starts_with("mDNS")),
        "without an adapter only the LDS is asked: {lines:?}"
    );

    // A UA server that is not an LDS answers "unsupported": reachable, no noise.
    let id = app.send(
        crate::bff::model::Owner::Discovery,
        CommandKind::SearchUaNetwork {
            bind: None,
            lds: Some(target(server.port)),
        },
    );
    assert!(matches!(
        app.reply(id, Duration::from_secs(60)),
        Some(Event {
            kind: EventKind::UaNetworkFound { .. },
            ..
        })
    ));
    let noise = app.wait(Duration::from_millis(300), |event| match &event.kind {
        EventKind::Log { message }
            if crate::i18n::backend_text(crate::i18n::Lang::En, &message)
                .contains(&server.port.to_string()) =>
        {
            Some(crate::i18n::backend_text(crate::i18n::Lang::En, &message))
        }
        _ => None,
    });
    assert!(noise.is_none(), "{noise:?}");
}

#[test]
fn u1_a_retry_right_after_a_failed_connect_never_meets_a_stale_connecting_slot() {
    // The application retries as soon as it sees the Connect error. The slot of
    // the failed attempt must already be gone by then.
    let mut app = Harness::new();
    for attempt in 0..15 {
        let message = app
            .connect(
                SessionId::new(9).unwrap(),
                Harness::endpoint("opc.tcp://127.0.0.1:48679/"),
            )
            .expect_err("nothing listens there");
        assert!(
            !message.contains("already in progress") && !message.contains("already connected"),
            "attempt {attempt}: {message}"
        );
    }
}

// ---------------------------------------------------------------------------
// V3: structures decode into named fields
// ---------------------------------------------------------------------------

#[test]
fn v3_custom_structure_reads_as_named_fields() {
    let server = TestServer::start(48700);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    let results = app.add_items(
        SessionId::new(1).unwrap(),
        &[(ItemKey::new(1).unwrap(), server.node("Pump"))],
    );
    assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}");
    let type_name = results
        .first()
        .and_then(|(_, r)| r.as_ref().ok())
        .and_then(|info| info.data_type.clone());
    assert!(
        type_name.is_some(),
        "the Type column names the structure: {results:?}"
    );
    let expected = Value::Struct {
        type_name: "PumpType".to_string(),
        fields: vec![
            ("Speed".to_string(), Value::F64(12.5)),
            ("Running".to_string(), Value::Bool(true)),
            ("Name".to_string(), Value::Str("P-101".to_string())),
        ],
    };
    let got = app.value_of(ItemKey::new(1).unwrap(), PROMPT_LONG, |v| {
        matches!(v, Value::Struct { .. })
    });
    let got = got.unwrap_or_else(|| {
        panic!(
            "no structured value; events: {:?}",
            app.seen.iter().map(|(e, _)| &e.kind).collect::<Vec<_>>()
        )
    });
    match &got {
        Value::Struct { fields, .. } => assert_eq!(
            fields,
            match &expected {
                Value::Struct { fields, .. } => fields,
                _ => unreachable!(),
            }
        ),
        _ => unreachable!(),
    }
    let unresolved = app.wait(Duration::from_millis(500), |event| match &event.kind {
        EventKind::Log {
            message: BackendMsg::TypesUnresolved { .. },
        } => Some(()),
        _ => None,
    });
    assert!(
        unresolved.is_none(),
        "a known structure is not an unresolved type"
    );
}

// ---------------------------------------------------------------------------
// P1: a large dropped tree must keep pushing every tag
// ---------------------------------------------------------------------------

impl TestServer {
    /// `count` Int32 variables `Big0..` (value 0) in a `Big` folder.
    fn add_many(&self, count: usize) -> Vec<String> {
        let space = self.nm.address_space();
        let mut space = space.write();
        let folder = NodeId::new(self.ns, "Big");
        space.add_folder(&folder, "Big", "Big", &ObjectId::ObjectsFolder.into());
        let rw = AccessLevel::CURRENT_READ | AccessLevel::CURRENT_WRITE;
        let vars = (0..count)
            .map(|i| {
                let name = format!("Big{i}");
                VariableBuilder::new(&NodeId::new(self.ns, name.as_str()), &name, &name)
                    .data_type(DataTypeId::Int32)
                    .value(Variant::Int32(0))
                    .access_level(rw)
                    .user_access_level(rw)
                    .build()
            })
            .collect();
        space.add_variables(vars, &folder);
        (0..count).map(|i| self.node(&format!("Big{i}"))).collect()
    }
}

#[test]
fn explicit_poll_reads_without_monitored_value_stream() {
    use crate::bff::model::{AcquisitionConfig, Origin, SubscriptionState, UpdateMode};
    let server = TestServer::start(free_port());
    let mut app = Harness::new();
    let session = SessionId::new(1).unwrap();
    let key = ItemKey::new(1).unwrap();
    let mut endpoint = Harness::endpoint(&server.url);
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Poll;
    endpoint.acquisition = Some(profile);
    app.connect(session, endpoint).expect("connect");
    assert!(
        app.add_items(session, &[(key, server.node("Level"))])[0]
            .1
            .is_ok()
    );
    server.set("Level", Variant::Double(37.5));
    let pushed = app.wait(Duration::from_secs(2), |event| match &event.kind {
        EventKind::Values { values }
            if matches!(event.origin, Origin::Session(_))
                && values.iter().any(|(k, _)| *k == key) =>
        {
            Some(())
        }
        EventKind::SubscriptionState {
            state: SubscriptionState::Active,
        } => Some(()),
        _ => None,
    });
    assert!(pushed.is_none(), "poll created a value stream");
    let id = app.send(session.into(), CommandKind::Read { keys: vec![key] });
    let reply = app.reply(id, Duration::from_secs(15)).expect("read");
    assert!(
        matches!(reply.kind, EventKind::Values { values } if values.iter().any(|(k, v)| *k == key && v.value == Value::F64(37.5)))
    );
}

/// P1 scenario: `total` tags added in batches of 100 without waiting (like
/// «add children»), then every tag must show its initial value and a change.
/// Returns how many keys got each, plus the poll-only keys reported.
fn p1_run(server: &TestServer, total: usize, pushed: usize) -> (usize, usize, Vec<ItemKey>) {
    let nodes = server.add_many(total);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect");
    app.send(
        SessionId::new(1).unwrap().into(),
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 500,
            deadband: 0.0,
        },
    );
    let mut requests = Vec::new();
    for (batch, chunk) in nodes.chunks(100).enumerate() {
        let items = chunk
            .iter()
            .enumerate()
            .map(|(i, node)| ItemSpec {
                key: ItemKey::new((batch * 100 + i + 1) as u32).unwrap(),
                item_id: node.clone(),
            })
            .collect();
        requests.push(app.send(
            SessionId::new(1).unwrap().into(),
            CommandKind::AddItems { items },
        ));
    }
    for id in requests {
        let reply = app
            .reply(id, Duration::from_secs(90))
            .expect("AddItems reply");
        match reply.kind {
            EventKind::AddItems { results } => {
                assert!(results.iter().all(|(_, r)| r.is_ok()), "{results:?}")
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    let poll_only: Vec<ItemKey> = std::iter::from_fn(|| {
        app.wait(Duration::from_millis(300), |event| match &event.kind {
            EventKind::PollOnly { keys } => Some(keys.clone()),
            _ => None,
        })
    })
    .flatten()
    .collect();
    let seen = |app: &mut Harness, want: Value, secs: u64| {
        let mut got = std::collections::HashSet::new();
        let deadline = Instant::now() + Duration::from_secs(secs);
        while got.len() < pushed && Instant::now() < deadline {
            let _ = app.wait(Duration::from_millis(200), |event| match &event.kind {
                EventKind::Values { values } => {
                    for (k, v) in values {
                        if v.value == want {
                            got.insert(*k);
                        }
                    }
                    Some(())
                }
                _ => None,
            });
        }
        got.len()
    };
    let initial = seen(&mut app, Value::I64(0), 30);
    for i in 0..total {
        server.set(&format!("Big{i}"), Variant::Int32(7));
    }
    let changed = seen(&mut app, Value::I64(7), 30);
    (initial, changed, poll_only)
}

/// P1 regression: the sample server takes 1000 monitored items per
/// subscription; tags past that were refused (BadTooManyMonitoredItems) and
/// never updated. Now they go to a second subscription.
#[test]
fn p1_tags_past_the_per_subscription_limit_still_push() {
    let server = TestServer::start(48701);
    let (initial, changed, poll_only) = p1_run(&server, 1500, 1500);
    assert!(poll_only.is_empty(), "two subscriptions take them all");
    assert_eq!(initial, 1500, "initial values pushed");
    assert_eq!(changed, 1500, "changed values pushed");
}

/// P1: a server that takes fewer items per call than the client sends
/// (BadTooManyOperations) gets smaller calls instead of failing every item.
#[test]
fn p1_too_many_operations_per_call_shrinks_the_call() {
    let server = TestServer::start_limited(48702, |l| {
        l.operational.max_monitored_items_per_call = 30;
    });
    let (initial, changed, poll_only) = p1_run(&server, 300, 300);
    assert!(poll_only.is_empty());
    assert_eq!(initial, 300);
    assert_eq!(changed, 300);
}

/// P1: when no subscription can take more items (one subscription of 200
/// allowed), the rest is reported poll-only, never left silent.
#[test]
fn p1_tags_past_every_limit_are_reported_poll_only() {
    let server = TestServer::start_limited(48703, |l| {
        l.subscriptions.max_monitored_items_per_sub = 200;
        l.subscriptions.max_subscriptions_per_session = 1;
    });
    let (initial, _changed, poll_only) = p1_run(&server, 300, 200);
    assert_eq!(poll_only.len(), 100, "the 100 over the limit");
    assert_eq!(initial, 200, "the monitored ones push");
}

/// CI flake (PR #7): a fixed test port already held by another socket made
/// the server stop before listening. The start moves to a free port.
#[test]
fn test_server_moves_off_a_taken_port() {
    let _holder = std::net::TcpListener::bind(("127.0.0.1", 48710)).expect("hold the port");
    let server = TestServer::start(48710);
    assert_ne!(server.port, 48710);
    let mut app = Harness::new();
    app.connect(SessionId::new(1).unwrap(), Harness::endpoint(&server.url))
        .expect("connect on the moved port");
}

const KEY_1: ItemKey = ItemKey::FIRST;

/// Transparent None-security proxy which withholds the real initial namespace
/// ReadResponse and subsequent responses, counting queued Browse requests.
struct HeldNamespaceProxy {
    url: String,
    held: std::sync::mpsc::Receiver<()>,
    later_services: Arc<std::sync::atomic::AtomicUsize>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HeldNamespaceProxy {
    fn start(server: &TestServer) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_port = server.port;
        let (held_tx, held) = channel();
        let later_services = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = later_services.clone();
        let (stop, stopped) = tokio::sync::oneshot::channel();
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
                            let Ok((client, _)) = accepted else { break; };
                            let held_tx = held_tx.clone();
                            let counter = counter.clone();
                            tasks.spawn(async move {
                                let Ok(server) = tokio::net::TcpStream::connect(("127.0.0.1", server_port)).await else { return; };
                                let (mut cr, mut cw) = client.into_split();
                                let (mut sr, mut sw) = server.into_split();
                                let pending = Arc::new(std::sync::atomic::AtomicU64::new(0));
                                let pending_request = pending.clone();
                                let upstream = async move {
                                    let mut held = false;
                                    loop {
                                        let mut header = [0u8; 8];
                                        if cr.read_exact(&mut header).await.is_err() { break; }
                                        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                                        if !(8..=1_048_576).contains(&len) { break; }
                                        let mut frame = vec![0; len];
                                        frame[..8].copy_from_slice(&header);
                                        if cr.read_exact(&mut frame[8..]).await.is_err() { break; }
                                        if &header[..4] == b"MSGF" && len > 24 {
                                            use opcua::types::{BinaryDecodable, ContextOwned, ObjectId, ReadRequest};
                                            let context = ContextOwned::default();
                                            let mut body = std::io::Cursor::new(&frame[24..]);
                                            if let Ok(id) = NodeId::decode(&mut body, &context.context()) {
                                                if id == NodeId::from(ObjectId::ReadRequest_Encoding_DefaultBinary) {
                                                    if let Ok(request) = ReadRequest::decode(&mut body, &context.context()) {
                                                        let namespace = NodeId::new(0, 2255u32);
                                                        if !held && request.nodes_to_read.as_ref().is_some_and(|nodes| nodes.iter().any(|n| n.node_id == namespace)) {
                                                            held = true;
                                                            let request_id = u32::from_le_bytes([frame[20], frame[21], frame[22], frame[23]]);
                                                            pending_request.store(u64::from(request_id) + 1, std::sync::atomic::Ordering::SeqCst);
                                                            let _ = held_tx.send(());
                                                        }
                                                    }
                                                }
                                                if held && id == NodeId::from(ObjectId::BrowseRequest_Encoding_DefaultBinary) {
                                                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                                }
                                            }
                                        }
                                        if sw.write_all(&frame).await.is_err() { break; }
                                    }
                                };
                                let downstream = async move {
                                    let mut withholding = false;
                                    loop {
                                        let mut header = [0u8; 8];
                                        if sr.read_exact(&mut header).await.is_err() { break; }
                                        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                                        if !(8..=1_048_576).contains(&len) { break; }
                                        let mut frame = vec![0; len];
                                        frame[..8].copy_from_slice(&header);
                                        if sr.read_exact(&mut frame[8..]).await.is_err() { break; }
                                        if &header[..4] == b"MSGF" && len >= 24 {
                                            let id = u32::from_le_bytes([frame[20], frame[21], frame[22], frame[23]]);
                                            let held_id = pending.load(std::sync::atomic::Ordering::SeqCst);
                                            if held_id != 0 && u64::from(id) + 1 == held_id { withholding = true; }
                                        }
                                        if !withholding && cw.write_all(&frame).await.is_err() { break; }
                                    }
                                };
                                tokio::select! { _ = upstream => {}, _ = downstream => {} }
                            });
                        }
                    }
                }
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
            });
        });
        Self {
            url: format!("opc.tcp://127.0.0.1:{port}/"),
            held,
            later_services,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}
impl Drop for HeldNamespaceProxy {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn b15_live_initial_load_timeout_rejects_queued_service() {
    let server = TestServer::start(48721);
    let proxy = HeldNamespaceProxy::start(&server);
    let mut app = Harness::new();
    let session = SessionId::new(1).unwrap();
    app.connect(session, Harness::endpoint(&proxy.url))
        .expect("connect through proxy");
    proxy
        .held
        .recv_timeout(Duration::from_secs(10))
        .expect("namespace read held");
    let request = app.send(
        session.into(),
        CommandKind::Browse {
            node_id: "i=85".into(),
            filter: crate::bff::model::NodeClassFilter::All,
            view_id: None,
        },
    );
    let read = app.send(
        session.into(),
        CommandKind::Read {
            keys: vec![ItemKey::new(1).unwrap()],
        },
    );
    let disconnected = app.wait(Duration::from_secs(50), |event| {
        matches!(
            &event.kind,
            EventKind::Disconnected {
                reason: BackendMsg::SessionClosedAfterTimeout
            }
        )
        .then_some(())
    });
    assert!(
        disconnected.is_some(),
        "typed timeout disconnect; events: {:?}",
        app.seen
    );
    let reply = app.reply(request, Duration::from_secs(5));
    assert!(
        matches!(
            reply,
            Some(Event {
                kind: EventKind::Error {
                    message: BackendMsg::NoConnection,
                    ..
                },
                ..
            })
        ),
        "queued command rejected: {reply:?}"
    );
    let read_reply = app.reply(read, Duration::from_secs(5));
    assert!(
        matches!(read_reply, Some(Event { kind: EventKind::Values { values }, .. }) if values.len() == 1 && matches!(values[0].1.reason, Some(BackendMsg::NoConnection))),
        "queued read rejected"
    );
    assert_eq!(
        proxy
            .later_services
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[test]
fn b15_live_disconnect_interrupts_held_initial_load() {
    let server = TestServer::start(48722);
    let proxy = HeldNamespaceProxy::start(&server);
    let mut app = Harness::new();
    let session = SessionId::new(1).unwrap();
    app.connect(session, Harness::endpoint(&proxy.url))
        .expect("connect through proxy");
    proxy
        .held
        .recv_timeout(Duration::from_secs(10))
        .expect("namespace read held");
    let request = app.send(
        session.into(),
        CommandKind::Browse {
            node_id: "i=85".into(),
            filter: crate::bff::model::NodeClassFilter::All,
            view_id: None,
        },
    );
    let started = Instant::now();
    let disconnect = app.send(session.into(), CommandKind::Disconnect);
    let reply = app.reply(disconnect, Duration::from_secs(12));
    assert!(
        matches!(
            reply,
            Some(Event {
                kind: EventKind::Disconnected {
                    reason: BackendMsg::ConnectionClosed
                },
                ..
            })
        ),
        "disconnect: {reply:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(12));
    let reply = app.reply(request, Duration::from_secs(5));
    assert!(
        matches!(
            reply,
            Some(Event {
                kind: EventKind::Error {
                    message: BackendMsg::NoConnection,
                    ..
                },
                ..
            })
        ),
        "queued command rejected: {reply:?}"
    );
    assert_eq!(
        proxy
            .later_services
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

// Observe the actual unencrypted SDK ReadRequest while forwarding the complete
// conversation to the sample server. The target NodeId excludes setup reads.
struct ReadAgeProxy {
    url: String,
    ages: Receiver<f64>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl ReadAgeProxy {
    fn start(server: &TestServer, target: NodeId) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_port = server.port;
        let (tx, ages) = channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
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
                            let Ok((client, _)) = accepted else { break; };
                            let tx = tx.clone();
                            let target = target.clone();
                            tasks.spawn(async move {
                                let Ok(server) = tokio::net::TcpStream::connect(("127.0.0.1", server_port)).await else { return; };
                                let (mut cr, mut cw) = client.into_split();
                                let (mut sr, mut sw) = server.into_split();
                                let upstream = async move {
                                    loop {
                                        let mut header = [0u8; 8];
                                        if cr.read_exact(&mut header).await.is_err() { break; }
                                        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                                        if !(8..=1_048_576).contains(&len) { break; }
                                        let mut frame = vec![0; len];
                                        frame[..8].copy_from_slice(&header);
                                        if cr.read_exact(&mut frame[8..]).await.is_err() { break; }
                                        if &header[..4] == b"MSGF" && len > 24 {
                                            use opcua::types::{BinaryDecodable, ContextOwned, ObjectId, ReadRequest};
                                            let context = ContextOwned::default();
                                            let mut body = std::io::Cursor::new(&frame[24..]);
                                            if let Ok(id) = NodeId::decode(&mut body, &context.context()) {
                                                if id == NodeId::from(ObjectId::ReadRequest_Encoding_DefaultBinary) {
                                                    if let Ok(request) = ReadRequest::decode(&mut body, &context.context()) {
                                                        if request.nodes_to_read.as_ref().is_some_and(|nodes| nodes.iter().any(|n| n.node_id == target)) {
                                                            let _ = tx.send(request.max_age);
                                                        }
                                                    }
                                                }
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
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
            });
        });
        Self {
            url: format!("opc.tcp://127.0.0.1:{port}/"),
            ages,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}
impl Drop for ReadAgeProxy {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn ua_live_read_sends_requested_max_age_and_direct_read_sends_zero() {
    let server = TestServer::start(free_port());
    let node = server.node("Level");
    let proxy = ReadAgeProxy::start(&server, super::values::parse_node(&node).unwrap());
    let mut app = Harness::new();
    let session = SessionId::new(1).unwrap();
    let key = ItemKey::new(1).unwrap();
    let mut endpoint = Harness::endpoint(&proxy.url);
    let mut profile = crate::bff::model::AcquisitionConfig::ua_default();
    profile.update_mode = crate::bff::model::UpdateMode::Poll;
    endpoint.acquisition = Some(profile);
    app.connect(session, endpoint).expect("connect");
    let added = app.add_items(session, &[(key, node)]);
    assert!(added[0].1.is_ok(), "{added:?}");
    // AddItems may itself read attributes of the target; only the following
    // correlated commands are under test.
    while proxy.ages.try_recv().is_ok() {}
    for (kind, expected) in [
        (
            CommandKind::ReadCached {
                keys: vec![key],
                max_age_ms: 1234,
            },
            1234.0,
        ),
        (CommandKind::Read { keys: vec![key] }, 0.0),
    ] {
        let id = app.send(session.into(), kind);
        let reply = app.reply(id, Duration::from_secs(10)).expect("read reply");
        assert!(
            matches!(reply.kind, EventKind::Values { ref values } if values.len() == 1 && values[0].1.reason.is_none()),
            "{reply:?}"
        );
        assert_eq!(
            proxy
                .ages
                .recv_timeout(Duration::from_secs(10))
                .expect("SDK wire ReadRequest"),
            expected
        );
    }
    app.disconnect(session);
}

#[test]
fn ua_live_reports_requested_and_server_revised_subscription_parameters() {
    let server = TestServer::start_limited(free_port(), |limits| {
        limits.subscriptions.min_publishing_interval_ms = 2000.0;
        limits.subscriptions.min_sampling_interval_ms = 2000.0;
    });
    let mut app = Harness::new();
    let session = SessionId::new(1).unwrap();
    let key = ItemKey::new(1).unwrap();
    let mut endpoint = Harness::endpoint(&server.url);
    let mut profile = crate::bff::model::AcquisitionConfig::ua_default();
    profile.interval_ms = 1000;
    profile.update_mode = crate::bff::model::UpdateMode::Subscription;
    endpoint.acquisition = Some(profile);
    app.connect(session, endpoint).expect("connect");
    let results = app.add_items(session, &[(key, server.node("Level"))]);
    assert!(results[0].1.is_ok(), "{results:?}");
    let subscriptions = app
        .wait(Duration::from_secs(10), |event| match &event.kind {
            EventKind::UaSubscriptionParameters { subscriptions }
                if subscriptions
                    .iter()
                    .any(|s| s.items.iter().any(|i| i.key == key)) =>
            {
                Some(subscriptions.clone())
            }
            _ => None,
        })
        .expect("negotiated SDK snapshot");
    let sub = subscriptions
        .iter()
        .find(|s| s.items.iter().any(|i| i.key == key))
        .unwrap();
    assert_eq!(sub.requested_publishing_ms, 1000);
    let revised = sub.revised.as_ref().expect("SDK subscription state");
    assert_eq!(revised.publishing_ms, 2000);
    assert!(revised.valid, "{revised:?}");
    let item = sub.items.iter().find(|item| item.key == key).unwrap();
    assert_eq!(item.requested_sampling_ms, -1.0);
    assert_eq!(item.requested_queue_size, 1);
    // This sample server returns -1 for the subscription-default sentinel.
    assert_eq!(item.revised_sampling_ms, -1.0);
    assert!(item.revised_queue_size > 0);
    app.disconnect(session);
}
