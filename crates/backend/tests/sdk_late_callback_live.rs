//! Real SDK callback held across removal and re-registration in one session.
#![cfg(all(windows, feature = "native-da-test-hooks"))]
use opclook_backend::bff::{
    Gateway, ShutdownOutcome,
    da::{
        test_late_arm, test_late_close, test_late_entered, test_late_held, test_late_processed,
        test_late_release,
    },
    finish_native_sdk,
};
use opclook_contract::model::{
    AcquisitionConfig, BackendMsg, Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey,
    ItemSpec, Origin, Owner, PollFreshness, Protocol, RequestId, SessionId, SubscriptionState,
    UpdateMode, Value, WriteDisposition, WriteRequest,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

struct ReleaseOnDrop;
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = test_late_release();
    }
}
fn send(g: &Gateway, session: SessionId, id: u64, kind: CommandKind) {
    g.send(Command {
        protocol: Protocol::OpcDa,
        owner: Owner::Session(session),
        request_id: Some(RequestId::new(id).unwrap()),
        kind,
    })
    .unwrap();
}
fn assert_no_callback_loss(event: &Event) {
    assert!(
        !matches!(
            &event.kind,
            EventKind::SubscriptionState {
                state: SubscriptionState::Failed
            }
        ),
        "subscription failed: {event:?}"
    );
    assert!(
        !matches!(&event.kind, EventKind::Log { message: BackendMsg::Raw(text) } if text.contains("DA subscription queue overflow") || text.contains("DA callback queue:")),
        "callback queue failed: {event:?}"
    );
}
struct Inbox {
    queued: VecDeque<Event>,
}
impl Inbox {
    fn new() -> Self {
        Self {
            queued: VecDeque::new(),
        }
    }
    fn next(&mut self, g: &Gateway, deadline: Instant) -> Event {
        loop {
            if let Some(event) = self.queued.pop_front().or_else(|| g.try_recv()) {
                assert_no_callback_loss(&event);
                return event;
            }
            assert!(Instant::now() < deadline, "SDK event deadline");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn reply(&mut self, g: &Gateway, session: SessionId, id: u64) -> Event {
        let until = Instant::now() + Duration::from_secs(10);
        let mut deferred = VecDeque::new();
        loop {
            let event = self.next(g, until);
            if event.origin == Origin::Reply(RequestId::new(id).unwrap()) {
                assert_eq!(event.owner, Owner::Session(session));
                self.queued.extend(deferred);
                return event;
            }
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
            assert!(deferred.len() < 512, "too many deferred events");
            deferred.push_back(event);
        }
    }
    fn expect_value(
        &mut self,
        g: &Gateway,
        session: SessionId,
        epoch: RequestId,
        key: ItemKey,
        expected: i64,
        reject_old: bool,
    ) {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let event = self.next(g, until);
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
            if event.owner != Owner::Session(session) {
                continue;
            }
            if let EventKind::Values { values } = event.kind {
                assert_eq!(event.origin, Origin::Session(epoch));
                for (item, value) in values {
                    if item != key {
                        continue;
                    }
                    if reject_old {
                        assert_ne!(
                            value.value,
                            Value::I64(88),
                            "stale callback reached new registration"
                        );
                    }
                    if value.value == Value::I64(expected) {
                        assert_eq!(value.quality.code, 0xc0);
                        return;
                    }
                }
            }
        }
    }
    fn no_stale(&mut self, g: &Gateway, session: SessionId, epoch: RequestId, key: ItemKey) {
        while let Some(event) = self.queued.pop_front().or_else(|| g.try_recv()) {
            assert_no_callback_loss(&event);
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
            if event.owner == Owner::Session(session) {
                if let EventKind::Values { values } = event.kind {
                    assert_eq!(event.origin, Origin::Session(epoch));
                    assert!(
                        !values
                            .iter()
                            .any(|(k, v)| *k == key && v.value == Value::I64(88)),
                        "stale callback: {values:?}"
                    );
                }
            }
        }
    }
}
fn write(g: &Gateway, inbox: &mut Inbox, session: SessionId, id: u64, key: ItemKey, value: i64) {
    send(
        g,
        session,
        id,
        CommandKind::Write {
            request: WriteRequest {
                key,
                value: Value::I64(value),
                type_hint: None,
            },
        },
    );
    assert!(
        matches!(inbox.reply(g,session,id).kind,EventKind::Wrote {key:k,result:Ok(outcome)} if k==key && outcome.disposition==WriteDisposition::Accepted)
    );
}
#[test]
fn sdk_old_callback_is_rejected_after_remove_and_readd() {
    let mut gateway = Gateway::new();
    let mut inbox = Inbox::new();
    let _release = ReleaseOnDrop;
    let a = SessionId::FIRST;
    let b = SessionId::new(2).unwrap();
    let key = ItemKey::FIRST;
    let mut endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    endpoint.acquisition = Some(AcquisitionConfig {
        update_mode: UpdateMode::Subscription,
        interval_ms: 100,
        read_after_write: false,
        poll_freshness: PollFreshness::CacheAllowed,
        da_read_method: opclook_contract::model::DaReadMethod::Sync,
    });
    send(&gateway, a, 1, CommandKind::Connect { endpoint });
    assert!(matches!(
        inbox.reply(&gateway, a, 1).kind,
        EventKind::Connected { .. }
    ));
    send(
        &gateway,
        a,
        2,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key,
                item_id: "Fixture.Value".into(),
            }],
        },
    );
    assert!(
        matches!(inbox.reply(&gateway,a,2).kind,EventKind::AddItems{results} if results.len()==1 && results[0].1.is_ok())
    );
    inbox.expect_value(&gateway, a, RequestId::new(1).unwrap(), key, 42, false);
    send(
        &gateway,
        b,
        3,
        CommandKind::Connect {
            endpoint: EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1"),
        },
    );
    assert!(matches!(
        inbox.reply(&gateway, b, 3).kind,
        EventKind::Connected { .. }
    ));
    send(
        &gateway,
        b,
        4,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key,
                item_id: "Fixture.Value".into(),
            }],
        },
    );
    assert!(
        matches!(inbox.reply(&gateway,b,4).kind,EventKind::AddItems{results} if results.len()==1 && results[0].1.is_ok())
    );
    test_late_arm(88).expect("arm callback barrier");
    write(&gateway, &mut inbox, b, 5, key, 88);
    test_late_entered(10000).expect("real SDK callback entered");
    test_late_held().expect("callback held");
    write(&gateway, &mut inbox, b, 6, key, 99);
    send(&gateway, a, 7, CommandKind::RemoveItems { keys: vec![key] });
    assert!(matches!(
        inbox.reply(&gateway, a, 7).kind,
        EventKind::Removed { .. }
    ));
    send(
        &gateway,
        a,
        8,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key,
                item_id: "Fixture.Value".into(),
            }],
        },
    );
    assert!(
        matches!(inbox.reply(&gateway,a,8).kind,EventKind::AddItems{results} if results.len()==1 && results[0].1.is_ok())
    );
    test_late_held().expect("old callback held through re-registration");
    test_late_release().expect("release old callback");
    test_late_processed(10000).expect("old callback processed");
    inbox.expect_value(&gateway, a, RequestId::new(1).unwrap(), key, 99, true);
    // Session worker drains callback facts before processing this correlated read.
    send(&gateway, a, 9, CommandKind::ReadFresh { keys: vec![key] });
    let read = inbox.reply(&gateway, a, 9);
    assert!(
        matches!(read.kind,EventKind::Values {values} if values.len()==1 && values[0].0==key && values[0].1.value==Value::I64(99))
    );
    inbox.no_stale(&gateway, a, RequestId::new(1).unwrap(), key);
    test_late_close().expect("close barrier");
    for (session, id) in [(a, 10), (b, 11)] {
        send(&gateway, session, id, CommandKind::Disconnect);
        assert!(matches!(
            inbox.reply(&gateway, session, id).kind,
            EventKind::Disconnected { .. }
        ));
    }
    let outcome = gateway.shutdown();
    assert_eq!(outcome.da, ShutdownOutcome::Joined);
    assert_eq!(outcome.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).expect("finish SDK host");
    inbox.no_stale(&gateway, a, RequestId::new(1).unwrap(), key);
}
