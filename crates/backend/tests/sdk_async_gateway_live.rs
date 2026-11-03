//! Test-only SDK Async slot through the real Gateway session and correlation IDs.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    Gateway, ShutdownOutcome,
    da::{
        test_async_gate_arm, test_async_gate_close, test_async_gate_entered, test_async_gate_held,
        test_async_gate_release, test_gateway_async_arm, test_gateway_async_clear,
        test_gateway_async_observation,
    },
    finish_native_sdk,
};
use opclook_contract::model::{
    AcquisitionConfig, BackendMsg, Command, CommandKind, DaReadMethod, EndpointSpec, Event,
    EventKind, ItemKey, ItemSpec, Operation, Origin, Owner, PollFreshness, Protocol, RequestId,
    SessionId, UpdateMode, Value,
};
use std::time::{Duration, Instant};

struct ReleaseOnDrop;
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = test_async_gate_release();
    }
}
fn id(n: u64) -> RequestId {
    RequestId::new(n).unwrap()
}
fn send(gateway: &Gateway, owner: SessionId, request: u64, kind: CommandKind) {
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: owner.into(),
            request_id: Some(id(request)),
            kind,
        })
        .unwrap();
}
fn reply(gateway: &Gateway, owner: SessionId, request: u64, epoch: RequestId) -> Event {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = gateway.try_recv() {
            if let Origin::Reply(actual) = event.origin {
                assert_eq!(
                    actual,
                    id(request),
                    "unexpected, duplicate or stale reply: {event:?}"
                );
                assert_eq!(event.owner, Owner::Session(owner));
                return event;
            }
            if event.owner == Owner::Session(SessionId::FIRST) {
                if let Origin::Session(actual) = event.origin {
                    assert_eq!(actual, epoch, "old epoch event");
                }
            }
        }
        assert!(Instant::now() < end, "reply {request} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn subscription() -> EndpointSpec {
    let mut endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    endpoint.acquisition = Some(AcquisitionConfig {
        update_mode: UpdateMode::Subscription,
        interval_ms: 100,
        read_after_write: false,
        poll_freshness: PollFreshness::CacheAllowed,
        da_read_method: DaReadMethod::Sync,
    });
    endpoint
}
fn add(g: &Gateway, owner: SessionId, request: u64, epoch: RequestId) {
    send(
        g,
        owner,
        request,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key: ItemKey::FIRST,
                item_id: "Fixture.Value".into(),
            }],
        },
    );
    assert!(
        matches!(reply(g,owner,request,epoch).kind,EventKind::AddItems{results} if results.len()==1 && results[0].0==ItemKey::FIRST && results[0].1.is_ok())
    );
}
fn fresh(g: &Gateway, owner: SessionId, request: u64, epoch: RequestId) {
    send(
        g,
        owner,
        request,
        CommandKind::ReadFresh {
            keys: vec![ItemKey::FIRST],
        },
    );
    assert!(
        matches!(reply(g,owner,request,epoch).kind,EventKind::Values{values} if values.len()==1 && values[0].0==ItemKey::FIRST && values[0].1.value==Value::I64(42) && values[0].1.quality.code==0xc0 && values[0].1.timestamp.is_some() && values[0].1.reason.is_none())
    );
}
#[test]
fn gateway_async_timeout_is_correlated_and_late_completion_only_retires() {
    let a = SessionId::FIRST;
    let b = SessionId::new(2).unwrap();
    let epoch = id(10);
    test_gateway_async_arm(a, epoch, id(12), ItemKey::FIRST, 100).unwrap();
    test_async_gate_arm().unwrap();
    let _release = ReleaseOnDrop;
    let mut gateway = Gateway::new();
    send(
        &gateway,
        a,
        10,
        CommandKind::Connect {
            endpoint: subscription(),
        },
    );
    assert!(matches!(
        reply(&gateway, a, 10, epoch).kind,
        EventKind::Connected { .. }
    ));
    add(&gateway, a, 11, epoch);
    // Wrong request ID must remain the normal synchronous route.
    fresh(&gateway, a, 13, epoch);
    assert!(!test_gateway_async_observation().unwrap().dispatched);
    send(
        &gateway,
        a,
        12,
        CommandKind::ReadFresh {
            keys: vec![ItemKey::FIRST],
        },
    );
    assert!(matches!(
        reply(&gateway, a, 12, epoch).kind,
        EventKind::Error {
            operation: Operation::Read,
            message: BackendMsg::TimeoutRead
        }
    ));
    test_async_gate_entered(10_000).unwrap();
    test_async_gate_held().unwrap();
    let first = test_gateway_async_observation().unwrap();
    assert!(first.dispatched && first.token > 0 && first.registration > 0);
    assert_eq!(
        (first.owner, first.epoch, first.request, first.key),
        (a, epoch, id(12), ItemKey::FIRST)
    );
    // A different live Gateway session completes its real SDK read while A remains held.
    send(
        &gateway,
        b,
        40,
        CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        },
    );
    assert!(matches!(
        reply(&gateway, b, 40, epoch).kind,
        EventKind::Connected { .. }
    ));
    add(&gateway, b, 41, epoch);
    fresh(&gateway, b, 42, epoch);
    test_async_gate_held().unwrap();
    test_async_gate_release().unwrap();
    // This request explicitly waits for retirement, not a second result for 12.
    send(&gateway, a, 14, CommandKind::Test);
    assert!(matches!(
        reply(&gateway, a, 14, epoch).kind,
        EventKind::Log {
            message: BackendMsg::DaStateOk
        }
    ));
    test_async_gate_close().unwrap();
    send(&gateway, a, 15, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, a, 15, epoch).kind,
        EventKind::Disconnected { .. }
    ));
    test_gateway_async_clear();
    let next_epoch = id(20);
    test_gateway_async_arm(a, next_epoch, id(22), ItemKey::FIRST, 10_000).unwrap();
    send(
        &gateway,
        a,
        20,
        CommandKind::Connect {
            endpoint: subscription(),
        },
    );
    assert!(matches!(
        reply(&gateway, a, 20, next_epoch).kind,
        EventKind::Connected { .. }
    ));
    add(&gateway, a, 21, next_epoch);
    fresh(&gateway, a, 22, next_epoch);
    let next = test_gateway_async_observation().unwrap();
    assert!(next.dispatched && next.token > 0 && next.registration > 0);
    assert_ne!(first.registration, next.registration);
    assert_eq!(
        (next.owner, next.epoch, next.request),
        (a, next_epoch, id(22))
    );
    send(&gateway, a, 23, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, a, 23, next_epoch).kind,
        EventKind::Disconnected { .. }
    ));
    send(&gateway, b, 43, CommandKind::Disconnect);
    assert!(matches!(
        reply(&gateway, b, 43, next_epoch).kind,
        EventKind::Disconnected { .. }
    ));
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(shutdown.ua, ShutdownOutcome::Joined);
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected duplicate reply: {event:?}"
        );
    }
    test_gateway_async_clear();
    finish_native_sdk(Duration::from_secs(10)).unwrap();
}
