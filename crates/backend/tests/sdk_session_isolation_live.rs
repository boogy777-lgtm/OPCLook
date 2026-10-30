//! Strict local SDK server/Gateway isolation test; requires --isolation fixture mode.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::{Gateway, ShutdownOutcome};
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, Origin, Owner,
    Protocol, RequestId, SessionId, Value, WriteDisposition, WriteRequest,
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
        // Always unblock SDK request before Gateway is dropped, including assertion failures.
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
fn send(gateway: &Gateway, session: SessionId, id: u64, kind: CommandKind) {
    eprintln!("isolation: dispatch {session} {id}: {kind:?}");
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Session(session),
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
                eprintln!("isolation: reply {id}: {:?}", event.kind);
                return event;
            }
            // A reply before release is a failure, not a discarded unrelated event.
            assert_ne!(
                event.origin,
                Origin::Reply(RequestId::new(5).unwrap()),
                "A completed before release"
            );
        }
        assert!(Instant::now() < deadline, "reply {id} timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn setup(gateway: &Gateway, session: SessionId, base: u64, item: &str) {
    send(
        gateway,
        session,
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
        session,
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
#[test]
fn sdk_session_isolation_live() {
    let mut gateway = Gateway::new();
    let _release = ReleaseOnDrop;
    let a = SessionId::FIRST;
    let b = SessionId::new(2).unwrap();
    setup(&gateway, a, 1, "Fixture.Delayed");
    setup(&gateway, b, 3, "Fixture.Value");
    event("release", "assert-unset");
    send(
        &gateway,
        a,
        5,
        CommandKind::Write {
            request: WriteRequest {
                key: ItemKey::FIRST,
                value: Value::I64(99),
                type_hint: None,
            },
        },
    );
    event("entered", "wait");
    event("release", "assert-unset");
    send(
        &gateway,
        b,
        6,
        CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        },
    );
    assert!(
        matches!(reply(&gateway, 6).kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(42) && values[0].1.reason.is_none())
    );
    send(
        &gateway,
        b,
        7,
        CommandKind::Write {
            request: WriteRequest {
                key: ItemKey::FIRST,
                value: Value::I64(77),
                type_hint: None,
            },
        },
    );
    assert!(
        matches!(reply(&gateway, 7).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    send(
        &gateway,
        b,
        8,
        CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        },
    );
    assert!(
        matches!(reply(&gateway, 8).kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(77) && values[0].1.reason.is_none())
    );
    event("release", "assert-unset");
    event("release", "set");
    assert!(
        matches!(reply(&gateway, 5).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    for (session, id) in [(a, 9), (b, 10)] {
        send(&gateway, session, id, CommandKind::Disconnect);
        assert!(matches!(
            reply(&gateway, id).kind,
            EventKind::Disconnected { .. }
        ));
    }
    assert_eq!(gateway.shutdown().da, ShutdownOutcome::Joined);
    opclook_backend::bff::finish_native_sdk(Duration::from_secs(10)).unwrap();
}
