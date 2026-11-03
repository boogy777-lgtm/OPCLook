//! Strict SDK/Gateway shutdown test with a real blocked fixture device write.
//! Requires the local SDK server launched with `run-interop.py --isolation`.
#![cfg(all(windows, feature = "native-da-preview"))]

use opclook_backend::bff::{Gateway, ShutdownOutcome, finish_native_sdk};
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
            // Unblock the fixture even if an assertion or SDK call fails.
            let _ = std::process::Command::new(python)
                .arg(helper)
                .args(["release", "set"])
                .status();
        }
    }
}

fn send(gateway: &Gateway, id: u64, kind: CommandKind) {
    eprintln!("pending shutdown: dispatch {id}: {kind:?}");
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
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(event) = gateway.try_recv() {
            if event.origin == Origin::Reply(RequestId::new(id).unwrap()) {
                assert_eq!(event.owner, Owner::Session(SessionId::FIRST));
                eprintln!("pending shutdown: reply {id}: {:?}", event.kind);
                return event;
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply while waiting for {id}: {event:?}"
            );
        }
        assert!(Instant::now() < deadline, "reply {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn assert_no_reply(gateway: &Gateway, phase: &str) {
    while let Some(event) = gateway.try_recv() {
        assert!(
            !matches!(event.origin, Origin::Reply(_)),
            "unexpected reply {phase}: {event:?}"
        );
    }
}

#[test]
fn sdk_pending_write_shutdown_is_bounded_and_host_closes_after_release() {
    let mut gateway = Gateway::new();
    let _release = ReleaseOnDrop;
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
                item_id: "Fixture.Delayed".into(),
            }],
        },
    );
    assert!(matches!(reply(&gateway, 2).kind,
        EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok()));

    event("release", "assert-unset");
    send(
        &gateway,
        3,
        CommandKind::Write {
            request: WriteRequest {
                key: ItemKey::FIRST,
                value: Value::I64(99),
                type_hint: None,
            },
        },
    );
    // The server signals only once the SDK Write reaches its device handler.
    event("entered", "wait");
    event("release", "assert-unset");
    assert_no_reply(&gateway, "before release");

    let started = Instant::now();
    let outcome = gateway.shutdown();
    let elapsed = started.elapsed();
    eprintln!("pending shutdown: {outcome:?}, elapsed={elapsed:?}");
    assert_eq!(outcome.da, ShutdownOutcome::TimedOut);
    assert_eq!(outcome.ua, ShutdownOutcome::Joined);
    // The router has a five-second join budget; Gateway has a ten-second
    // budget. Allow scheduling margin while enforcing a bounded caller.
    assert!(
        elapsed >= Duration::from_secs(4),
        "returned too early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "shutdown exceeded budget: {elapsed:?}"
    );
    event("release", "assert-unset");
    assert_no_reply(&gateway, "before release");

    // A timed-out Gateway does not imply that the in-flight write succeeded.
    // Native host destruction must be refused while its SDK call remains live.
    let busy = finish_native_sdk(Duration::from_secs(3)).unwrap_err();
    assert_eq!(busy, "SDK sessions still running");
    event("release", "assert-unset");
    assert_no_reply(&gateway, "before release");

    event("release", "set");
    assert!(matches!(reply(&gateway, 3).kind,
        EventKind::Wrote { result: Ok(outcome), .. }
            if outcome.disposition == WriteDisposition::Accepted &&
               matches!(outcome.raw, Some(NativeWriteStatus::Da(0)))));

    // The terminal Write reply precedes session teardown. Busy is a read-only
    // ownership observation; only the first idle call performs final shutdown.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match finish_native_sdk(Duration::from_secs(3)) {
            Ok(()) => break,
            Err(error) if error == "SDK sessions still running" && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("SDK did not finish after release: {error}"),
        }
    }
    // Successful finish means no native session/worker can enqueue a late
    // duplicate after this drain.
    assert_no_reply(&gateway, "after SDK cleanup");
}
