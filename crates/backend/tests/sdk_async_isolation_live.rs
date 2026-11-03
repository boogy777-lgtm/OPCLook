//! A real manager-owned async session must not block an independent Gateway.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    Gateway, ShutdownOutcome,
    da::{
        TestManagerAsyncLease, test_async_gate_arm, test_async_gate_close, test_async_gate_entered,
        test_async_gate_held, test_async_gate_release, test_host_acquisitions,
    },
    finish_native_sdk,
};
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, Origin, Owner,
    Protocol, RequestId, SessionId, Value,
};
use std::time::{Duration, Instant};

struct ReleaseOnDrop;
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = test_async_gate_release();
    }
}
fn send(gateway: &Gateway, id: u64, kind: CommandKind) {
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
#[test]
fn sdk_async_pending_session_does_not_block_other_gateway() {
    assert_eq!(
        test_host_acquisitions(),
        (0, true),
        "fresh process required"
    );
    let mut blocked = TestManagerAsyncLease::open_fixture().expect("session A");
    assert_eq!(test_host_acquisitions(), (1, true));
    let mut gateway = Gateway::new();
    test_async_gate_arm().expect("arm");
    let _release = ReleaseOnDrop;
    let started = Instant::now();
    assert_eq!(blocked.read(100).expect("A read"), 0x80070102_u32 as i32);
    assert!(started.elapsed() < Duration::from_secs(10));
    test_async_gate_entered(10000).expect("SDK worker A entered barrier");
    test_async_gate_held().expect("A release unset");
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
    assert_eq!(
        test_host_acquisitions(),
        (2, true),
        "both use the same manager host"
    );
    let key = ItemKey::FIRST;
    send(
        &gateway,
        2,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key,
                item_id: "Fixture.DeviceValue".into(),
            }],
        },
    );
    assert!(
        matches!(reply(&gateway, 2).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].0 == key && results[0].1.is_ok())
    );
    test_async_gate_held().expect("A still blocked before B read");
    send(&gateway, 3, CommandKind::ReadFresh { keys: vec![key] });
    assert!(
        matches!(reply(&gateway, 3).kind, EventKind::Values { values } if values.len() == 1 && values[0].0 == key && values[0].1.value == Value::I64(42) && values[0].1.quality.code == 0xc0 && values[0].1.timestamp.is_some() && values[0].1.reason.is_none())
    );
    test_async_gate_held().expect("B completed while A remained blocked");
    send(&gateway, 4, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, 4).kind,
        EventKind::Disconnected { .. }
    ));
    let outcome = gateway.shutdown();
    assert_eq!(outcome.da, ShutdownOutcome::Joined);
    assert_eq!(outcome.ua, ShutdownOutcome::Joined);
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply: {event:?}"
        );
    }
    test_async_gate_held().expect("A still blocked after B shutdown");
    assert_eq!(
        finish_native_sdk(Duration::from_secs(3)).unwrap_err(),
        "SDK sessions still running"
    );
    test_async_gate_held().expect("A still blocked after manager Busy");
    test_async_gate_release().expect("release A");
    assert_eq!(blocked.retire(30000).expect("retire A"), (0, 0));
    test_async_gate_close().expect("close barrier");
    blocked.close().expect("close A");
    finish_native_sdk(Duration::from_secs(10)).expect("finish manager and clean SDK host");
}
