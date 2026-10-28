//! Live (ignored) test for OPC UA client lifecycle teardown: several
//! connect/browse/add/disconnect cycles plus one-shot searches, asserting a
//! `Disconnected` event every cycle and no hang. Requires an OPC UA server on
//! 127.0.0.1:4840 (CODESYS).
//!
//! Run:  cargo test --test ua_lifecycle -- --ignored --nocapture
//!
//! After the run the test process must not leave an ESTABLISHED TCP connection
//! to port 4840 (verify with `netstat -ano | findstr 4840`). The worker's
//! graceful-close semantics (Session::disconnect -> CloseSession +
//! CloseSecureChannel, then wait for the session event loop) are what close the
//! sockets, so the observable socket count is the regression signal.
use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, Protocol,
};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";
const CYCLES: u32 = 5;
const SEARCHES: u32 = 3;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_url() -> String {
    std::env::var("OPCUA_TEST_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

/// Send `command`, then poll events until one with the same request_id/session
/// matches `predicate` or the timeout elapses. Other events are drained so the
/// worker never blocks on a full channel.
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
        std::thread::sleep(Duration::from_millis(20));
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
    .expect("connect timeout (server up on 127.0.0.1:4840?)")
}

fn disconnect(gateway: &Gateway, session: u64) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        // Disconnect awaits the graceful CloseSession + event-loop shutdown, but
        // must never hang; 10 s is far more than the 5 s graceful budget.
        Duration::from_secs(10),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    )
    .expect("disconnect timeout: worker hung during graceful teardown")
}

/// One full connect -> browse -> add items -> read -> disconnect cycle.
///
/// The Read exercises the chunked read path (`READ_MAX_ITEMS_PER_CALL`) and must
/// return every requested key, so a batched read cannot silently drop the tail
/// of a request before the graceful teardown.
fn run_cycle(gateway: &Gateway, session: u64, cycle: u32) {
    let connected = connect(gateway, session);
    assert!(
        matches!(connected.kind, EventKind::Connected { .. }),
        "cycle {cycle}: connection failed: {connected:?}"
    );

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
    .expect("cycle {cycle}: browse timeout");
    match browse.kind {
        EventKind::Browse { children, .. } => {
            assert!(
                !children.is_empty(),
                "cycle {cycle}: Objects (i=85) browse empty"
            );
        }
        other => panic!("cycle {cycle}: browse failed: {other:?}"),
    }

    // Keys unique per cycle so event matching never crosses runs.
    let k1 = opcda_egui::bff::model::ItemKey::new((cycle * 2 + 1) as u32).unwrap();
    let k2 = opcda_egui::bff::model::ItemKey::new((cycle * 2 + 2) as u32).unwrap();
    let add = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems {
                items: vec![
                    ItemSpec {
                        key: k1,
                        item_id: "i=2259".to_string(),
                    },
                    ItemSpec {
                        key: k2,
                        item_id: "i=2258".to_string(),
                    },
                ],
            },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::AddItems { .. } | EventKind::Error { .. }),
    )
    .expect("cycle {cycle}: add items timeout");
    match add.kind {
        EventKind::AddItems { results } => {
            for (key, result) in &results {
                assert!(
                    result.is_ok(),
                    "cycle {cycle}: add item key {key} failed: {result:?}"
                );
            }
        }
        other => panic!("cycle {cycle}: add items failed: {other:?}"),
    }

    // A Read command must return every requested key in caller order even though
    // large reads are split into `READ_MAX_ITEMS_PER_CALL` service chunks.
    let read = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Read { keys: vec![k1, k2] },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::Values { .. } | EventKind::Error { .. }),
    )
    .expect("cycle {cycle}: read timeout");
    match read.kind {
        EventKind::Values { values } => {
            assert_eq!(
                values.len(),
                2,
                "cycle {cycle}: read returned {} values for 2 keys",
                values.len()
            );
            assert_eq!(values[0].0, k1, "cycle {cycle}: read order changed");
            assert_eq!(values[1].0, k2, "cycle {cycle}: read order changed");
        }
        other => panic!("cycle {cycle}: read failed: {other:?}"),
    }

    // Disconnect must report Disconnected and the whole cycle must not hang.
    let disconnected = disconnect(gateway, session);
    assert!(
        matches!(disconnected.kind, EventKind::Disconnected { .. }),
        "cycle {cycle}: unexpected event: {disconnected:?}"
    );
}

/// One-shot discovery search (`FindServers` + `GetEndpoints`): returns and its
/// own per-call channel is closed gracefully by the library. Must complete.
fn run_search(gateway: &Gateway, session: u64, index: u32) {
    let found = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::SearchUaServers {
                host: "127.0.0.1".to_string(),
                port: 4840,
                bind: None,
            },
        },
        Duration::from_secs(60),
        |kind| {
            matches!(
                kind,
                EventKind::UaServersFound { .. } | EventKind::Error { .. }
            )
        },
    )
    .unwrap_or_else(|| panic!("search {index}: timeout"));
    match found.kind {
        EventKind::UaServersFound { servers, .. } => {
            assert!(
                !servers.is_empty(),
                "search {index}: no servers found on 127.0.0.1"
            );
        }
        EventKind::Error { message, .. } => panic!("search {index}: failed: {message:?}"),
        other => panic!("search {index}: unexpected event: {other:?}"),
    }
}

/// 5 full connect/browse/add/disconnect cycles + 3 one-shot searches, all over
/// one shared worker (thread + tokio runtime), then a final drain of the event
/// queue. The graceful-close paths must close every TCP channel to the server.
#[test]
#[ignore = "requires OPC UA server at 127.0.0.1:4840"]
fn ua_lifecycle_two_dozen_connections_close_cleanly() {
    let gateway = Gateway::new();
    let mut session = next_id();

    for cycle in 1..=CYCLES {
        run_cycle(&gateway, session, cycle);
        session = next_id();
    }

    for index in 1..=SEARCHES {
        run_search(&gateway, session, index);
        session = next_id();
    }

    // Drain any trailing push/subscription events so nothing is left queued.
    while gateway.try_recv().is_some() {}
}
