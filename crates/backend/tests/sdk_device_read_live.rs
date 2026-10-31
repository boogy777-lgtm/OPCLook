//! Native DA cache/device proof through the real Gateway and SDK fixture.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::{Gateway, ShutdownOutcome, finish_native_sdk};
use opclook_contract::model::{
    BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec,
    NativeWriteStatus, Origin, Owner, Protocol, RequestId, SessionId, Value, WriteDisposition,
    WriteRequest,
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
fn drain(gateway: &Gateway) {
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply: {event:?}"
        );
    }
}
fn read(g: &Gateway, id: u64, key: ItemKey, fresh: bool) -> i64 {
    let kind = if fresh {
        CommandKind::ReadFresh { keys: vec![key] }
    } else {
        CommandKind::ReadCached {
            keys: vec![key],
            max_age_ms: 1000,
        }
    };
    send(g, id, kind);
    let EventKind::Values { values } = reply(g, id).kind else {
        panic!("expected value")
    };
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].0, key);
    assert!(values[0].1.reason.is_none());
    assert_eq!(values[0].1.quality.code, 0xc0);
    let Value::I64(value) = &values[0].1.value else {
        panic!("expected I4")
    };
    *value
}
#[test]
fn sdk_cache_and_device_reads_and_post_write_readback() {
    let mut gateway = Gateway::new();
    let value = ItemKey::FIRST;
    let count = ItemKey::new(2).unwrap();
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
                    key: value,
                    item_id: "Fixture.DeviceValue".into(),
                },
                ItemSpec {
                    key: count,
                    item_id: "Fixture.DeviceReadCount".into(),
                },
            ],
        },
    );
    assert!(
        matches!(reply(&gateway, 2).kind, EventKind::AddItems { results } if results.len() == 2 && results.iter().all(|(_, result)| result.is_ok()))
    );
    let initial = read(&gateway, 3, count, false);
    assert_eq!(read(&gateway, 4, value, false), 42);
    assert_eq!(
        read(&gateway, 5, count, false),
        initial,
        "cache read must not call device"
    );
    assert_eq!(read(&gateway, 6, value, true), 42);
    assert_eq!(
        read(&gateway, 7, count, false),
        initial + 1,
        "fresh read reaches SDK device callback"
    );
    send(
        &gateway,
        8,
        CommandKind::Write {
            request: WriteRequest {
                key: value,
                value: Value::I64(77),
                type_hint: None,
            },
        },
    );
    assert!(
        matches!(reply(&gateway, 8).kind, EventKind::Wrote { key, result: Ok(result) } if key == value && result.disposition == WriteDisposition::Accepted && result.raw == Some(NativeWriteStatus::Da(0)))
    );
    assert_eq!(read(&gateway, 9, value, true), 77);
    assert_eq!(
        read(&gateway, 10, count, false),
        initial + 2,
        "readback reaches device exactly once"
    );
    let missing = ItemKey::new(3).unwrap();
    send(
        &gateway,
        11,
        CommandKind::ReadFresh {
            keys: vec![missing],
        },
    );
    assert!(
        matches!(reply(&gateway, 11).kind, EventKind::Values { values } if values.len() == 1 && values[0].0 == missing && values[0].1.reason == Some(BackendMsg::SignalNotSubscribed))
    );
    drain(&gateway);
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
