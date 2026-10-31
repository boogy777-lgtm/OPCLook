//! R2 G/D matrix: actual Engine state transitions, mocked Gateway boundary.
use super::*;
use opclook_contract::model::{AcquisitionConfig, BackendMsg, ItemInfo, ItemSpec};

fn setup(now: Instant) -> (Engine<()>, SessionId, ItemKey) {
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let session = SessionId::FIRST;
    let key = ItemKey::FIRST;
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = opclook_contract::model::UpdateMode::Poll;
    profile.interval_ms = 1000;
    endpoint.acquisition = Some(profile);
    let connect = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Connect {
                endpoint: endpoint.clone(),
            },
            (),
        )
        .unwrap();
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(connect),
                kind: EventKind::Connected {
                    endpoint,
                    info: BackendMsg::Raw(String::new()),
                },
            },
            now,
        )
        .unwrap();
    let add = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::AddItems {
                items: vec![ItemSpec {
                    key,
                    item_id: "ns=2;s=tag".into(),
                }],
            },
            (),
        )
        .unwrap();
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(add),
                kind: EventKind::AddItems {
                    results: vec![(
                        key,
                        Ok(ItemInfo {
                            data_type: None,
                            type_id: None,
                            writable: true,
                        }),
                    )],
                },
            },
            now,
        )
        .unwrap();
    (engine, session, key)
}

#[test]
fn disconnect_pending_does_not_schedule_a_read_without_a_current_epoch() {
    use opclook_contract::model::{ReadCancelReason, ReadNeed, ReadTerminalStatus};
    use std::sync::{Arc, Mutex};
    let now = Instant::now();
    let (mut engine, session, key) = setup(now);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = observed.clone();
    engine.set_read_terminal_sink(Some(Box::new(move |_, _, terminal| {
        sink.lock().unwrap().push(terminal.clone())
    })));
    engine.request_refresh(session, key).unwrap();
    engine
        .send(session, Protocol::OpcUa, CommandKind::Disconnect, ())
        .unwrap();
    assert_eq!(engine.connection_id(session), None);
    let permit_before = engine.next_permit;
    let due = engine.due(now + Duration::from_secs(10), Duration::from_secs(1), None);
    assert!(
        !due.iter().any(|event| matches!(event, Due::Read { .. })),
        "Read scheduled during pending Disconnect"
    );
    assert_eq!(engine.next_permit, permit_before);
    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert!(observed[0].needs.contains(&ReadNeed::Initial));
    assert!(observed[0].needs.contains(&ReadNeed::ExplicitRefresh));
    assert!(matches!(
        observed[0].status,
        ReadTerminalStatus::Cancelled(ReadCancelReason::Disconnected)
    ));
}
