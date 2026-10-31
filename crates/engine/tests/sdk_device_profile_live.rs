//! Real Engine -> Gateway -> SDK fixture; run under run-interop.py on Win32.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::{ShutdownOutcome, finish_native_sdk};
use opclook_contract::model::{
    AcquisitionConfig, CommandKind, DataValue, EndpointSpec, Event, EventKind, ItemKey, ItemSpec,
    NativeWriteStatus, Origin, Protocol, ReadNeed, ReadTerminalStatus, RequestId, SessionId, Value,
    ValueOrigin, WriteDisposition, WriteRequest,
};
use opclook_engine::{Due, Engine, PendingRequest};
use std::time::{Duration, Instant};

fn reply(engine: &mut Engine<()>, id: RequestId) -> (Event, PendingRequest<()>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = engine.try_recv() {
            let matched = event.origin == Origin::Reply(id);
            let pending = engine.accept(&event).expect("accepted event");
            if matched {
                return (event, pending.expect("correlated reply"));
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply {event:?}"
            );
        }
        assert!(Instant::now() < deadline, "request {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn due(engine: &mut Engine<()>, now: Instant) -> RequestId {
    let tasks = engine.due(now, Duration::from_secs(60), None);
    let [
        Due::Read {
            session,
            protocol,
            keys,
            permit,
        },
    ] = tasks.as_slice()
    else {
        panic!("expected one read")
    };
    engine
        .send_due_read(*session, *protocol, keys.clone(), *permit, ())
        .expect("dispatch due")
}
fn item(event: &Event, key: ItemKey) -> &DataValue {
    let EventKind::Values { values } = &event.kind else {
        panic!("expected values: {event:?}")
    };
    &values
        .iter()
        .find(|(k, _)| *k == key)
        .expect("missing item")
        .1
}
fn integer(event: &Event, key: ItemKey) -> i64 {
    let value = item(event, key);
    assert!(value.reason.is_none());
    assert_eq!(value.quality.code, 0xc0);
    assert_eq!(value.times.origin, ValueOrigin::Read);
    assert_eq!(value.times.da_timestamp, value.timestamp);
    assert!(value.times.da_timestamp.is_some());
    assert!(value.times.received_at.is_some());
    assert!(value.times.ua_source.is_none() && value.times.ua_server.is_none());
    assert!(value.times.source_clock.is_none());
    let Value::I64(n) = value.value else {
        panic!("expected I4")
    };
    n
}
#[test]
#[ignore = "requires registered Win32 Softing SDK fixture through run-interop.py"]
fn sdk_engine_cache_refresh_and_profile_readback() {
    let mut engine = Engine::<()>::default();
    let session = SessionId::FIRST;
    let value = ItemKey::FIRST;
    let count = ItemKey::new(2).unwrap();
    let mut endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    let mut profile = AcquisitionConfig::da_available();
    profile.read_after_write = true;
    endpoint.acquisition = Some(profile);
    let id = engine
        .send(
            session,
            Protocol::OpcDa,
            CommandKind::Connect { endpoint },
            (),
        )
        .unwrap();
    assert!(matches!(
        reply(&mut engine, id).0.kind,
        EventKind::Connected { .. }
    ));
    let id = engine
        .send(
            session,
            Protocol::OpcDa,
            CommandKind::AddItems {
                items: vec![
                    ItemSpec {
                        key: value,
                        item_id: "Fixture.DeviceValue".into(),
                    },
                    ItemSpec {
                        key: count,
                        item_id: "Fixture.DeviceReadCount".into(),
                    },
                ],
            },
            (),
        )
        .unwrap();
    assert!(
        matches!(reply(&mut engine, id).0.kind, EventKind::AddItems { results } if results.len() == 2 && results.iter().all(|(_, result)| result.is_ok()))
    );
    let now = Instant::now();
    let id = due(&mut engine, now);
    let (first, _) = reply(&mut engine, id);
    assert_eq!(integer(&first, value), 42);
    let initial = integer(&first, count);
    let id = due(&mut engine, now + Duration::from_secs(2));
    let (cached, _) = reply(&mut engine, id);
    assert_eq!(integer(&cached, count), initial);
    engine.request_refresh(session, value).unwrap();
    let id = due(&mut engine, now + Duration::from_secs(2));
    let (_, terminal) = reply(&mut engine, id);
    assert!(terminal.read_terminals.iter().any(|t| t.key == value
        && t.needs.contains(&ReadNeed::ExplicitRefresh)
        && matches!(t.status, ReadTerminalStatus::Received(_))));
    let id = due(&mut engine, now + Duration::from_secs(4));
    let (after_refresh, _) = reply(&mut engine, id);
    assert_eq!(integer(&after_refresh, count), initial + 1);
    let id = engine
        .send(
            session,
            Protocol::OpcDa,
            CommandKind::Write {
                request: WriteRequest {
                    key: value,
                    value: Value::I64(77),
                    type_hint: None,
                },
            },
            (),
        )
        .unwrap();
    assert!(
        matches!(reply(&mut engine, id).0.kind, EventKind::Wrote { result: Ok(result), .. } if result.disposition == WriteDisposition::Accepted && result.raw == Some(NativeWriteStatus::Da(0)))
    );
    let id = due(&mut engine, now + Duration::from_secs(4));
    let (readback, terminal) = reply(&mut engine, id);
    assert_eq!(integer(&readback, value), 77);
    assert!(terminal.read_terminals.iter().any(|t| t.key == value
        && t.needs.contains(&ReadNeed::WriteReadback)
        && matches!(t.status, ReadTerminalStatus::Received(_))));
    let id = due(&mut engine, now + Duration::from_secs(6));
    let (after_write, _) = reply(&mut engine, id);
    assert_eq!(integer(&after_write, count), initial + 2);
    let id = engine
        .send(session, Protocol::OpcDa, CommandKind::Disconnect, ())
        .unwrap();
    assert!(matches!(
        reply(&mut engine, id).0.kind,
        EventKind::Disconnected { .. }
    ));
    let shutdown = engine.shutdown();
    assert_eq!(shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(shutdown.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).unwrap();
}
