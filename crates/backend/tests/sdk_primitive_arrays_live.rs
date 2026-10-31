//! Strict SDK fixture array coverage: direct fixed-width arrays only.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::{Gateway, ShutdownOutcome, finish_native_sdk};
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NativeWriteStatus,
    Origin, Owner, Protocol, RequestId, SessionId, Value, VarType, WriteDisposition, WriteRequest,
};
use std::time::{Duration, Instant};
fn send(gateway: &Gateway, id: u64, kind: CommandKind) {
    eprintln!("device read: dispatch {id}: {kind:?}");
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Session(SessionId::FIRST),
            request_id: Some(RequestId::new(id).unwrap()),
            kind,
        })
        .unwrap();
}
fn reply(gateway: &Gateway, id: u64) -> Event {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = gateway.try_recv() {
            eprintln!("device read: event {:?} {:?}", event.origin, event.kind);
            if event.origin == Origin::Reply(RequestId::new(id).unwrap()) {
                assert_eq!(event.owner, Owner::Session(SessionId::FIRST));
                return event;
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
        }
        assert!(Instant::now() < deadline, "reply {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn read(g: &Gateway, id: u64, key: ItemKey, expected: &Value) {
    send(g, id, CommandKind::ReadFresh { keys: vec![key] });
    let EventKind::Values { values } = reply(g, id).kind else {
        panic!("expected read values")
    };
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].0, key);
    let value = &values[0].1;
    assert_eq!(&value.value, expected);
    assert!(value.reason.is_none());
    assert_eq!(value.quality.code, 0xc0);
    assert!(value.timestamp.is_some());
    assert_eq!(value.times.da_timestamp, value.timestamp);
}
#[test]
fn sdk_primitive_arrays_exact_read_write_and_readback() {
    let mut gateway = Gateway::new();
    send(
        &gateway,
        1,
        CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        },
    );
    assert!(matches!(
        reply(&gateway, 1).kind,
        EventKind::Connected { .. }
    ));
    let cases = [
        (
            "Fixture.Float64Array",
            0x2005,
            Value::Array(vec![Value::F64(-1.25), Value::F64(0.5), Value::F64(42.0)]),
            Value::Array(vec![Value::F64(2.5), Value::F64(-17.75)]),
        ),
        (
            "Fixture.BoolArray",
            0x200b,
            Value::Array(vec![
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(false),
            ]),
            Value::Array(vec![Value::Bool(true), Value::Bool(false)]),
        ),
        (
            "Fixture.Int64Array",
            0x2014,
            Value::Array(vec![
                Value::I64(i64::MIN),
                Value::I64(-9_007_199_254_740_993),
                Value::I64(i64::MAX),
            ]),
            Value::Array(vec![Value::I64(i64::MAX), Value::I64(i64::MIN)]),
        ),
        (
            "Fixture.UInt64Array",
            0x2015,
            Value::Array(vec![
                Value::U64(0),
                Value::U64(9_007_199_254_740_993),
                Value::U64(u64::MAX),
            ]),
            Value::Array(vec![Value::U64(u64::MAX), Value::U64(17)]),
        ),
        (
            "Fixture.Float64ArrayReadOnly",
            0x2005,
            Value::Array(vec![Value::F64(-1.25), Value::F64(0.5), Value::F64(42.0)]),
            Value::Empty,
        ),
    ];
    let specs = cases
        .iter()
        .enumerate()
        .map(|(i, (id, _, _, _))| ItemSpec {
            key: ItemKey::new((i + 1) as u32).unwrap(),
            item_id: (*id).into(),
        })
        .collect();
    send(&gateway, 2, CommandKind::AddItems { items: specs });
    let EventKind::AddItems { results } = reply(&gateway, 2).kind else {
        panic!("expected add items")
    };
    assert_eq!(results.len(), cases.len());
    for (index, (_, vt, _, _)) in cases.iter().enumerate() {
        assert_eq!(results[index].0, ItemKey::new((index + 1) as u32).unwrap());
        let info = results[index].1.as_ref().unwrap();
        assert_eq!(info.type_id, Some(*vt));
        assert_eq!(info.writable, index < 4);
    }
    for (index, (_, vt, initial, written)) in cases.iter().enumerate() {
        let key = ItemKey::new((index + 1) as u32).unwrap();
        let base = 10 + index as u64 * 4;
        read(&gateway, base, key, initial);
        if index == 4 {
            continue;
        }
        send(
            &gateway,
            base + 1,
            CommandKind::Write {
                request: WriteRequest {
                    key,
                    value: written.clone(),
                    type_hint: Some(VarType(*vt)),
                },
            },
        );
        assert!(
            matches!(reply(&gateway, base+1).kind, EventKind::Wrote { key: k, result: Ok(outcome) } if k == key && outcome.disposition == WriteDisposition::Accepted && outcome.raw == Some(NativeWriteStatus::Da(0)))
        );
        read(&gateway, base + 2, key, written);
    }
    let read_only = ItemKey::new(5).unwrap();
    send(
        &gateway,
        32,
        CommandKind::Write {
            request: WriteRequest {
                key: read_only,
                value: Value::Array(vec![Value::F64(123.0)]),
                type_hint: Some(VarType(0x2005)),
            },
        },
    );
    match reply(&gateway, 32).kind {
        EventKind::Wrote { key, result } => {
            assert_eq!(key, read_only);
            assert!(
                !matches!(result, Ok(outcome) if outcome.disposition == WriteDisposition::Accepted)
            );
        }
        other => panic!("unexpected read-only write: {other:?}"),
    }
    read(&gateway, 33, read_only, &cases[4].2);
    // Invalid I8 is rejected before dispatch and leaves the prior value intact.
    let key = ItemKey::new(3).unwrap();
    send(
        &gateway,
        40,
        CommandKind::Write {
            request: WriteRequest {
                key,
                value: Value::Array(vec![Value::U64(u64::MAX)]),
                type_hint: Some(VarType(0x2014)),
            },
        },
    );
    assert!(
        matches!(reply(&gateway, 40).kind, EventKind::Wrote { key: k, result: Err(_) } if k == key)
    );
    read(&gateway, 41, key, &cases[2].3);
    send(&gateway, 42, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, 42).kind,
        EventKind::Disconnected { .. }
    ));
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(shutdown.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).unwrap();
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply: {event:?}"
        );
    }
}
