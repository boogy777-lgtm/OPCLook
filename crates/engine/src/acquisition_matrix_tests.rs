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

fn csv_rows(data: &str) -> Vec<Vec<&str>> {
    data.lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split(',').collect())
        .collect()
}
fn bit(row: &[&str], i: usize) -> bool {
    assert!(row[i] == "0" || row[i] == "1");
    row[i] == "1"
}

fn reset(engine: &mut Engine<()>) {
    engine.sessions.clear();
    engine.requests.clear();
    engine.pending_adds.clear();
    engine.timeout_queued.clear();
    engine.route_events.clear();
    engine.next = Some(RequestId::FIRST);
    engine.next_permit = Some(1);
    engine.test_commands.borrow_mut().clear();
}

fn register(
    engine: &mut Engine<()>,
    now: Instant,
    profile: AcquisitionConfig,
    add: bool,
) -> (SessionId, ItemKey) {
    register_as(engine, now, profile, add, SessionId::FIRST)
}
fn register_as(
    engine: &mut Engine<()>,
    now: Instant,
    profile: AcquisitionConfig,
    add: bool,
    session: SessionId,
) -> (SessionId, ItemKey) {
    let key = ItemKey::FIRST;
    let mut endpoint = EndpointSpec::ua("opc.tcp://127.0.0.1:4840");
    endpoint.acquisition = Some(profile);
    let id = engine
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
                origin: Origin::Reply(id),
                kind: EventKind::Connected {
                    endpoint,
                    info: BackendMsg::Raw(String::new()),
                },
            },
            now,
        )
        .unwrap();
    if add {
        let id = engine
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
                    origin: Origin::Reply(id),
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
    }
    (session, key)
}
fn finish_read(
    engine: &mut Engine<()>,
    session: SessionId,
    id: RequestId,
    now: Instant,
) -> PendingRequest<()> {
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(id),
                kind: EventKind::Values { values: vec![] },
            },
            now,
        )
        .unwrap()
        .unwrap()
}

#[test]
fn d_csv_128_cases_exercise_real_demand_creation_and_dispatch() {
    use opclook_contract::model::{ReadNeed, UpdateMode, Value, WriteOutcome, WriteRequest};
    let rows = csv_rows(include_str!("../../../docs/testing/acquisition-demand.csv"));
    assert_eq!(rows.len(), 128);
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let now = Instant::now();
    let mut issued = 0;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), 9);
        assert_eq!(row[0], format!("D{index:03}"));
        let (poll, timer, initial, grace, wrote, readback, refresh, expected) = (
            bit(row, 1),
            bit(row, 2),
            bit(row, 3),
            bit(row, 4),
            bit(row, 5),
            bit(row, 6),
            bit(row, 7),
            bit(row, 8),
        );
        reset(&mut engine);
        let mut profile = AcquisitionConfig::ua_default();
        profile.update_mode = if poll {
            UpdateMode::Poll
        } else {
            UpdateMode::Subscription
        };
        profile.interval_ms = 1000;
        profile.read_after_write = readback;
        let registered_at = if grace {
            now - Duration::from_secs(2)
        } else {
            now
        };
        let (session, key) = register(&mut engine, registered_at, profile, true);
        if !initial {
            let id = engine
                .send(
                    session,
                    Protocol::OpcUa,
                    CommandKind::Read { keys: vec![key] },
                    (),
                )
                .unwrap();
            finish_read(&mut engine, session, id, now);
        }
        if wrote {
            let id = engine
                .send(
                    session,
                    Protocol::OpcUa,
                    CommandKind::Write {
                        request: WriteRequest {
                            key,
                            value: Value::I64(42),
                            type_hint: None,
                        },
                    },
                    (),
                )
                .unwrap();
            engine
                .accept_at(
                    &Event {
                        owner: session.into(),
                        origin: Origin::Reply(id),
                        kind: EventKind::Wrote {
                            key,
                            result: Ok(WriteOutcome::from_ua(0, "Good")),
                        },
                    },
                    now,
                )
                .unwrap();
        }
        if refresh {
            engine.request_refresh(session, key).unwrap();
        }
        engine.sessions.get_mut(&session).unwrap().last_poll = if timer {
            now - Duration::from_secs(2)
        } else {
            now
        };
        let due = engine.due(now, Duration::from_secs(1), None);
        let read = due.iter().find_map(|v| match v {
            Due::Read {
                session: s,
                keys,
                permit,
                ..
            } if *s == session => Some((keys.clone(), *permit)),
            _ => None,
        });
        assert_eq!(read.is_some(), expected, "{}", row[0]);
        if let Some((keys, permit)) = read {
            assert_eq!(keys, vec![key], "{}", row[0]);
            let id = engine
                .send_due_read(session, Protocol::OpcUa, keys, permit, ())
                .unwrap();
            let terminal = finish_read(&mut engine, session, id, now);
            let needs = &terminal.read_terminals[0].needs;
            if refresh {
                assert!(needs.contains(&ReadNeed::ExplicitRefresh), "{}", row[0]);
            }
            if wrote && readback {
                assert!(needs.contains(&ReadNeed::WriteReadback), "{}", row[0]);
            }
            if initial && (poll || grace) {
                assert!(needs.contains(&ReadNeed::Initial), "{}", row[0]);
            }
            if poll && timer {
                assert!(needs.contains(&ReadNeed::Poll), "{}", row[0]);
            }
            issued += 1;
        }
    }
    eprintln!(
        "D: {} CSV rows, {} actual dispatched reads, {} no-read",
        rows.len(),
        issued,
        rows.len() - issued
    );
}

#[test]
fn g_csv_128_rows_check_reachable_admission_and_early_guards() {
    use opclook_contract::model::{AcquisitionConfig, NodeClassFilter, PollFreshness, UpdateMode};
    let rows = csv_rows(include_str!(
        "../../../docs/testing/acquisition-read-gate.csv"
    ));
    assert_eq!(rows.len(), 128);
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let now = Instant::now();
    let mut reachable = 0;
    let mut disconnected = 0;
    let mut stale_epoch = 0;
    let mut empty_keys = 0;
    let mut unsupported = 0;
    let mut unavailable_capability = 0;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), 9);
        assert_eq!(row[0], format!("G{index:03}"));
        let (connected, epoch, keys, in_flight, browsing, demand, supported, expected) = (
            bit(row, 1),
            bit(row, 2),
            bit(row, 3),
            bit(row, 4),
            bit(row, 5),
            bit(row, 6),
            bit(row, 7),
            bit(row, 8),
        );
        reset(&mut engine);
        if !supported {
            assert!(!expected, "{}", row[0]);
            if supports_fresh_source(Protocol::OpcDa) {
                unavailable_capability += 1;
                continue;
            }
            let mut endpoint = EndpointSpec::da("", "test");
            let mut profile = AcquisitionConfig::da_available();
            profile.poll_freshness = PollFreshness::FreshSource;
            endpoint.acquisition = Some(profile);
            assert!(
                matches!(
                    engine.send(
                        SessionId::FIRST,
                        Protocol::OpcDa,
                        CommandKind::Connect { endpoint },
                        ()
                    ),
                    Err(DispatchError::InvalidAcquisition(
                        AcquisitionError::FreshnessUnsupported
                    ))
                ),
                "{}",
                row[0]
            );
            assert!(engine.requests.is_empty());
            unsupported += 1;
            continue;
        }
        let mut profile = AcquisitionConfig::ua_default();
        profile.update_mode = UpdateMode::Subscription;
        profile.read_after_write = false;
        profile.interval_ms = 1000;
        let (session, key) = register(&mut engine, now, profile, keys);
        engine.sessions.get_mut(&session).unwrap().last_poll = now;
        if !connected || !epoch {
            if demand && keys {
                engine.request_refresh(session, key).unwrap();
            }
            let id = engine
                .send(session, Protocol::OpcUa, CommandKind::Disconnect, ())
                .unwrap();
            if !connected {
                engine
                    .accept_at(
                        &Event {
                            owner: session.into(),
                            origin: Origin::Reply(id),
                            kind: EventKind::Disconnected {
                                reason: BackendMsg::Raw(String::new()),
                            },
                        },
                        now,
                    )
                    .unwrap();
                assert!(matches!(
                    engine.request_refresh(session, key),
                    Err(DemandError::SessionUnavailable)
                ));
                disconnected += 1;
            } else {
                assert_eq!(engine.connection_id(session), None);
                stale_epoch += 1;
            }
            assert!(!expected, "{}", row[0]);
            assert!(
                !engine
                    .due(now + Duration::from_secs(2), Duration::from_secs(1), None)
                    .iter()
                    .any(|e| matches!(e, Due::Read { .. })),
                "{}",
                row[0]
            );
            continue;
        }
        if !keys {
            if demand {
                assert!(matches!(
                    engine.request_refresh(session, key),
                    Err(DemandError::ItemNotRegistered)
                ));
            }
            assert!(!expected, "{}", row[0]);
            assert!(
                !engine
                    .due(now, Duration::from_secs(1), None)
                    .iter()
                    .any(|e| matches!(e, Due::Read { .. })),
                "{}",
                row[0]
            );
            empty_keys += 1;
            continue;
        }
        // Settle the registration's initial read before arranging these bits.
        let initial = engine
            .send(
                session,
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key] },
                (),
            )
            .unwrap();
        finish_read(&mut engine, session, initial, now);
        // Start Browse before the demand: an already accepted Browse may
        // coexist with a later demand under the fairness policy.
        if browsing {
            engine
                .send(
                    session,
                    Protocol::OpcUa,
                    CommandKind::Browse {
                        node_id: String::new(),
                        filter: NodeClassFilter::All,
                        view_id: None,
                    },
                    (),
                )
                .unwrap();
        }
        // Exercise the slot through an actual direct Read, not a synthetic flag.
        if in_flight {
            engine
                .send(
                    session,
                    Protocol::OpcUa,
                    CommandKind::Read { keys: vec![key] },
                    (),
                )
                .unwrap();
        }
        if demand {
            engine.request_refresh(session, key).unwrap();
        }
        let due = engine.due(now, Duration::from_secs(1), None);
        let read = due.iter().find_map(|event| match event {
            Due::Read { keys, permit, .. } => Some((keys.clone(), *permit)),
            _ => None,
        });
        assert_eq!(read.is_some(), expected, "{}", row[0]);
        if let Some((batch, permit)) = read {
            assert_eq!(batch, vec![key]);
            let id = engine
                .send_due_read(session, Protocol::OpcUa, batch, permit, ())
                .unwrap();
            let terminal = finish_read(&mut engine, session, id, now);
            assert_eq!(terminal.read_terminals.len(), 1);
        }
        reachable += 1;
    }
    assert_eq!(
        reachable + disconnected + stale_epoch + empty_keys + unsupported + unavailable_capability,
        128
    );
    eprintln!(
        "G: {} fully reachable; early disconnected={disconnected}, epoch={stale_epoch}, empty={empty_keys}, unsupported={unsupported}, capability unavailable={unavailable_capability}",
        reachable
    );
}

#[test]
fn pending_refresh_must_not_starve_under_continuous_browse_and_other_session_progresses() {
    use opclook_contract::model::{NodeClassFilter, UpdateMode};
    let now = Instant::now();
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Subscription;
    profile.read_after_write = false;
    let (a, key) = register_as(&mut engine, now, profile.clone(), true, SessionId::FIRST);
    let (b, _) = register_as(&mut engine, now, profile, true, SessionId::new(2).unwrap());
    for session in [a, b] {
        let id = engine
            .send(
                session,
                Protocol::OpcUa,
                CommandKind::Read { keys: vec![key] },
                (),
            )
            .unwrap();
        finish_read(&mut engine, session, id, now);
        engine.request_refresh(session, key).unwrap();
    }
    let mut a_progress = false;
    let mut b_progress = false;
    for n in 0..8 {
        let browse = engine.send(
            a,
            Protocol::OpcUa,
            CommandKind::Browse {
                node_id: String::new(),
                filter: NodeClassFilter::All,
                view_id: None,
            },
            (),
        );
        if let Err(error) = &browse {
            assert_eq!(
                format!("{error:?}"),
                "ActionUnavailable(ReadPending)",
                "unexpected rejection"
            );
        }
        let at = now + Duration::from_millis(n * 100);
        for work in engine.due(at, Duration::from_secs(60), None) {
            if let Due::Read {
                session,
                protocol,
                keys,
                permit,
            } = work
            {
                let id = engine
                    .send_due_read(session, protocol, keys, permit, ())
                    .unwrap();
                let terminal = finish_read(&mut engine, session, id, at);
                assert!(!terminal.read_terminals.is_empty());
                if session == a {
                    a_progress = true;
                }
                if session == b {
                    b_progress = true;
                }
            }
        }
        if let Ok(id) = browse {
            engine
                .accept_at(
                    &Event {
                        owner: a.into(),
                        origin: Origin::Reply(id),
                        kind: EventKind::Browse {
                            node_id: String::new(),
                            children: vec![],
                        },
                    },
                    at,
                )
                .unwrap();
        }
    }
    assert!(b_progress, "session B must progress while A is busy");
    assert!(
        a_progress,
        "continuous Browse starves A's previously queued Refresh"
    );
}

#[test]
fn writes_before_dispatch_coalesce_but_later_write_and_push_need_another_read() {
    use opclook_contract::model::{
        DataValue, Quality, ReadNeed, UpdateMode, Value, WriteOutcome, WriteRequest,
    };
    let now = Instant::now();
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Subscription;
    profile.read_after_write = true;
    let (session, key) = register(&mut engine, now, profile, true);
    let initial = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Read { keys: vec![key] },
            (),
        )
        .unwrap();
    finish_read(&mut engine, session, initial, now);
    let write = |engine: &mut Engine<()>| {
        let id = engine
            .send(
                session,
                Protocol::OpcUa,
                CommandKind::Write {
                    request: WriteRequest {
                        key,
                        value: Value::I64(42),
                        type_hint: None,
                    },
                },
                (),
            )
            .unwrap();
        engine
            .accept_at(
                &Event {
                    owner: session.into(),
                    origin: Origin::Reply(id),
                    kind: EventKind::Wrote {
                        key,
                        result: Ok(WriteOutcome::from_ua(0, "good")),
                    },
                },
                now,
            )
            .unwrap();
    };
    write(&mut engine);
    write(&mut engine);
    let first_due = engine.due(now, Duration::from_secs(60), None);
    let [Due::Read { keys, permit, .. }] = first_due.as_slice() else {
        panic!("first read due")
    };
    let first = engine
        .send_due_read(session, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    write(&mut engine);
    let epoch = engine.connection_id(session).unwrap();
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Session(epoch),
                kind: EventKind::Values {
                    values: vec![(
                        key,
                        DataValue {
                            value: Value::I64(42),
                            quality: Quality::from_ua(0, "good"),
                            timestamp: None,
                            times: Default::default(),
                            reason: None,
                        },
                    )],
                },
            },
            now,
        )
        .unwrap();
    assert!(
        engine.due(now, Duration::from_secs(60), None).is_empty(),
        "one read slot must remain occupied"
    );
    let old = finish_read(&mut engine, session, first, now);
    assert_eq!(old.read_terminals.len(), 1);
    assert_eq!(
        old.read_terminals[0]
            .needs
            .iter()
            .filter(|n| **n == ReadNeed::WriteReadback)
            .count(),
        1
    );
    let second_due = engine.due(now, Duration::from_secs(60), None);
    let [Due::Read { keys, permit, .. }] = second_due.as_slice() else {
        panic!("later write still needs readback")
    };
    let second = engine
        .send_due_read(session, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    assert_ne!(first, second);
    let new = finish_read(&mut engine, session, second, now);
    assert_eq!(new.read_terminals.len(), 1);
    assert!(
        new.read_terminals[0]
            .needs
            .contains(&ReadNeed::WriteReadback)
    );
}

#[test]
fn late_read_reply_cannot_complete_a_new_connection_epoch() {
    use opclook_contract::model::{ReadNeed, UpdateMode};
    let now = Instant::now();
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Subscription;
    let (session, key) = register(&mut engine, now, profile.clone(), true);
    engine.request_refresh(session, key).unwrap();
    let old_due = engine.due(now, Duration::from_secs(60), None);
    let [Due::Read { keys, permit, .. }] = old_due.as_slice() else {
        panic!("old due")
    };
    let old = engine
        .send_due_read(session, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    engine
        .send(session, Protocol::OpcUa, CommandKind::Disconnect, ())
        .unwrap();
    // Reconnect through Engine; request IDs do not get reset.
    let (new_session, new_key) = register_as(&mut engine, now, profile, true, session);
    assert_eq!((new_session, new_key), (session, key));
    engine.request_refresh(session, key).unwrap();
    let next_due = engine.due(now, Duration::from_secs(60), None);
    let [Due::Read { keys, permit, .. }] = next_due.as_slice() else {
        panic!("new due")
    };
    let current = engine
        .send_due_read(session, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    assert_ne!(old, current);
    assert!(
        engine
            .accept_at(
                &Event {
                    owner: session.into(),
                    origin: Origin::Reply(old),
                    kind: EventKind::Values { values: vec![] }
                },
                now
            )
            .is_err()
    );
    assert!(engine.has_pending(session, current));
    let terminal = finish_read(&mut engine, session, current, now);
    assert!(
        terminal.read_terminals[0]
            .needs
            .contains(&ReadNeed::ExplicitRefresh)
    );
}

#[test]
fn removed_inflight_key_gets_one_cancel_and_late_reply_cannot_revive_it() {
    use opclook_contract::model::{ReadCancelReason, ReadTerminalStatus, UpdateMode};
    use std::sync::{Arc, Mutex};
    let now = Instant::now();
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Subscription;
    let (session, key) = register(&mut engine, now, profile, true);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = observed.clone();
    engine.set_read_terminal_sink(Some(Box::new(move |_, _, terminal| {
        sink.lock().unwrap().push(terminal.clone())
    })));
    engine.request_refresh(session, key).unwrap();
    let due = engine.due(now, Duration::from_secs(60), None);
    let [Due::Read { keys, permit, .. }] = due.as_slice() else {
        panic!("read due")
    };
    let read = engine
        .send_due_read(session, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    let remove = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::RemoveItems { keys: vec![key] },
            (),
        )
        .unwrap();
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(remove),
                kind: EventKind::Removed { keys: vec![key] },
            },
            now,
        )
        .unwrap();
    let pending = finish_read(&mut engine, session, read, now);
    assert!(pending.read_terminals.is_empty());
    assert!(!engine.session(session).unwrap().items.contains_key(&key));
    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert!(matches!(
        observed[0].status,
        ReadTerminalStatus::Cancelled(ReadCancelReason::Removed)
    ));
}

#[test]
fn multi_key_batch_reports_independent_needs_and_missing_value() {
    use opclook_contract::model::{
        DataValue, Quality, ReadNeed, ReadTerminalStatus, UpdateMode, Value, WriteOutcome,
        WriteRequest,
    };
    let now = Instant::now();
    let mut engine = Engine::<()>::default();
    engine.test_skip_dispatch = true;
    let mut profile = AcquisitionConfig::ua_default();
    profile.update_mode = UpdateMode::Subscription;
    profile.read_after_write = true;
    let (session, first) = register(&mut engine, now, profile, true);
    let second = ItemKey::new(2).unwrap();
    let id = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::AddItems {
                items: vec![ItemSpec {
                    key: second,
                    item_id: "ns=2;s=second".into(),
                }],
            },
            (),
        )
        .unwrap();
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(id),
                kind: EventKind::AddItems {
                    results: vec![(
                        second,
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
    let id = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Read {
                keys: vec![first, second],
            },
            (),
        )
        .unwrap();
    finish_read(&mut engine, session, id, now);
    let id = engine
        .send(
            session,
            Protocol::OpcUa,
            CommandKind::Write {
                request: WriteRequest {
                    key: first,
                    value: Value::I64(10),
                    type_hint: None,
                },
            },
            (),
        )
        .unwrap();
    engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(id),
                kind: EventKind::Wrote {
                    key: first,
                    result: Ok(WriteOutcome::from_ua(0, "good")),
                },
            },
            now,
        )
        .unwrap();
    engine.request_refresh(session, second).unwrap();
    let work = engine.due(now, Duration::from_secs(60), None);
    let [Due::Read { keys, permit, .. }] = work.as_slice() else {
        panic!("combined read")
    };
    assert_eq!(keys, &vec![first, second]);
    let id = engine
        .send_due_read(session, Protocol::OpcUa, keys.clone(), *permit, ())
        .unwrap();
    let pending = engine
        .accept_at(
            &Event {
                owner: session.into(),
                origin: Origin::Reply(id),
                kind: EventKind::Values {
                    values: vec![(
                        first,
                        DataValue {
                            value: Value::I64(10),
                            quality: Quality::from_ua(0, "good"),
                            timestamp: None,
                            times: Default::default(),
                            reason: None,
                        },
                    )],
                },
            },
            now,
        )
        .unwrap()
        .unwrap();
    assert_eq!(pending.read_terminals.len(), 2);
    let a = pending
        .read_terminals
        .iter()
        .find(|t| t.key == first)
        .unwrap();
    assert!(a.needs.contains(&ReadNeed::WriteReadback));
    assert!(matches!(a.status, ReadTerminalStatus::Received(_)));
    let b = pending
        .read_terminals
        .iter()
        .find(|t| t.key == second)
        .unwrap();
    assert!(b.needs.contains(&ReadNeed::ExplicitRefresh));
    assert!(matches!(b.status, ReadTerminalStatus::MissingResult));
}

#[test]
fn browse_uses_actual_legacy_initial_grace_and_does_not_block_periodic_poll() {
    let now = Instant::now();
    let (mut engine, session, key) = setup(now);
    {
        let view = engine.sessions.get_mut(&session).unwrap();
        view.endpoint.acquisition = None;
        view.items.insert(key, false);
        view.pending_initial.insert(key, now);
    }
    assert!(engine.browse_deferred_at(session, now));
    assert!(engine.due(now, Duration::from_secs(100), None).is_empty());
    assert!(!engine.browse_deferred_at(session, now + Duration::from_secs(2)));
    assert!(engine.browse_deferred_at(session, now + Duration::from_secs(100)));
    engine
        .sessions
        .get_mut(&session)
        .unwrap()
        .pending_initial
        .clear();
    assert!(
        !engine.browse_deferred_at(session, now + Duration::from_secs(1000)),
        "periodic poll alone must not starve browse"
    );
}
