//! N2L09: strict two-Gateway test, using the SDK fixture's device-write barrier.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    Gateway, ShutdownOutcome, da::test_host_acquisitions, finish_native_sdk,
};
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NativeWriteStatus,
    Origin, Owner, Protocol, RequestId, SessionId, Value, WriteDisposition, WriteRequest,
};
use std::time::{Duration, Instant};

fn event(name: &str, action: &str) {
    let status = std::process::Command::new(std::env::var("OPCLOOK_ISOLATION_PYTHON").unwrap())
        .arg(std::env::var("OPCLOOK_ISOLATION_HELPER").unwrap())
        .args([name, action])
        .status()
        .unwrap();
    assert!(status.success(), "fixture event {name} {action}: {status}");
}
struct ReleaseOnDrop;
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        if let (Ok(python), Ok(helper)) = (
            std::env::var("OPCLOOK_ISOLATION_PYTHON"),
            std::env::var("OPCLOOK_ISOLATION_HELPER"),
        ) {
            let _ = std::process::Command::new(python)
                .arg(helper)
                .args(["release", "set"])
                .status();
        }
    }
}
fn send(gateway: &Gateway, id: u64, kind: CommandKind) {
    eprintln!("host reuse: dispatch {id}: {kind:?}");
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
            eprintln!("host reuse: event {:?} {:?}", event.origin, event.kind);
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
fn assert_no_replies(gateway: &Gateway) {
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply: {event:?}"
        );
    }
}
fn setup(gateway: &Gateway, base: u64, item: &str) {
    send(
        gateway,
        base,
        CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        },
    );
    assert!(matches!(
        reply(gateway, base).kind,
        EventKind::Connected { .. }
    ));
    send(
        gateway,
        base + 1,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key: ItemKey::FIRST,
                item_id: item.into(),
            }],
        },
    );
    assert!(
        matches!(reply(gateway, base + 1).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
}
fn write(value: i64) -> CommandKind {
    CommandKind::Write {
        request: WriteRequest {
            key: ItemKey::FIRST,
            value: Value::I64(value),
            type_hint: None,
        },
    }
}
#[test]
fn sdk_host_reused_while_old_gateway_write_is_blocked() {
    assert!(
        test_host_acquisitions().0 == 0,
        "test must run in a fresh process"
    );
    let mut old = Gateway::new();
    // Both Gateway owners precede the release guard, including on assertion failure.
    let mut healthy = None;
    let _release = ReleaseOnDrop;
    setup(&old, 1, "Fixture.Delayed");
    let first = test_host_acquisitions();
    assert_eq!(first, (1, true), "old Gateway acquired one host");
    event("release", "assert-unset");
    send(&old, 3, write(99));
    event("entered", "wait");
    event("release", "assert-unset");
    let start = Instant::now();
    let shutdown = old.shutdown();
    assert_eq!(shutdown.da, ShutdownOutcome::TimedOut);
    assert_eq!(shutdown.ua, ShutdownOutcome::Joined);
    assert!(
        start.elapsed() < Duration::from_secs(12),
        "old Gateway shutdown was not bounded"
    );
    assert_no_replies(&old);

    assert!(healthy.is_none());
    healthy = Some(Gateway::new());
    let new_gateway = healthy.as_mut().unwrap();
    setup(new_gateway, 10, "Fixture.Value");
    let acquisitions = test_host_acquisitions();
    assert_eq!(
        acquisitions,
        (2, true),
        "both Gateways must receive the same live SharedCore"
    );
    send(
        new_gateway,
        12,
        CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        },
    );
    assert!(
        matches!(reply(new_gateway, 12).kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(42) && values[0].1.reason.is_none())
    );
    send(new_gateway, 13, write(77));
    assert!(
        matches!(reply(new_gateway, 13).kind, EventKind::Wrote { key, result: Ok(outcome) } if key == ItemKey::FIRST && outcome.disposition == WriteDisposition::Accepted && outcome.raw == Some(NativeWriteStatus::Da(0)))
    );
    send(
        new_gateway,
        14,
        CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        },
    );
    assert!(
        matches!(reply(new_gateway, 14).kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(77) && values[0].1.reason.is_none())
    );
    send(new_gateway, 15, CommandKind::Disconnect);
    assert!(matches!(
        reply(new_gateway, 15).kind,
        EventKind::Disconnected { .. }
    ));
    let healthy_shutdown = new_gateway.shutdown();
    assert_eq!(healthy_shutdown.da, ShutdownOutcome::Joined);
    assert_eq!(healthy_shutdown.ua, ShutdownOutcome::Joined);
    assert_no_replies(new_gateway);
    event("release", "assert-unset");
    let busy = finish_native_sdk(Duration::from_secs(2)).unwrap_err();
    assert_eq!(busy, "SDK sessions still running");
    assert_no_replies(&old);
    event("release", "set");
    let write_reply = reply(&old, 3);
    assert!(
        matches!(write_reply.kind, EventKind::Wrote { key, result: Ok(outcome) } if key == ItemKey::FIRST && outcome.disposition == WriteDisposition::Accepted && outcome.raw == Some(NativeWriteStatus::Da(0)))
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match finish_native_sdk(Duration::from_secs(2)) {
            Ok(()) => break,
            Err(e) if e == "SDK sessions still running" && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("SDK final cleanup failed: {e}"),
        }
    }
    assert_no_replies(&old);
}
