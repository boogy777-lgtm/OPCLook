//! Strict native SDK group S_FALSE and independent Gateway key outcomes.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    Gateway, ShutdownOutcome, da::test_sdk_mixed_read_fixture, finish_native_sdk,
};
use opclook_contract::model::{
    BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, Origin,
    Owner, Protocol, QualityKind, RequestId, SessionId, Value, ValueOrigin,
};
use std::time::{Duration, Instant};
fn send(gateway: &Gateway, id: u64, kind: CommandKind) {
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Session(SessionId::FIRST),
            request_id: RequestId::new(id),
            kind,
        })
        .unwrap();
}
fn reply(gateway: &Gateway, id: u64) -> Event {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(event) = gateway.try_recv() {
            if event.origin == Origin::Reply(RequestId::new(id).unwrap()) {
                assert_eq!(event.owner, Owner::Session(SessionId::FIRST));
                return event;
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected correlated reply: {event:?}"
            );
        }
        assert!(Instant::now() < deadline, "reply {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn sdk_mixed_group_sfalse_and_gateway_per_item_facts() {
    // This is one real SDK group syncRead(2), distinct from Gateway's per-key calls.
    let mixed = test_sdk_mixed_read_fixture().expect("actual SDK mixed group read");
    assert_eq!(mixed[0].service, 1);
    assert_eq!(mixed[1].service, 1); // S_FALSE
    assert_eq!(mixed[0].available, 7);
    assert_eq!(mixed[0].item, 0);
    assert_eq!(mixed[0].vartype, 3);
    assert_eq!(mixed[0].bits_low, 42);
    assert_eq!(mixed[0].quality, 0xc0);
    assert_ne!(mixed[0].filetime, 0);
    assert_eq!(mixed[1].available, 3); // service + item only; no sample facts
    assert_eq!(mixed[1].item, 0x80070005_u32 as i32); // E_ACCESSDENIED
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
        "Fixture.Value",
        "Fixture.ReadDenied",
        "Fixture.ReadUncertain",
        "Fixture.ReadBad",
    ];
    let items = cases
        .iter()
        .enumerate()
        .map(|(i, id)| ItemSpec {
            key: ItemKey::new((i + 1) as u32).unwrap(),
            item_id: (*id).into(),
        })
        .collect();
    send(&gateway, 2, CommandKind::AddItems { items });
    assert!(
        matches!(reply(&gateway,2).kind,EventKind::AddItems{results} if results.len()==4 && results.iter().all(|(_,r)|r.is_ok()))
    );
    let keys: Vec<_> = (1..=4).map(|n| ItemKey::new(n).unwrap()).collect();
    send(&gateway, 3, CommandKind::ReadFresh { keys: keys.clone() });
    let EventKind::Values { values } = reply(&gateway, 3).kind else {
        panic!("expected Values")
    };
    assert_eq!(values.len(), 4);
    for (index, (key, value)) in values.iter().enumerate() {
        assert_eq!(*key, keys[index]);
        assert_eq!(value.times.origin, ValueOrigin::Read);
        assert!(value.times.received_at.is_some());
        match index {
            0 => {
                assert_eq!(value.value, Value::I64(42));
                assert_eq!(value.quality.kind, QualityKind::Good);
                assert_eq!(value.quality.code, 0xc0);
                assert!(value.timestamp.is_some());
                assert_eq!(value.times.da_timestamp, value.timestamp);
                assert!(value.reason.is_none());
            }
            1 => {
                assert_eq!(value.value, Value::Empty);
                assert_eq!(value.quality.kind, QualityKind::Bad);
                assert!(value.timestamp.is_none());
                assert!(value.times.da_timestamp.is_none());
                assert!(
                    matches!(&value.reason,Some(BackendMsg::DaReadFailure {adapter:Some(0),service:Some(1),item:Some(code),quality:None,filetime:None,vartype:None,cause:None}) if *code==0x80070005_u32 as i32)
                );
            }
            2 => {
                assert_eq!(value.value, Value::I64(51));
                assert_eq!(value.quality.kind, QualityKind::Uncertain);
                assert_eq!(value.quality.code, 0x40);
                assert!(value.timestamp.is_some());
                assert_eq!(value.times.da_timestamp, value.timestamp);
                assert!(value.reason.is_none());
            }
            3 => {
                assert_eq!(value.value, Value::I64(52));
                assert_eq!(value.quality.kind, QualityKind::Bad);
                assert_eq!(value.quality.code, 0);
                assert!(value.timestamp.is_some());
                assert_eq!(value.times.da_timestamp, value.timestamp);
                assert!(value.reason.is_none());
            }
            _ => panic!("unexpected item"),
        }
    }
    send(&gateway, 4, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, 4).kind,
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
