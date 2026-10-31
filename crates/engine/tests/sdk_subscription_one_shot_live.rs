//! Real Engine -> SDK Subscription plus one-shot read; fixture-only proof.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::{Gateway, ShutdownOutcome, finish_native_sdk};
use opclook_contract::model::{
    AcquisitionConfig, BackendMsg, Command, CommandKind, DataValue, EndpointSpec, Event, EventKind,
    ItemKey, ItemSpec, NativeWriteStatus, Origin, Owner, PollFreshness, Protocol, ReadNeed,
    ReadTerminalStatus, RequestId, SessionId, SubscriptionState, UpdateMode, Value, ValueOrigin,
    WriteDisposition, WriteRequest,
};
use opclook_engine::{Due, Engine, PendingRequest};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Seen {
    active: bool,
    first: bool,
    changed: bool,
}
fn accept(
    engine: &mut Engine<()>,
    event: &Event,
    epoch: RequestId,
    seen: &mut Seen,
) -> Option<PendingRequest<()>> {
    if event.owner == Owner::Session(SessionId::FIRST) && event.origin == Origin::Session(epoch) {
        match &event.kind {
            EventKind::SubscriptionState { state } => {
                assert!(*state != SubscriptionState::Failed, "subscription failed");
                assert!(
                    !seen.active || *state == SubscriptionState::Active,
                    "subscription stopped being active"
                );
                seen.active = *state == SubscriptionState::Active;
            }
            EventKind::Values { values } => {
                for (key, value) in values {
                    if *key == ItemKey::FIRST {
                        assert_eq!(value.quality.code, 0xc0);
                        assert_eq!(value.times.origin, ValueOrigin::Subscription);
                        if value.value == Value::I64(42) {
                            seen.first = true;
                        }
                        if value.value == Value::I64(77) {
                            seen.changed = true;
                        }
                    }
                }
            }
            EventKind::Log {
                message: BackendMsg::Raw(text),
            } if text.contains("queue overflow") || text.contains("callback queue:") => {
                panic!("callback failure: {text}")
            }
            _ => {}
        }
    }
    engine.accept(event).expect("accept engine event")
}
fn reply(
    engine: &mut Engine<()>,
    id: RequestId,
    epoch: RequestId,
    seen: &mut Seen,
) -> (Event, PendingRequest<()>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = engine.try_recv() {
            let matched = event.origin == Origin::Reply(id);
            let pending = accept(engine, &event, epoch, seen);
            if matched {
                return (event, pending.expect("correlated reply"));
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
        }
        assert!(Instant::now() < deadline, "request {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn await_push(engine: &mut Engine<()>, epoch: RequestId, seen: &mut Seen, changed: bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !(seen.active && if changed { seen.changed } else { seen.first }) {
        if let Some(event) = engine.try_recv() {
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
            accept(engine, &event, epoch, seen);
        } else {
            assert!(
                Instant::now() < deadline,
                "SDK subscription callback timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
// Exercise the actual Engine scheduling loop during the observation interval.
// Any due read is dispatched and its real SDK effect is reflected in the server counter.
fn pump_for(engine: &mut Engine<()>, epoch: RequestId, seen: &mut Seen, duration: Duration) {
    let until = Instant::now() + duration;
    let mut outstanding = HashSet::new();
    while Instant::now() < until {
        while let Some(event) = engine.try_recv() {
            if let Origin::Reply(id) = event.origin {
                assert!(outstanding.remove(&id), "unexpected reply: {event:?}");
            }
            accept(engine, &event, epoch, seen);
        }
        for task in engine.due(Instant::now(), Duration::from_millis(100), None) {
            match task {
                Due::Read {
                    session,
                    protocol,
                    keys,
                    permit,
                } => {
                    outstanding.insert(
                        engine
                            .send_due_read(session, protocol, keys, permit, ())
                            .expect("dispatch due read"),
                    );
                }
                _ => panic!("unexpected Engine due task"),
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !outstanding.is_empty() {
        if let Some(event) = engine.try_recv() {
            if let Origin::Reply(id) = event.origin {
                assert!(outstanding.remove(&id), "unexpected reply: {event:?}");
            }
            accept(engine, &event, epoch, seen);
        } else {
            assert!(Instant::now() < deadline, "outstanding due read timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
fn send(g: &Gateway, id: u64, kind: CommandKind) {
    g.send(Command {
        protocol: Protocol::OpcDa,
        owner: Owner::Session(SessionId::FIRST),
        request_id: Some(RequestId::new(id).unwrap()),
        kind,
    })
    .unwrap();
}
fn gateway_reply(g: &Gateway, id: u64) -> Event {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = g.try_recv() {
            if event.origin == Origin::Reply(RequestId::new(id).unwrap()) {
                assert_eq!(event.owner, Owner::Session(SessionId::FIRST));
                return event;
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
        }
        assert!(Instant::now() < deadline, "counter Gateway reply timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn counters(g: &Gateway, id: u64, device: ItemKey, requests: ItemKey) -> (i64, i64) {
    send(
        g,
        id,
        CommandKind::ReadCached {
            keys: vec![device, requests],
            max_age_ms: 1000,
        },
    );
    let EventKind::Values { values } = gateway_reply(g, id).kind else {
        panic!("counter values")
    };
    assert_eq!(values.len(), 2);
    let lookup = |key| {
        let v = &values
            .iter()
            .find(|(k, _)| *k == key)
            .expect("counter key")
            .1;
        assert!(v.reason.is_none());
        assert_eq!(v.quality.code, 0xc0);
        let Value::I64(n) = v.value else {
            panic!("counter type")
        };
        n
    };
    (lookup(device), lookup(requests))
}
fn read_value(event: &Event, key: ItemKey) -> &DataValue {
    let EventKind::Values { values } = &event.kind else {
        panic!("expected read")
    };
    &values.iter().find(|(k, _)| *k == key).expect("read key").1
}
#[test]
#[ignore = "requires registered Win32 Softing SDK fixture through run-interop.py"]
fn active_sdk_subscription_allows_one_fresh_read_without_hidden_poll() {
    let mut engine = Engine::<()>::default();
    let mut observer = Gateway::new();
    let mut seen = Seen::default();
    let a = SessionId::FIRST;
    let value = ItemKey::FIRST;
    let dev_count = ItemKey::new(2).unwrap();
    let request_count = ItemKey::new(3).unwrap();
    let mut endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    endpoint.acquisition = Some(AcquisitionConfig {
        update_mode: UpdateMode::Subscription,
        interval_ms: 100,
        read_after_write: false,
        poll_freshness: PollFreshness::CacheAllowed,
        da_read_method: opclook_contract::model::DaReadMethod::Sync,
    });
    let epoch = engine
        .send(a, Protocol::OpcDa, CommandKind::Connect { endpoint }, ())
        .unwrap();
    assert!(matches!(
        reply(&mut engine, epoch, epoch, &mut seen).0.kind,
        EventKind::Connected { .. }
    ));
    let id = engine
        .send(
            a,
            Protocol::OpcDa,
            CommandKind::AddItems {
                items: vec![ItemSpec {
                    key: value,
                    item_id: "Fixture.DeviceValue".into(),
                }],
            },
            (),
        )
        .unwrap();
    assert!(
        matches!(reply(&mut engine,id,epoch,&mut seen).0.kind,EventKind::AddItems {results} if results.len()==1 && results[0].1.is_ok())
    );
    await_push(&mut engine, epoch, &mut seen, false);
    send(
        &observer,
        1,
        CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        },
    );
    assert!(matches!(
        gateway_reply(&observer, 1).kind,
        EventKind::Connected { .. }
    ));
    send(
        &observer,
        2,
        CommandKind::AddItems {
            items: vec![
                ItemSpec {
                    key: value,
                    item_id: "Fixture.DeviceValue".into(),
                },
                ItemSpec {
                    key: dev_count,
                    item_id: "Fixture.SyncDeviceReadCount".into(),
                },
                ItemSpec {
                    key: request_count,
                    item_id: "Fixture.SyncReadRequestCount".into(),
                },
            ],
        },
    );
    assert!(
        matches!(gateway_reply(&observer,2).kind,EventKind::AddItems{results} if results.len()==3 && results.iter().all(|(_,result)| result.is_ok()))
    );
    // Settle any legitimate initial read and sample a baseline across 5 intervals.
    pump_for(&mut engine, epoch, &mut seen, Duration::from_millis(600));
    let baseline = counters(&observer, 3, dev_count, request_count);
    pump_for(&mut engine, epoch, &mut seen, Duration::from_millis(600));
    assert_eq!(
        counters(&observer, 4, dev_count, request_count),
        baseline,
        "subscription generated a hidden synchronous poll"
    );
    engine.request_refresh(a, value).unwrap();
    let tasks = engine.due(Instant::now(), Duration::from_millis(100), None);
    let [
        Due::Read {
            session,
            protocol,
            keys,
            permit,
        },
    ] = tasks.as_slice()
    else {
        panic!("expected one explicit read")
    };
    let id = engine
        .send_due_read(*session, *protocol, keys.clone(), *permit, ())
        .unwrap();
    let (read, pending) = reply(&mut engine, id, epoch, &mut seen);
    let v = read_value(&read, value);
    assert_eq!(v.value, Value::I64(42));
    assert_eq!(v.quality.code, 0xc0);
    assert!(v.reason.is_none());
    assert_eq!(v.times.origin, ValueOrigin::Read);
    assert!(v.times.da_timestamp.is_some());
    assert_eq!(v.times.da_timestamp, v.timestamp);
    assert_eq!(pending.read_terminals.len(), 1);
    assert!(pending.read_terminals.iter().any(|t| t.key == value
        && t.needs.contains(&ReadNeed::ExplicitRefresh)
        && matches!(t.status, ReadTerminalStatus::Received(_))));
    let after_read = counters(&observer, 5, dev_count, request_count);
    assert_eq!(after_read, (baseline.0 + 1, baseline.1 + 1));
    send(
        &observer,
        6,
        CommandKind::Write {
            request: WriteRequest {
                key: value,
                value: Value::I64(77),
                type_hint: None,
            },
        },
    );
    assert!(
        matches!(gateway_reply(&observer,6).kind,EventKind::Wrote {key,result:Ok(outcome)} if key==value && outcome.disposition==WriteDisposition::Accepted && outcome.raw==Some(NativeWriteStatus::Da(0)))
    );
    await_push(&mut engine, epoch, &mut seen, true);
    let after_push = counters(&observer, 7, dev_count, request_count);
    assert_eq!(after_push, after_read);
    // Fifteen configured intervals; no further client SyncRead, including cache-only.
    pump_for(&mut engine, epoch, &mut seen, Duration::from_millis(1500));
    assert_eq!(
        counters(&observer, 8, dev_count, request_count),
        after_push,
        "unexpected persistent synchronous poll"
    );
    let id = engine
        .send(a, Protocol::OpcDa, CommandKind::Disconnect, ())
        .unwrap();
    assert!(matches!(
        reply(&mut engine, id, epoch, &mut seen).0.kind,
        EventKind::Disconnected { .. }
    ));
    send(&observer, 9, CommandKind::Disconnect);
    assert!(matches!(
        gateway_reply(&observer, 9).kind,
        EventKind::Disconnected { .. }
    ));
    let a_shutdown = engine.shutdown();
    let b_shutdown = observer.shutdown();
    assert_eq!(a_shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(a_shutdown.ua, ShutdownOutcome::Joined);
    assert_eq!(b_shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(b_shutdown.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).unwrap();
    while let Some(event) = observer.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply: {event:?}"
        );
    }
}
