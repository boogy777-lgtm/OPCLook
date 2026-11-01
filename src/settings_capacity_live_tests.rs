//! M09/M03–M04: old settings through the real loader, Engine and UA Gateway.
use super::{Settings, SettingsNotice, settings_path, test_isolation};
use crate::bff::model::{
    BackendMsg, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, Origin,
    PollFreshness, Protocol, ReadNeed, ReadTerminal, ReadTerminalStatus, RequestId, SessionId,
    UaSecurityMode, UaSecurityPolicy, UpdateMode, Value,
};
use opclook_engine::{Due, Engine};
use opcua::server::ServerBuilder;
use opcua::server::address_space::{AccessLevel, VariableBuilder};
use opcua::server::diagnostics::NamespaceMetadata;
use opcua::server::node_manager::memory::{SimpleNodeManager, simple_node_manager};
use opcua::types::{DataTypeId, DataValue, NodeId, ObjectId, Variant};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Server {
    rt: tokio::runtime::Runtime,
    handle: opcua::server::ServerHandle,
    nm: Arc<SimpleNodeManager>,
    ns: u16,
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
                        .data_type(DataTypeId::Int32)
                        .value(Variant::Int32(1))
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
            nm,
            ns,
            url,
            dir,
        }
    }
    fn set(&self, name: &str, value: i32) {
        self.nm
            .set_value(
                self.handle.subscriptions(),
                &NodeId::new(self.ns, name),
                None,
                DataValue::new_now(Variant::Int32(value)),
            )
            .unwrap();
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

fn recv_until(engine: &mut Engine<()>, seen: &mut Vec<Event>, predicate: impl Fn(&Event) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(event) = engine.try_recv() {
            let done = predicate(&event);
            engine
                .accept(&event)
                .unwrap_or_else(|_| panic!("rejected event: {event:?}"));
            seen.push(event);
            if done {
                return;
            }
        } else {
            assert!(Instant::now() < deadline, "timed out; events: {seen:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
fn reply(engine: &mut Engine<()>, seen: &mut Vec<Event>, id: RequestId) {
    recv_until(engine, seen, |event| event.origin == Origin::Reply(id));
}
fn exercise(server: &Server, endpoint: EndpointSpec, explicit: bool) {
    let terminals = Arc::new(Mutex::new(Vec::<ReadTerminal>::new()));
    let sink = terminals.clone();
    let mut engine = Engine::<()>::default();
    engine.set_read_terminal_sink(Some(Box::new(move |_, _, terminal| {
        sink.lock().unwrap().push(terminal.clone())
    })));
    let mut seen = Vec::new();
    let sid = SessionId::FIRST;
    let a = ItemKey::FIRST;
    let b = ItemKey::new(2).unwrap();
    let id = engine
        .send(sid, Protocol::OpcUa, CommandKind::Connect { endpoint }, ())
        .unwrap();
    reply(&mut engine, &mut seen, id);
    assert!(matches!(
        seen.last().unwrap().kind,
        EventKind::Connected { .. }
    ));
    // Separate Add requests deterministically give A the only monitored slot.
    for (key, name) in [(a, "A"), (b, "B")] {
        let id = engine
            .send(
                sid,
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key,
                        item_id: server.node(name),
                    }],
                },
                (),
            )
            .unwrap();
        reply(&mut engine, &mut seen, id);
        assert!(
            matches!(&seen.last().unwrap().kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
        );
    }
    assert!(
        seen.iter()
            .any(|e| matches!(&e.kind, EventKind::PollOnly { keys } if keys == &[b]))
    );
    assert!(
        seen.iter().any(|e| match &e.kind {
            EventKind::Log {
                message: BackendMsg::SubscriptionItemsUnavailable { count: 1 },
            } => explicit,
            EventKind::Log {
                message: BackendMsg::PollOnlyItems { count: 1 },
            } => !explicit,
            _ => false,
        }),
        "profile-specific warning missing: {seen:?}"
    );
    // Advance the Engine clock instead of waiting for the five-second grace.
    let now = Instant::now() + Duration::from_secs(10);
    let due = engine.due(now, Duration::from_secs(5), None);
    let [
        Due::Read {
            session,
            protocol,
            keys,
            permit,
        },
    ] = due.as_slice()
    else {
        panic!("initial read missing or ambiguous")
    };
    assert!(keys.contains(&b));
    let id = engine
        .send_due_read(*session, *protocol, keys.clone(), *permit, ())
        .unwrap();
    reply(&mut engine, &mut seen, id);
    assert!(
        terminals.lock().unwrap().iter().any(|t| t.key == b
            && t.needs.contains(&ReadNeed::Initial)
            && matches!(t.status, ReadTerminalStatus::Received(_))),
        "unmonitored item needs a terminal initial read"
    );
    server.set("A", 42);
    recv_until(
        &mut engine,
        &mut seen,
        |event| matches!(&event.kind, EventKind::Values { values } if matches!(event.origin, Origin::Session(_) | Origin::UaSession { .. }) && values.iter().any(|(key, value)| *key == a && value.value == Value::I64(42))),
    );
    let next = engine.due(now + Duration::from_secs(10), Duration::from_secs(5), None);
    if explicit {
        assert!(
            next.is_empty(),
            "explicit subscription must not schedule fallback"
        );
    } else {
        let [
            Due::Read {
                session,
                protocol,
                keys,
                permit,
            },
        ] = next.as_slice()
        else {
            panic!("legacy fallback absent or ambiguous")
        };
        assert_eq!(keys, &[b]);
        let id = engine
            .send_due_read(*session, *protocol, keys.clone(), *permit, ())
            .unwrap();
        reply(&mut engine, &mut seen, id);
        assert!(
            matches!(&seen.last().unwrap().kind, EventKind::Values { values } if values.iter().any(|(key, _)| *key == b))
        );
    }
    let shutdown = engine.shutdown();
    assert_eq!(shutdown.ua, opclook_backend::bff::ShutdownOutcome::Joined);
    assert_eq!(shutdown.da, opclook_backend::bff::ShutdownOutcome::Joined);
}

#[test]
fn migrated_and_legacy_profiles_with_limited_monitored_capacity() {
    opclook_backend::bff::ua::use_private_pki_dir();
    let server = Server::start();
    let dir = std::env::temp_dir().join(format!("opclook-m09-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut endpoint = EndpointSpec::ua(&server.url);
    endpoint.ua_security_mode = UaSecurityMode::None;
    endpoint.ua_security_policy = UaSecurityPolicy::None;
    let migrated = test_isolation::with_appdata_dir(&dir, || {
        let mut settings = Settings::default();
        settings.view.options.update_interval_secs = 5;
        settings.recent_endpoints.push(endpoint.clone());
        let mut old = serde_json::to_value(&settings).unwrap();
        old["recent_endpoints"][0]
            .as_object_mut()
            .unwrap()
            .remove("acquisition");
        let path = settings_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let loaded = Settings::load();
        assert!(matches!(
            loaded.notices.as_slice(),
            [SettingsNotice::AcquisitionMigrated {
                ua: 1,
                da: 0,
                interval_ms: 5000
            }]
        ));
        let migrated = loaded.settings.recent_endpoints[0].clone();
        let profile = migrated.acquisition.as_ref().unwrap();
        assert_eq!(profile.interval_ms, 5000);
        assert_eq!(profile.update_mode, UpdateMode::Subscription);
        assert_eq!(profile.poll_freshness, PollFreshness::FreshSource);
        assert!(!profile.read_after_write);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert!(saved["recent_endpoints"][0]["acquisition"].is_object());
        assert!(Settings::load().notices.is_empty());
        migrated
    });
    exercise(&server, migrated, true);
    server.set("A", 1);
    exercise(&server, endpoint, false);
    let _ = std::fs::remove_dir_all(dir);
}
