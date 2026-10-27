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
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemInfo, ItemKey, ItemSpec, Protocol,
    SessionId, UaSecurityMode, UaSecurityPolicy, Value,
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
