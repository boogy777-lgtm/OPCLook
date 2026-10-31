//! Real SDK server value exceeding the adapter's bounded array codec.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::{Gateway, ShutdownOutcome, finish_native_sdk};
use opclook_contract::model::{
    BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, Origin,
    Owner, Protocol, QualityKind, RequestId, SessionId, Value,
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
fn sdk_codec_failure_retains_attested_read_facts() {
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
    send(
        &gateway,
        2,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key: ItemKey::FIRST,
                item_id: "Fixture.LargeInts".into(),
            }],
        },
    );
    assert!(
        matches!(reply(&gateway, 2).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    send(
        &gateway,
        3,
        CommandKind::ReadFresh {
            keys: vec![ItemKey::FIRST],
        },
    );
    let EventKind::Values { values } = reply(&gateway, 3).kind else {
        panic!("expected Values")
    };
    assert_eq!(values.len(), 1);
    let (key, value) = &values[0];
    assert_eq!(*key, ItemKey::FIRST);
    assert_eq!(value.value, Value::Empty);
    assert_eq!(value.quality.kind, QualityKind::Bad);
    assert!(value.timestamp.is_some());
    assert_eq!(value.times.da_timestamp, value.timestamp);
    assert!(
        matches!(&value.reason, Some(BackendMsg::DaReadFailure { adapter: Some(-2147024774), service: Some(0), item: Some(0), quality: Some(0xc0), filetime: Some(ticks), vartype: Some(0x2003), .. }) if *ticks != 0)
    );
    send(&gateway, 4, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, 4).kind,
        EventKind::Disconnected { .. }
    ));
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(shutdown.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).unwrap();
}
