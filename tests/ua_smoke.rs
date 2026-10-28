use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, Protocol, QualityKind, Value,
};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_url() -> String {
    std::env::var("OPCUA_TEST_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn wait_for(
    gateway: &Gateway,
    command: Command,
    timeout: Duration,
    predicate: impl Fn(&EventKind) -> bool,
) -> Option<Event> {
    let request_id = command.request_id;
    let session = command.session;
    gateway.send(command).expect("gateway send");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.origin != request_id.into() || event.session != session {
                continue;
            }
            if predicate(&event.kind) {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

fn connect(gateway: &Gateway, session: u64) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(test_url()),
            },
        },
        Duration::from_secs(40),
        |kind| matches!(kind, EventKind::Connected { .. } | EventKind::Error { .. }),
    )
    .expect("connect timeout")
}

fn disconnect(gateway: &Gateway, session: u64) {
    let _ = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(5),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    );
}

fn with_session(test: impl FnOnce(&Gateway, u64)) {
    let gateway = Gateway::new();
    let session = next_id();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let connected = connect(&gateway, session);
        assert!(
            matches!(connected.kind, EventKind::Connected { .. }),
            "connection failed: {connected:?}"
        );
        test(&gateway, session);
    }));
    disconnect(&gateway, session);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn server_browse_add_and_read() {
    with_session(|gateway, session| {
        let browse = wait_for(
            gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::Browse {
                    node_id: "i=85".to_string(),
                    filter: opcda_egui::bff::model::NodeClassFilter::All,
                    view_id: None,
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
        )
        .expect("browse timeout");
        let children = match browse.kind {
            EventKind::Browse { children, .. } => children,
            other => panic!("browse failed: {other:?}"),
        };
        assert!(
            !children.is_empty(),
            "Objects (i=85) browse returned no children"
        );

        let add = wait_for(
            gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::AddItems {
                    items: vec![
                        ItemSpec {
                            key: opcda_egui::bff::model::ItemKey::new(1).unwrap(),
                            item_id: "i=2259".to_string(),
                        },
                        ItemSpec {
                            key: opcda_egui::bff::model::ItemKey::new(2).unwrap(),
                            item_id: "i=2258".to_string(),
                        },
                    ],
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::AddItems { .. }),
        )
        .expect("add items timeout");
        let results = match add.kind {
            EventKind::AddItems { results } => results,
            other => panic!("unexpected event: {other:?}"),
        };
        for (key, result) in &results {
            assert!(result.is_ok(), "add item key {key} failed: {result:?}");
        }

        let read = wait_for(
            gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::Read {
                    keys: vec![
                        opcda_egui::bff::model::ItemKey::new(1).unwrap(),
                        opcda_egui::bff::model::ItemKey::new(2).unwrap(),
                    ],
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Values { .. }),
        )
        .expect("read timeout");
        let values = match read.kind {
            EventKind::Values { values } => values,
            other => panic!("unexpected event: {other:?}"),
        };
        assert_eq!(values.len(), 2, "expected 2 values, got {values:?}");

        for (key, dv) in &values {
            assert_eq!(
                dv.quality.kind,
                QualityKind::Good,
                "key {key}: quality not Good: {:?}",
                dv.quality
            );
            assert!(
                dv.timestamp.is_some(),
                "key {key}: missing timestamp: {dv:?}"
            );
        }

        let state = &values
            .iter()
            .find(|(k, _)| *k == opcda_egui::bff::model::ItemKey::new(1).unwrap())
            .expect("missing key 1")
            .1;
        let current_time = &values
            .iter()
            .find(|(k, _)| *k == opcda_egui::bff::model::ItemKey::new(2).unwrap())
            .expect("missing key 2")
            .1;
        assert!(
            matches!(state.value, Value::I64(_)),
            "i=2259 not I64: {:?}",
            state.value
        );
        assert!(
            matches!(current_time.value, Value::DateTime(_)),
            "i=2258 not DateTime: {:?}",
            current_time.value
        );
    });
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn server_invalid_node_read_does_not_panic() {
    with_session(|gateway, session| {
        let add = wait_for(
            gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: opcda_egui::bff::model::ItemKey::new(1).unwrap(),
                        item_id: "ns=999;s=does-not-exist".to_string(),
                    }],
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::AddItems { .. }),
        )
        .expect("add timeout");
        let _results = match add.kind {
            EventKind::AddItems { results } => results,
            other => panic!("unexpected event: {other:?}"),
        };

        let read = wait_for(
            gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::Read {
                    keys: vec![opcda_egui::bff::model::ItemKey::new(1).unwrap()],
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Values { .. }),
        )
        .expect("read timeout");
        let values = match read.kind {
            EventKind::Values { values } => values,
            other => panic!("unexpected event: {other:?}"),
        };
        let (_, dv) = values.first().expect("expected a value");
        assert_ne!(
            dv.quality.kind,
            QualityKind::Good,
            "invalid node unexpectedly read Good: {dv:?}"
        );
    });
}
