//! CODESYS live subscription evidence; no periodic/manual Read in push checks.
//! Write check requires an operator-designated disposable scalar test tag via
//! OPCUA_TEST_WRITE_NODE. Never probe arbitrary writable production PLC tags.
use opclook::bff::model::{
    Command, CommandKind, DataValue, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, Origin,
    Owner, Protocol, QualityKind, RequestId, SessionId, Value, WriteRequest,
};
use opclook_backend::bff::Gateway;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const CURRENT_TIME: &str = "i=2258";
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn next_id() -> RequestId {
    RequestId::new(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)).unwrap()
}

struct LiveSession {
    gateway: Gateway,
    owner: Owner,
    connection: RequestId,
    pending: VecDeque<Event>,
}

impl LiveSession {
    fn new() -> Self {
        let mut session = Self {
            gateway: Gateway::new(),
            owner: Owner::Session(SessionId::FIRST),
            connection: RequestId::FIRST,
            pending: VecDeque::new(),
        };
        session.connect();
        session
    }

    fn send(&self, kind: CommandKind) -> RequestId {
        let request_id = next_id();
        self.gateway
            .send(Command {
                request_id: Some(request_id),
                owner: self.owner,
                protocol: Protocol::OpcUa,
                kind,
            })
            .expect("gateway send");
        request_id
    }

    fn wait(&mut self, description: &str, mut predicate: impl FnMut(&Event) -> bool) -> Event {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(
                Instant::now() < deadline,
                "{description}: timeout; queued events (including errors, subscription state, negotiated parameters and unmatched pushes): {:?}",
                self.pending
            );
            if let Some(index) = self.pending.iter().position(&mut predicate) {
                return self.pending.remove(index).unwrap();
            }
            if let Some(event) = self.gateway.try_recv() {
                if event.owner == self.owner {
                    assert!(
                        self.pending.len() < 4096,
                        "test event backlog exceeded: {:?}",
                        self.pending
                    );
                    self.pending.push_back(event);
                }
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn reply(&mut self, request: RequestId) -> EventKind {
        self.wait("correlated reply", |event| {
            event.origin == Origin::Reply(request)
        })
        .kind
    }

    fn connect(&mut self) {
        let url = std::env::var("OPCUA_TEST_URL")
            .unwrap_or_else(|_| "opc.tcp://127.0.0.1:4840".to_string());
        assert!(
            url == "opc.tcp://127.0.0.1:4840",
            "only the approved numeric loopback endpoint is allowed"
        );
        self.connection = self.send(CommandKind::Connect {
            endpoint: EndpointSpec::ua(url),
        });
        let reply = self.reply(self.connection);
        assert!(
            matches!(reply, EventKind::Connected { .. }),
            "connect failed: {reply:?}"
        );
        self.send(CommandKind::ConfigureUaSubscription {
            publishing_interval_ms: 1000,
            deadband: 0.0,
        });
    }

    fn add(&mut self, key: ItemKey, node: &str, require_writable: bool) {
        let request = self.send(CommandKind::AddItems {
            items: vec![ItemSpec {
                key,
                item_id: node.to_string(),
            }],
        });
        match self.reply(request) {
            EventKind::AddItems { results } => {
                assert_eq!(results.len(), 1);
                let (actual, info) = results.into_iter().next().unwrap();
                assert_eq!(actual, key);
                let info = info.expect("AddItems failed");
                assert!(
                    !require_writable || info.writable,
                    "designated test node is not writable: {node}"
                );
            }
            other => panic!("AddItems failed: {other:?}"),
        }
    }

    fn push(&mut self, key: ItemKey, target: Option<&Value>) -> DataValue {
        let connection = self.connection;
        let event = self.wait("matching push (no Read issued)", |event| {
            let current = match event.origin {
                Origin::UaSession {
                    connection: actual, ..
                }
                | Origin::Session(actual) => actual == connection,
                _ => false,
            };
            current
                && match &event.kind {
                    EventKind::Values { values } => values.iter().any(|(actual, value)| {
                        *actual == key && target.is_none_or(|target| value.value == *target)
                    }),
                    _ => false,
                }
        });
        match event.kind {
            EventKind::Values { values } => {
                values
                    .into_iter()
                    .find(|(actual, _)| *actual == key)
                    .unwrap()
                    .1
            }
            _ => unreachable!(),
        }
    }

    fn read(&mut self, key: ItemKey) -> Value {
        let request = self.send(CommandKind::Read { keys: vec![key] });
        match self.reply(request) {
            EventKind::Values { values } => {
                let value = values
                    .into_iter()
                    .find(|(actual, _)| *actual == key)
                    .expect("Read missing key")
                    .1;
                assert_eq!(
                    value.quality.kind,
                    QualityKind::Good,
                    "baseline/restore Read is not Good: {value:?}"
                );
                value.value
            }
            other => panic!("Read failed: {other:?}"),
        }
    }

    fn write(&mut self, key: ItemKey, value: Value) {
        let request = self.send(CommandKind::Write {
            request: WriteRequest {
                key,
                value,
                type_hint: None,
            },
        });
        match self.reply(request) {
            EventKind::Wrote {
                key: actual,
                result,
            } => {
                assert_eq!(actual, key);
                assert!(
                    result.as_ref().is_ok_and(|outcome| outcome.accepted()),
                    "Write failed: {result:?}"
                );
            }
            other => panic!("Write failed: {other:?}"),
        }
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        // No second panic on cleanup. Restoration below happens before this.
        let _ = self.gateway.send(Command {
            request_id: Some(next_id()),
            owner: self.owner,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        });
    }
}

/// Always run synchronous, acknowledged restore + readback, including when the
/// body panics. A lost runtime cannot guarantee physical restoration: report both
/// failures prominently and never claim cleanup success in that case.
fn with_restore<T>(state: &mut T, body: impl FnOnce(&mut T), restore: impl FnOnce(&mut T)) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(state)));
    let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| restore(state)));
    if let Err(payload) = cleanup {
        eprintln!(
            "RESTORE FAILED: operator must verify designated test tag; original test failed={}",
            result.is_err()
        );
        std::panic::resume_unwind(payload);
    }
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn differing_value(value: &Value) -> Value {
    match value {
        Value::Bool(value) => Value::Bool(!value),
        Value::I64(value) => Value::I64(if *value == 0 { 1 } else { 0 }),
        Value::U64(value) => Value::U64(if *value == 0 { 1 } else { 0 }),
        Value::F64(value) if value.is_finite() => Value::F64(if *value == 0.0 { 1.0 } else { 0.0 }),
        _ => panic!("designated tag must be a disposable numeric/boolean scalar, got {value:?}"),
    }
}

#[test]
fn restore_runs_before_propagating_assertion_failure() {
    let mut value = 7;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_restore(
            &mut value,
            |value| {
                *value = 9;
                panic!("injected write-push failure");
            },
            |value| *value = 7,
        );
    }));
    assert!(result.is_err());
    assert_eq!(value, 7);
}

#[test]
fn failed_restore_is_not_success() {
    let result = std::panic::catch_unwind(|| {
        with_restore(&mut (), |_| {}, |_| panic!("injected restore failure"))
    });
    assert!(result.is_err());
}

#[test]
fn scalar_targets_are_distinct_and_bounded() {
    for value in [
        Value::Bool(false),
        Value::Bool(true),
        Value::I64(i64::MAX),
        Value::I64(i64::MIN),
        Value::I64(0),
        Value::U64(u64::MAX),
        Value::U64(0),
        Value::F64(0.0),
        Value::F64(f64::MAX),
    ] {
        assert_ne!(differing_value(&value), value);
    }
}

#[test]
#[ignore = "requires CODESYS at 127.0.0.1:4840; read-only push check"]
fn ua_subscription_pushes_current_time_without_reads() {
    let mut session = LiveSession::new();
    session.add(ItemKey::FIRST, CURRENT_TIME, false);
    let value = session.push(ItemKey::FIRST, None);
    assert!(matches!(value.value, Value::DateTime(_)));
    assert!(value.timestamp.is_some());
    assert_eq!(value.quality.kind, QualityKind::Good);
}

/// N5/O01: real CODESYS requested/revised facts, not inferred from a push rate.
/// This does not establish differing revised counters, overflow or recovery.
#[test]
#[ignore = "requires CODESYS at 127.0.0.1:4840; read-only negotiated facts"]
fn ua_subscription_reports_codesys_negotiated_parameters() {
    let mut session = LiveSession::new();
    session.add(ItemKey::FIRST, CURRENT_TIME, false);
    let connection = session.connection;
    let event = session.wait("negotiated subscription/item facts", |event| {
        matches!(event.origin, Origin::UaSession { connection: actual, .. } if actual == connection)
            && matches!(&event.kind, EventKind::UaSubscriptionParameters { subscriptions }
                if subscriptions.iter().any(|subscription| subscription.items.iter().any(|item| item.key == ItemKey::FIRST)))
    });
    let EventKind::UaSubscriptionParameters { subscriptions } = event.kind else {
        unreachable!();
    };
    let subscription = subscriptions
        .iter()
        .find(|subscription| {
            subscription
                .items
                .iter()
                .any(|item| item.key == ItemKey::FIRST)
        })
        .unwrap();
    println!("CODESYS_NEGOTIATED {subscription:?}");
    assert_ne!(subscription.subscription_id, 0);
    assert_eq!(subscription.requested_publishing_ms, 1000);
    assert!(subscription.requested_keepalive > 0);
    assert!(
        subscription.requested_lifetime >= subscription.requested_keepalive.checked_mul(3).unwrap()
    );
    let revised = subscription
        .revised
        .as_ref()
        .expect("SDK revised subscription state missing");
    assert!(revised.valid, "invalid SDK revised facts: {revised:?}");
    let item = subscription
        .items
        .iter()
        .find(|item| item.key == ItemKey::FIRST)
        .unwrap();
    assert!(item.requested_sampling_ms.is_finite());
    assert!(item.requested_queue_size > 0);
    assert!(item.revised_sampling_ms.is_finite() && item.revised_sampling_ms >= 0.0);
    assert!(item.revised_queue_size > 0);
    assert!(matches!(
        session.push(ItemKey::FIRST, None).value,
        Value::DateTime(_)
    ));
}

/// Explicitly authorized CMD only; state is observed, never written. This short
/// pulse proves restore/readback, not guaranteed sampling of a transient value.
#[test]
#[ignore = "requires explicit OPCUA_TEST_WRITE_NODE authorization for Cmd_ChokePanelActive"]
fn ua_cmd_pair_minimal_write_restore_and_observe() {
    const CMD: &str = "ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.Cmd_ChokePanelActive";
    const STATE: &str = "ns=4;s=|var|CODESYS Control Win V3 x64.Application.GVL_Emulator_Additions.ChokePanelActiveSwitch";
    assert_eq!(
        std::env::var("OPCUA_TEST_WRITE_NODE").as_deref(),
        Ok(CMD),
        "explicit authorization for this CMD is required"
    );
    let mut session = LiveSession::new();
    let cmd_key = ItemKey::FIRST;
    let state_key = ItemKey::new(2).unwrap();
    session.add(cmd_key, CMD, true);
    session.add(state_key, STATE, false);
    let baseline = session.read(cmd_key);
    let state_before = session.read(state_key);
    let target = differing_value(&baseline);
    println!(
        "CMD_PAIR_BASELINE cmd={CMD} value={baseline:?} quality=Good state={STATE} value={state_before:?} quality=Good target={target:?}"
    );
    let started = Instant::now();
    with_restore(
        &mut session,
        |session| {
            session.write(cmd_key, target.clone());
            println!(
                "CMD_PAIR_WRITE_ACCEPTED target={target:?} elapsed={:?}",
                started.elapsed()
            );
            // No intervening Read, sleep or push wait before the restore Write.
        },
        |session| {
            session.write(cmd_key, baseline.clone());
            println!(
                "CMD_PAIR_RESTORE_ACCEPTED baseline={baseline:?} elapsed={:?}",
                started.elapsed()
            );
            assert_eq!(
                session.read(cmd_key),
                baseline,
                "CMD_PAIR_RESTORE readback mismatch"
            );
            println!("CMD_PAIR_RESTORE_READBACK_OK value={baseline:?} quality=Good");
        },
    );
    // Record state notifications, including possibly the initial snapshot;
    // no claim that CMD causes a particular transition or is sampled atomically.
    let deadline = Instant::now() + Duration::from_millis(1100);
    while Instant::now() < deadline {
        if let Some(event) = session.gateway.try_recv() {
            if event.owner == session.owner {
                assert!(session.pending.len() < 4096, "test event backlog exceeded");
                session.pending.push_back(event);
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    for event in &session.pending {
        if let EventKind::Values { values } = &event.kind {
            for (key, value) in values {
                if *key == state_key {
                    println!(
                        "CMD_PAIR_STATE_QUEUED origin={:?} data={value:?}; may include initial snapshot",
                        event.origin
                    );
                }
            }
        }
    }
    println!(
        "CMD_PAIR_STATE_AFTER value={:?} quality=Good",
        session.read(state_key)
    );
}

#[test]
#[ignore = "requires CODESYS and operator-designated disposable OPCUA_TEST_WRITE_NODE"]
fn ua_subscription_pushes_values_without_reads_and_writes() {
    let node = std::env::var("OPCUA_TEST_WRITE_NODE").ok().filter(|node| !node.trim().is_empty())
        .expect("MISSING_TEST_STAND: OPCUA_TEST_WRITE_NODE must name an operator-approved disposable writable scalar; arbitrary PLC tag probing is forbidden");
    let mut session = LiveSession::new();
    let key = ItemKey::FIRST;
    session.add(key, &node, true);
    let baseline = session.read(key);
    let target = differing_value(&baseline);
    // Wait until the baseline is actually delivered to the subscription before
    // mutating. No qualification trial Write, hence no repeat/coalescing race.
    session.push(key, Some(&baseline));
    println!("WRITE_PUSH node={node} baseline={baseline:?} target={target:?}");
    with_restore(
        &mut session,
        |session| {
            session.write(key, target.clone());
            let pushed = session.push(key, Some(&target));
            assert_eq!(pushed.quality.kind, QualityKind::Good);
            assert!(pushed.timestamp.is_some());
            println!("WRITE_PUSH_OK {pushed:?}");
        },
        |session| {
            session.write(key, baseline.clone());
            assert_eq!(
                session.read(key),
                baseline,
                "RESTORE readback mismatch for {node}"
            );
            println!("RESTORE_OK node={node} baseline={baseline:?}");
        },
    );
}

#[test]
#[ignore = "requires CODESYS at 127.0.0.1:4840"]
fn ua_subscription_resumes_after_reconnect() {
    let mut session = LiveSession::new();
    session.add(ItemKey::FIRST, CURRENT_TIME, false);
    assert!(matches!(
        session.push(ItemKey::FIRST, None).value,
        Value::DateTime(_)
    ));
    let request = session.send(CommandKind::Disconnect);
    assert!(matches!(
        session.reply(request),
        EventKind::Disconnected { .. }
    ));
    session.pending.clear();
    session.connect();
    session.add(ItemKey::FIRST, CURRENT_TIME, false);
    assert!(matches!(
        session.push(ItemKey::FIRST, None).value,
        Value::DateTime(_)
    ));
}
