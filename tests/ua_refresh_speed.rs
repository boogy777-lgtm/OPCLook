//! Live check for BUG 1: after several items are subscribed, an «Обновить»
//! (Browse "") command must not wait behind queued periodic Read commands.
//! Requires a live OPC UA server (default CODESYS at 127.0.0.1:4840).
use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, Protocol,
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
    let started = Instant::now();
    gateway.send(command).expect("gateway send");
    let deadline = started + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.origin != request_id.into() || event.session != session {
                continue;
            }
            if predicate(&event.kind) {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn connect(gateway: &Gateway, session: u64) {
    let connected = wait_for(
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
    .expect("connect timeout");
    assert!(matches!(connected.kind, EventKind::Connected { .. }));
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

const CODESYS_VAR: &str =
    "ns=4;s=|var|CODESYS Control Win V3 x64.Application.StatusDB.ActiveWHPSensor";

/// A Browse "" request must complete quickly even when recurring Read commands
/// are queued for subscribed items.
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn browse_refresh_jumps_over_queued_reads() {
    let gateway = Gateway::new();
    let session = next_id();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        connect(&gateway, session);

        // Subscribe ~10 items: a known writable CODESYS variable plus standard
        // nodes; repeat the standard nodes to reach 10 subscriptions.
        let mut items = Vec::new();
        items.push(ItemSpec {
            key: opcda_egui::bff::model::ItemKey::FIRST,
            item_id: CODESYS_VAR.to_string(),
        });
        for i in 0..9 {
            items.push(ItemSpec {
                key: opcda_egui::bff::model::ItemKey::new(2 + i as u32).unwrap(),
                item_id: if i % 2 == 0 {
                    "i=2259".to_string()
                } else {
                    "i=2258".to_string()
                },
            });
        }
        let add = wait_for(
            &gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::AddItems { items },
            },
            Duration::from_secs(30),
            |kind| matches!(kind, EventKind::AddItems { .. }),
        )
        .expect("add items timeout");
        let results = match add.kind {
            EventKind::AddItems { results } => results,
            other => panic!("unexpected event: {other:?}"),
        };
        let ok = results.iter().filter(|(_, r)| r.is_ok()).count();
        assert!(
            ok >= 10,
            "expected 10 subscribed items, got {ok}: {results:?}"
        );

        // Send two periodic Read commands and drain their Values so the state
        // is warm, leaving subsequent reads queued behind the browse.
        for _ in 0..2 {
            let read = wait_for(
                &gateway,
                Command {
                    request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                    session,
                    protocol: Protocol::OpcUa,
                    kind: CommandKind::Read {
                        keys: (1..=10)
                            .map(|key| opcda_egui::bff::model::ItemKey::new(key).unwrap())
                            .collect(),
                    },
                },
                Duration::from_secs(30),
                |kind| matches!(kind, EventKind::Values { .. }),
            )
            .expect("periodic read timeout");
            assert!(matches!(read.kind, EventKind::Values { .. }));
        }

        // Send a Browse "" (refresh) while reads are being issued periodically.
        let t0 = Instant::now();
        let browse = wait_for(
            &gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::Browse {
                    node_id: String::new(),
                    filter: opcda_egui::bff::model::NodeClassFilter::All,
                    view_id: None,
                },
            },
            Duration::from_secs(5),
            |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
        )
        .expect("browse did not arrive within 5s");
        let elapsed = t0.elapsed();
        match browse.kind {
            EventKind::Browse { children, .. } => {
                assert!(!children.is_empty(), "root browse returned no children");
            }
            other => panic!("browse failed: {other:?}"),
        }
        println!("browse (Обновить) arrived in {elapsed:?} with 10 subscribed items");
        assert!(
            elapsed < Duration::from_secs(5),
            "refresh must not wait for queued reads; took {elapsed:?}"
        );
    }));

    disconnect(&gateway, session);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
