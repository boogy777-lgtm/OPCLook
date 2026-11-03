//! N2L10: real SDK write task exception after successful fixture device I/O.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    Gateway, ShutdownOutcome,
    da::{test_arm_write_exception, test_write_exception_state},
    finish_native_sdk,
};
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NativeWriteStatus,
    Origin, Owner, Protocol, RequestId, SessionId, Value, WriteDisposition, WriteRequest,
};
use std::time::{Duration, Instant};

fn send(gateway: &Gateway, id: u64, kind: CommandKind) {
    eprintln!("write exception: dispatch {id}: {kind:?}");
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
            eprintln!("write exception: event {:?} {:?}", event.origin, event.kind);
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
fn drain(gateway: &Gateway) {
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply: {event:?}"
        );
    }
}
fn read(gateway: &Gateway, id: u64, key: ItemKey, expected: i64) {
    send(gateway, id, CommandKind::Read { keys: vec![key] });
    assert!(
        matches!(reply(gateway, id).kind, EventKind::Values { values } if values.len() == 1 && values[0].0 == key && values[0].1.value == Value::I64(expected) && values[0].1.reason.is_none())
    );
}
#[test]
fn sdk_write_exception_has_one_unknown_outcome_and_no_retry() {
    let mut gateway = Gateway::new();
    let value_key = ItemKey::FIRST;
    let count_key = ItemKey::new(2).unwrap();
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
    send(
        &gateway,
        2,
        CommandKind::AddItems {
            items: vec![
                ItemSpec {
                    key: value_key,
                    item_id: "Fixture.ExceptionValue".into(),
                },
                ItemSpec {
                    key: count_key,
                    item_id: "Fixture.ExceptionCount".into(),
                },
            ],
        },
    );
    assert!(
        matches!(reply(&gateway, 2).kind, EventKind::AddItems { results } if results.len() == 2 && results.iter().all(|(_, r)| r.is_ok()))
    );
    read(&gateway, 3, value_key, 42);
    read(&gateway, 4, count_key, 0);
    assert_eq!(
        test_write_exception_state(),
        0,
        "fresh process starts unarmed"
    );
    test_arm_write_exception().unwrap();
    assert_eq!(test_write_exception_state(), 1);
    assert!(
        test_arm_write_exception().is_err(),
        "an armed injection cannot be armed again"
    );
    send(
        &gateway,
        5,
        CommandKind::Write {
            request: WriteRequest {
                key: value_key,
                value: Value::I64(77),
                type_hint: None,
            },
        },
    );
    assert!(
        matches!(reply(&gateway, 5).kind, EventKind::Wrote { key, result: Ok(outcome) }
        if key == value_key && outcome.disposition == WriteDisposition::Unknown
        && outcome.raw == Some(NativeWriteStatus::DaService(0x8000_ffffu32 as i32)))
    );
    assert_eq!(
        test_write_exception_state(),
        2,
        "SDK task consumed injection after successful write"
    );
    assert!(
        test_arm_write_exception().is_err(),
        "a consumed injection cannot be rearmed"
    );
    read(&gateway, 6, value_key, 77);
    read(&gateway, 7, count_key, 1);
    drain(&gateway);
    send(&gateway, 8, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, 8).kind,
        EventKind::Disconnected { .. }
    ));
    send(
        &gateway,
        9,
        CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        },
    );
    assert!(matches!(
        reply(&gateway, 9).kind,
        EventKind::Connected { .. }
    ));
    send(
        &gateway,
        10,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key: count_key,
                item_id: "Fixture.ExceptionCount".into(),
            }],
        },
    );
    assert!(
        matches!(reply(&gateway, 10).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    read(&gateway, 11, count_key, 1);
    send(&gateway, 12, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, 12).kind,
        EventKind::Disconnected { .. }
    ));
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(shutdown.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).unwrap();
    drain(&gateway);
}
