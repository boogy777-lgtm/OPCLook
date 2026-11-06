//! Real SDK subscription callback held across Disconnect and same-ID reconnect.
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
    watch: Option<(SessionId, RequestId, ItemKey)>,
}
impl Inbox {
    fn new() -> Self {
        Self {
            queued: VecDeque::new(),
            watch: None,
        }
    }
    fn next(&mut self, g: &Gateway, deadline: Instant) -> Event {
        loop {
            assert!(Instant::now() < deadline, "SDK event deadline");
            if let Some(event) = self.queued.pop_front().or_else(|| g.try_recv()) {
                assert_no_callback_loss(&event);
                if let Some((session, epoch, key)) = self.watch
                    && event.owner == Owner::Session(session)
                    && event.origin == Origin::Session(epoch)
                    && let EventKind::Values { values } = &event.kind
                {
                    assert!(
                        !values
                            .iter()
                            .any(|(item, value)| *item == key && value.value == Value::I64(88)),
                        "stale callback in new epoch: {values:?}"
                    );
                }
                return event;
            }
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
            // The old Disconnect may complete before the new Connect or Add.
            assert!(
                !matches!(event.origin, Origin::Reply(other) if other != RequestId::new(7).unwrap()),
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
                if event.origin != Origin::Session(epoch) {
                    continue;
                }
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
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Some(event) = self.queued.pop_front().or_else(|| g.try_recv()) {
            assert!(Instant::now() < deadline, "SDK event drain deadline");
            assert_no_callback_loss(&event);
            assert!(
                !matches!(event.origin, Origin::Reply(_)),
                "unexpected reply: {event:?}"
            );
            if event.owner == Owner::Session(session) {
                if let EventKind::Values { values } = event.kind {
                    if event.origin != Origin::Session(epoch) {
                        continue;
                    }
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
fn sdk_old_callback_is_not_attributed_to_reconnected_epoch() {
    let mut gateway = Gateway::new();
    let mut inbox = Inbox::new();
    let _release = ReleaseOnDrop;
    let a = SessionId::FIRST;
    let b = SessionId::new(2).unwrap();
    let key = ItemKey::FIRST;
    // A permanently nonempty inbox must not bypass an expired event deadline.
    let mut expired = Inbox::new();
    expired.queued.push_back(Event {
        owner: Owner::Session(a),
        origin: Origin::Session(RequestId::new(1).unwrap()),
        kind: EventKind::SubscriptionState {
            state: SubscriptionState::Active,
        },
    });
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            expired.next(&gateway, Instant::now())
        }))
        .is_err(),
        "queued events must not bypass the deadline"
    );
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
    // Check new-epoch values even while waiting for unrelated correlated replies.
    inbox.watch = Some((a, RequestId::new(8).unwrap(), key));
    // Dispatch in order without waiting for old teardown. The new connection and
    // registration must finish while the SDK callback is still in its barrier.
    send(&gateway, a, 7, CommandKind::Disconnect);
    let mut endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    endpoint.acquisition = Some(AcquisitionConfig {
        update_mode: UpdateMode::Subscription,
        interval_ms: 100,
        read_after_write: false,
        poll_freshness: PollFreshness::CacheAllowed,
        da_read_method: opclook_contract::model::DaReadMethod::Sync,
    });
    send(&gateway, a, 8, CommandKind::Connect { endpoint });
    assert!(matches!(
        inbox.reply(&gateway, a, 8).kind,
        EventKind::Connected { .. }
    ));
    test_late_held().expect("old callback held through new connection");
    send(
        &gateway,
        a,
        9,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key,
                item_id: "Fixture.Value".into(),
            }],
        },
    );
    assert!(
        matches!(inbox.reply(&gateway,a,9).kind,EventKind::AddItems{results} if results.len()==1 && results[0].1.is_ok())
    );
    test_late_held().expect("old callback held through new registration");
    test_late_release().expect("release old callback");
    test_late_processed(10000).expect("old callback processed");
    assert!(matches!(inbox.reply(&gateway, a, 7).kind,
        EventKind::Disconnected { reason: BackendMsg::Raw(reason) } if reason == "DA disconnected"));
    // Advance through 77 after release, then require a subsequent 99 push so
    // the new epoch cannot pass solely on its initial registration snapshot.
    write(&gateway, &mut inbox, b, 13, key, 77);
    inbox.expect_value(&gateway, a, RequestId::new(8).unwrap(), key, 77, true);
    write(&gateway, &mut inbox, b, 14, key, 99);
    inbox.expect_value(&gateway, a, RequestId::new(8).unwrap(), key, 99, true);
    // Session worker drains callback facts before processing this correlated read.
    send(&gateway, a, 10, CommandKind::ReadFresh { keys: vec![key] });
    let read = inbox.reply(&gateway, a, 10);
    assert!(
        matches!(read.kind,EventKind::Values {values} if values.len()==1 && values[0].0==key && values[0].1.value==Value::I64(99))
    );
    inbox.no_stale(&gateway, a, RequestId::new(8).unwrap(), key);
    test_late_close().expect("close barrier");
    for (session, id) in [(a, 11), (b, 12)] {
        send(&gateway, session, id, CommandKind::Disconnect);
        assert!(matches!(
            inbox.reply(&gateway, session, id).kind,
            EventKind::Disconnected { reason: BackendMsg::Raw(reason) } if reason == "DA disconnected"
        ));
    }
    let outcome = gateway.shutdown();
    assert_eq!(outcome.da, ShutdownOutcome::Joined);
    assert_eq!(outcome.ua, ShutdownOutcome::Joined);
    finish_native_sdk(Duration::from_secs(10)).expect("finish SDK host");
    inbox.no_stale(&gateway, a, RequestId::new(8).unwrap(), key);
}
