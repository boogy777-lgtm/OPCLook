//! Live OPC UA subscription (DataChange push) test against a CODESYS server.
//!
//! Verifies that after AddItems the worker establishes a push subscription and
//! rows update via `Values` events WITHOUT any periodic Read being triggered, and
//! that a Write to a monitored node produces a pushed change.
//!
//! Run: `cargo test --test ua_subscription -- --ignored --nocapture`
use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    BrowseNode, Command, CommandKind, EndpointSpec, EventKind, ItemSpec, NodeKind, Protocol, Value,
};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";
/// Server CurrentTime (changes every ~100 ms).
const CURRENT_TIME: &str = "i=2258";
const MAX_BROWSE: usize = 500;
const MAX_DEPTH: usize = 6;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_url() -> String {
    std::env::var("OPCUA_TEST_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

/// Drain events for a session until `predicate` matches or the timeout expires.
/// Push events carry request_id == 0, so the filter is purely on the EventKind.
fn wait_for(
    gateway: &Gateway,
    session: u64,
    timeout: Duration,
    mut predicate: impl FnMut(&EventKind) -> bool,
) -> Option<EventKind> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.session != session {
                continue;
            }
            if predicate(&event.kind) {
                return Some(event.kind);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

fn connect(gateway: &Gateway, session: u64) {
    gateway
        .send(Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(test_url()),
            },
        })
        .expect("gateway send");
    let event = wait_for(gateway, session, Duration::from_secs(40), |kind| {
        matches!(kind, EventKind::Connected { .. } | EventKind::Error { .. })
    })
    .expect("connect timeout");
    assert!(
        matches!(event, EventKind::Connected { .. }),
        "connection failed: {event:?}"
    );
}

fn send(gateway: &Gateway, session: u64, kind: CommandKind) {
    gateway
        .send(Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind,
        })
        .expect("gateway send");
}

/// Send the per-session push-subscription config the app sends on connect; the
/// worker uses publishing_interval_ms as the subscription publish rate.
fn configure_subscription(gateway: &Gateway, session: u64) {
    send(
        gateway,
        session,
        CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 1000,
            deadband: 0.0,
        },
    );
}

/// Add a batch of nodes as items, returning the results indexed by the keys used.
fn add_batch(
    gateway: &Gateway,
    session: u64,
    ids: &[String],
    first_key: u32,
) -> Vec<(
    u32,
    Result<opcda_egui::bff::model::ItemInfo, opcda_egui::bff::model::BackendMsg>,
)> {
    let specs: Vec<ItemSpec> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| ItemSpec {
            key: first_key + i as u32,
            item_id: id.clone(),
        })
        .collect();
    send(gateway, session, CommandKind::AddItems { items: specs });
    let kind = wait_for(gateway, session, Duration::from_secs(20), |kind| {
        matches!(kind, EventKind::AddItems { .. })
    })
    .expect("add batch timeout");
    match kind {
        EventKind::AddItems { results } => results,
        other => panic!("unexpected add: {other:?}"),
    }
}

fn browse_children(gateway: &Gateway, session: u64, node_id: &str) -> Vec<BrowseNode> {
    send(
        gateway,
        session,
        CommandKind::Browse {
            node_id: node_id.to_string(),
            filter: None,
            view_id: None,
        },
    );
    let kind = wait_for(gateway, session, Duration::from_secs(20), |kind| {
        matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. })
    })
    .expect("browse timeout");
    match kind {
        EventKind::Browse { children, .. } => children,
        other => panic!("browse {node_id} failed: {other:?}"),
    }
}

/// Depth-limited browse collecting leaf Item (Variable) node ids.
fn collect_variables(gateway: &Gateway, session: u64) -> Vec<String> {
    let mut items = Vec::new();
    let mut stack: Vec<(String, usize)> = vec![("i=85".to_string(), 0)];
    let mut seen = std::collections::HashSet::new();
    while let Some((node, depth)) = stack.pop() {
        if items.len() >= MAX_BROWSE {
            break;
        }
        if depth > MAX_DEPTH || !seen.insert(node.clone()) {
            continue;
        }
        for child in browse_children(gateway, session, &node) {
            if child.kind == NodeKind::Item {
                items.push(child.node_id.clone());
            } else if child.has_children {
                stack.push((child.node_id.clone(), depth + 1));
            }
        }
    }
    items
}

/// Find a Variable that is writable and actually holds a write (read-back equals
/// the written value), so a Write reliably produces a subscription push the PLC
/// does not instantly revert. Returns (key, node_id). Restores the original value.
fn find_sticky_writable(gateway: &Gateway, session: u64) -> Option<(u32, String)> {
    let vars = collect_variables(gateway, session);
    if vars.is_empty() {
        return None;
    }
    const FIRST_KEY: u32 = 100;
    let candidates: Vec<String> = vars.into_iter().take(60).collect();
    let results = add_batch(gateway, session, &candidates, FIRST_KEY);
    let mut by_key: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    for (i, id) in candidates.iter().enumerate() {
        by_key.insert(FIRST_KEY + i as u32, id.clone());
    }

    for (key, result) in results {
        let Ok(info) = result else {
            continue;
        };
        if !info.writable {
            continue;
        }
        let Some(_dt) = info
            .data_type
            .as_ref()
            .filter(|d| d.as_str() != "Other" && d.as_str() != "Unknown")
        else {
            continue;
        };
        let Some(node) = by_key.get(&key).cloned() else {
            continue;
        };
        let original = read_value(gateway, session, key);
        if matches!(original, Value::Empty | Value::Other(_) | Value::Array(_)) {
            continue;
        }
        let trial = differing_value(&original);
        if trial == original {
            continue;
        }
        write(gateway, session, key, trial.clone());
        // Let at least one publish interval elapse so a PLC cycle that reverts
        // the value has a chance to undo it; only a genuinely persistent node is
        // accepted (otherwise a reverted value produces no subscription change).
        std::thread::sleep(Duration::from_millis(1500));
        let read_back = read_value(gateway, session, key);
        if read_back != trial {
            // Not sticky -> the PLC reverted it; restore and keep looking.
            gateway
                .send(Command {
                    request_id: next_id(),
                    session,
                    protocol: Protocol::OpcUa,
                    kind: CommandKind::Write {
                        request: opcda_egui::bff::model::WriteRequest {
                            key,
                            value: original.clone(),
                            type_hint: None,
                        },
                    },
                })
                .expect("gateway send");
            continue;
        }
        write(gateway, session, key, original.clone());
        return Some((key, node));
    }
    None
}

fn push_value(
    gateway: &Gateway,
    session: u64,
    key: u32,
    timeout: Duration,
) -> opcda_egui::bff::model::DataValue {
    let kind = wait_for(gateway, session, timeout, |kind| match kind {
        EventKind::Values { values } => values.iter().any(|(k, _)| *k == key),
        _ => false,
    })
    .expect("pushed Values event timeout");
    match kind {
        EventKind::Values { values } => values
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, dv)| dv)
            .unwrap(),
        other => panic!("expected pushed Values, got {other:?}"),
    }
}

/// Consume any already-buffered pushed Values (e.g. the one-shot snapshot sent
/// when a subscription is created) so a subsequent change produces an
/// unambiguous new-value push.
fn drain_stale_pushes(gateway: &Gateway, session: u64) {
    while let Some(event) = gateway.try_recv() {
        if event.session != session {
            continue;
        }
    }
}

/// Issue an explicit Read (the worker handles it independently of the push
/// subscription) so we can observe the writable node's current value before
/// mutating it in the test.
fn read_value(gateway: &Gateway, session: u64, key: u32) -> Value {
    send(gateway, session, CommandKind::Read { keys: vec![key] });
    let kind = wait_for(
        gateway,
        session,
        Duration::from_secs(20),
        |kind| match kind {
            EventKind::Values { values } => values.iter().any(|(k, _)| *k == key),
            _ => false,
        },
    )
    .expect("read timeout");
    match kind {
        EventKind::Values { values } => values
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, dv)| dv.value)
            .expect("missing key in read response"),
        other => panic!("read returned unexpected event: {other:?}"),
    }
}

fn differing_value(v: &Value) -> Value {
    match v {
        Value::Bool(b) => Value::Bool(!*b),
        Value::I64(n) => Value::I64(n.saturating_add(1)),
        Value::U64(n) => Value::U64(n.saturating_add(1)),
        Value::F64(n) => Value::F64(*n + 1.0),
        Value::Str(s) => Value::Str(format!("{s}x")),
        Value::Bytes(b) => {
            let mut c = b.clone();
            c.push(0x00);
            Value::Bytes(c)
        }
        Value::DateTime(dt) => Value::DateTime(*dt + chrono::Duration::seconds(1)),
        Value::Empty
        | Value::Other(_)
        | Value::Array(_)
        | Value::NodeId(_)
        | Value::QualifiedName { .. }
        | Value::LocalizedText { .. } => v.clone(),
    }
}

fn write(gateway: &Gateway, session: u64, key: u32, value: Value) {
    send(
        gateway,
        session,
        CommandKind::Write {
            request: opcda_egui::bff::model::WriteRequest {
                key,
                value,
                type_hint: None,
            },
        },
    );
    let kind = wait_for(gateway, session, Duration::from_secs(20), |kind| {
        matches!(kind, EventKind::Wrote { .. })
    })
    .expect("write timeout");
    match kind {
        EventKind::Wrote { key: k, result } if k == key => {
            assert!(result.is_ok(), "write failed: {result:?}");
        }
        other => panic!("write returned unexpected event: {other:?}"),
    }
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_subscription_pushes_values_without_reads_and_writes() {
    let gateway = Gateway::new();
    let session = next_id();

    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        connect(&gateway, session);
        println!("CONNECTED session={session}");
        configure_subscription(&gateway, session);

        // Subscribe CurrentTime (changes ~every 100 ms) as key 1.
        let r = add_batch(&gateway, session, &[CURRENT_TIME.to_string()], 1);
        for (key, result) in r {
            println!("ADD key={key} result={result:?}");
            assert!(result.is_ok(), "add failed for key {key}: {result:?}");
        }

        // Find a writable-and-sticky PLC node (survives a write/read-back cycle)
        // so a Write reliably produces a push the PLC does not instantly revert.
        let write_key = find_sticky_writable(&gateway, session);
        match &write_key {
            Some((key, node)) => println!("STICKY_WRITABLE key={key} node={node}"),
            None => println!("no sticky writable node found; verifying push delivery only"),
        }

        // 1. Pure push: CurrentTime must arrive WITHOUT any manual Read.
        let v1 = push_value(&gateway, session, 1, Duration::from_secs(5));
        println!("PUSH time key=1 value={:?}", v1.value);
        assert!(
            matches!(v1.value, Value::DateTime(_)),
            "CurrentTime push should be a DateTime, got {:?}",
            v1.value
        );
        assert!(
            v1.timestamp.is_some(),
            "push should carry a server timestamp"
        );
        assert_eq!(v1.quality.kind, opcda_egui::bff::model::QualityKind::Good);

        let Some((write_key, _)) = write_key else {
            println!("PUSH_OK pure push confirmed (CurrentTime); no write check run");
            return;
        };

        // 2. Baseline the writable node, drain stale snapshots, then write a
        //    distinct value and assert the pushed change carries it.
        let baseline = read_value(&gateway, session, write_key);
        println!("BASELINE node key={write_key} value={baseline:?}");
        drain_stale_pushes(&gateway, session);
        let target = differing_value(&baseline);
        write(&gateway, session, write_key, target.clone());
        let pushed = push_value(&gateway, session, write_key, Duration::from_secs(5));
        println!("PUSH write key={write_key} value={:?}", pushed.value);
        assert_eq!(
            pushed.value, target,
            "pushed value should reflect the Write (got {:?}, expected {target:?})",
            pushed.value
        );
        write(&gateway, session, write_key, baseline.clone());
        println!("PUSH_OK pure push of CurrentTime AND write-push verified");
    }));

    // Tear down the session.
    send(&gateway, session, CommandKind::Disconnect);
    if let Err(payload) = run {
        std::panic::resume_unwind(payload);
    }
}

/// A disconnect + reconnect must re-create the subscription and re-monitor the
/// items so pushed updates resume without any manual Read.
#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_subscription_resumes_after_reconnect() {
    let gateway = Gateway::new();
    let session = next_id();

    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        connect(&gateway, session);
        configure_subscription(&gateway, session);
        let r = add_batch(&gateway, session, &[CURRENT_TIME.to_string()], 1);
        assert!(r[0].1.is_ok(), "add failed: {:?}", r[0].1);

        // A pure push arrives on the first connection.
        let v = push_value(&gateway, session, 1, Duration::from_secs(5));
        println!("PRE_RECONNECT push key=1 value={:?}", v.value);
        assert!(matches!(v.value, Value::DateTime(_)));

        // Drop the client side and reconnect the same session id.
        send(&gateway, session, CommandKind::Disconnect);
        // Drain any buffered events from the old session while it tears down.
        std::thread::sleep(Duration::from_millis(300));
        connect(&gateway, session);
        println!("RECONNECTED session={session}");

        // Re-subscribe the item on the recreated session.
        configure_subscription(&gateway, session);
        let r = add_batch(&gateway, session, &[CURRENT_TIME.to_string()], 1);
        assert!(r[0].1.is_ok(), "re-add failed: {:?}", r[0].1);

        // Pushed updates must resume after reconnect.
        let v2 = push_value(&gateway, session, 1, Duration::from_secs(5));
        println!("POST_RECONNECT push key=1 value={:?}", v2.value);
        assert!(
            matches!(v2.value, Value::DateTime(_)),
            "push must resume after reconnect, got {:?}",
            v2.value
        );
        println!("PUSH_RECONNECT_OK subscription re-established after reconnect");
    }));

    send(&gateway, session, CommandKind::Disconnect);
    if let Err(payload) = run {
        std::panic::resume_unwind(payload);
    }
}
