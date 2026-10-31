//! Drop settles logical reads; SDK lifetime stays with Gateway.
use super::*;
use opclook_contract::model::{ReadCancelReason, ReadNeed, ReadTerminalStatus};
use std::sync::{Arc, Mutex};

fn engine_with_sink() -> (
    Engine<()>,
    Arc<Mutex<Vec<opclook_contract::model::ReadTerminal>>>,
) {
    let now = Instant::now();
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let mut view = SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), now);
    view.state = SessionState::Connected;
    view.connection_id = Some(RequestId::new(77).unwrap());
    view.items.insert(ItemKey::FIRST, false);
    view.pending_initial.insert(ItemKey::FIRST, now);
    engine.sessions.insert(SessionId::FIRST, view);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = observed.clone();
    engine.set_read_terminal_sink(Some(Box::new(move |_, _, terminal| {
        sink.lock().unwrap().push(terminal.clone())
    })));
    (engine, observed)
}

#[test]
fn pending_cancel_is_delivered_on_drop_and_not_duplicated_after_shutdown() {
    for explicit in [false, true] {
        let (mut engine, observed) = engine_with_sink();
        engine
            .request_refresh(SessionId::FIRST, ItemKey::FIRST)
            .unwrap();
        if explicit {
            engine.shutdown();
        }
        drop(engine);
        let terminals = observed.lock().unwrap();
        assert_eq!(terminals.len(), 1, "explicit shutdown={explicit}");
        assert!(terminals[0].needs.contains(&ReadNeed::Initial));
        assert!(terminals[0].needs.contains(&ReadNeed::ExplicitRefresh));
        assert!(matches!(
            terminals[0].status,
            ReadTerminalStatus::Cancelled(ReadCancelReason::Cancelled)
        ));
    }
}

#[test]
fn drop_cancels_inflight_and_later_pending_needs_separately() {
    let (mut engine, observed) = engine_with_sink();
    let now = Instant::now();
    engine
        .request_refresh(SessionId::FIRST, ItemKey::FIRST)
        .unwrap();
    let due = engine.due(now + Duration::from_secs(2), Duration::from_secs(1), None);
    let [Due::Read { keys, permit, .. }] = due.as_slice() else {
        panic!("read due")
    };
    let id = engine
        .send_due_read(SessionId::FIRST, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    engine
        .request_refresh(SessionId::FIRST, ItemKey::FIRST)
        .unwrap();
    drop(engine);
    let terminals = observed.lock().unwrap();
    assert_eq!(terminals.len(), 2);
    assert_eq!(
        terminals
            .iter()
            .filter(|t| t.request_id == Some(id))
            .count(),
        1
    );
    assert_eq!(
        terminals.iter().filter(|t| t.request_id.is_none()).count(),
        1
    );
    assert!(terminals.iter().all(|t| matches!(
        t.status,
        ReadTerminalStatus::Cancelled(ReadCancelReason::Cancelled)
    )));
}
