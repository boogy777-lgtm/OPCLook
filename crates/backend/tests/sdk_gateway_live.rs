//! Strict fixture test for the opt-in DA SDK Gateway. Requires the registered
//! fixture and fails if it is absent; it is never silently skipped.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::Gateway;
use opclook_contract::model::{
    AcquisitionConfig, Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec,
    NativeWriteStatus, NodeClassFilter, NodeKind, Origin, Owner, PollFreshness, Protocol,
    RequestId, SessionId, SubscriptionState, UpdateMode, Value, VarType, WriteDisposition,
    WriteRequest,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

thread_local! { static PENDING: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) }; }
fn next_event(gateway: &Gateway) -> Option<Event> {
    PENDING
        .with(|pending| pending.borrow_mut().pop_front())
        .or_else(|| gateway.try_recv())
}

fn reply(gateway: &Gateway, id: RequestId) -> Event {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = gateway.try_recv() {
            if event.origin == Origin::Reply(id) {
                return event;
            }
            if matches!(event.origin, Origin::Session(_)) {
                PENDING.with(|pending| pending.borrow_mut().push_back(event));
            }
        }
        assert!(
            Instant::now() < until,
            "SDK Gateway did not produce reply {id}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn command(gateway: &Gateway, id: u64, kind: CommandKind) -> Event {
    eprintln!("gateway fixture: dispatch request {id}: {kind:?}");
    let request_id = RequestId::new(id).unwrap();
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Session(SessionId::FIRST),
            request_id: Some(request_id),
            kind,
        })
        .unwrap();
    let response = reply(gateway, request_id);
    eprintln!("gateway fixture: reply request {id}: {:?}", response.kind);
    response
}
#[test]
fn sdk_gateway_live() {
    eprintln!("gateway fixture: create Gateway");
    let mut gateway = Gateway::new();
    eprintln!("gateway fixture: dispatch discovery");
    let discovery_id = RequestId::new(90).unwrap();
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Discovery,
            request_id: Some(discovery_id),
            kind: CommandKind::Discover {
                host: "localhost".into(),
            },
        })
        .unwrap();
    assert!(
        matches!(reply(&gateway, discovery_id).kind, EventKind::Servers { servers, .. }
        if servers.iter().any(|server| server.id.eq_ignore_ascii_case("OPCLook.NativeDaFixture.1")))
    );
    eprintln!("gateway fixture: discovery reply verified");
    let endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    assert!(matches!(
        command(&gateway, 1, CommandKind::Connect { endpoint }).kind,
        EventKind::Connected { .. }
    ));
    let browse = command(
        &gateway,
        91,
        CommandKind::Browse {
            node_id: String::new(),
            filter: NodeClassFilter::All,
            view_id: None,
        },
    );
    let branch = match browse.kind {
        EventKind::Browse { children, .. } => {
            children
                .into_iter()
                .find(|child| child.name == "Fixture" && child.kind == NodeKind::Branch)
                .expect("SDK Browse root must contain Fixture branch")
                .node_id
        }
        other => panic!("SDK Browse root failed: {other:?}"),
    };
    let browse = command(
        &gateway,
        92,
        CommandKind::Browse {
            node_id: branch.clone(),
            filter: NodeClassFilter::All,
            view_id: None,
        },
    );
    match browse.kind {
        EventKind::Browse { children, .. } => {
            assert!(children.iter().any(|node| node.name == "Value"
                && node.node_id == "Fixture.Value"
                && node.kind == NodeKind::Item));
            assert!(children.iter().any(|node| node.name == "Тег"
                && node.node_id == "Fixture.Тег"
                && node.kind == NodeKind::Item));
        }
        other => panic!("SDK Browse branch failed: {other:?}"),
    }
    let key = ItemKey::FIRST;
    assert!(
        matches!(command(&gateway, 2, CommandKind::AddItems { items: vec![ItemSpec { key, item_id: "Fixture.Value".into() }] }).kind,
        EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    assert!(
        matches!(command(&gateway, 3, CommandKind::Read { keys: vec![key] }).kind,
        EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(42) && values[0].1.reason.is_none())
    );
    let scalar_items = [
        (
            2,
            "Fixture.Bool",
            Value::Bool(true),
            Value::Bool(false),
            VarType::BOOL,
        ),
        (
            3,
            "Fixture.Int64",
            Value::I64(-9_007_199_254_740_993),
            Value::I64(-17),
            VarType::I8,
        ),
        (
            4,
            "Fixture.UInt64",
            Value::U64(18_446_744_073_709_551_600),
            Value::U64(17),
            VarType::UI8,
        ),
        (
            5,
            "Fixture.Float64",
            Value::F64(12.5),
            Value::F64(2.5),
            VarType::R8,
        ),
        (
            13,
            "Fixture.DateBeforeEpoch",
            Value::DateTime(chrono::DateTime::from_timestamp(-2_209_226_400, 0).unwrap()),
            Value::DateTime(chrono::DateTime::from_timestamp(-2_209_183_200, 0).unwrap()),
            VarType(7),
        ),
    ];
    let specs = scalar_items
        .iter()
        .map(|(n, id, _, _, _)| ItemSpec {
            key: ItemKey::new(*n).unwrap(),
            item_id: (*id).into(),
        })
        .collect();
    assert!(
        matches!(command(&gateway, 20, CommandKind::AddItems { items: specs }).kind,
        EventKind::AddItems { results } if results.len() == scalar_items.len() && results.iter().all(|(_, result)| result.is_ok()))
    );
    for (index, (n, _, initial, written, vt)) in scalar_items.iter().enumerate() {
        let item = ItemKey::new(*n).unwrap();
        let read = command(
            &gateway,
            21 + index as u64 * 3,
            CommandKind::Read { keys: vec![item] },
        );
        assert!(
            matches!(read.kind, EventKind::Values { values } if values.len() == 1 && &values[0].1.value == initial && values[0].1.reason.is_none())
        );
        let write = command(
            &gateway,
            22 + index as u64 * 3,
            CommandKind::Write {
                request: WriteRequest {
                    key: item,
                    value: written.clone(),
                    type_hint: Some(*vt),
                },
            },
        );
        assert!(
            matches!(write.kind, EventKind::Wrote { result: Ok(outcome), .. }
            if outcome.disposition == WriteDisposition::Accepted && matches!(outcome.raw, Some(NativeWriteStatus::Da(_))))
        );
        let readback = command(
            &gateway,
            23 + index as u64 * 3,
            CommandKind::Read { keys: vec![item] },
        );
        assert!(
            matches!(readback.kind, EventKind::Values { values } if values.len() == 1 && &values[0].1.value == written && values[0].1.reason.is_none())
        );
    }
    let array_items = [
        (
            9,
            "Fixture.Bytes",
            Value::Array(vec![Value::U64(0), Value::U64(127), Value::U64(255)]),
            Value::Array(vec![Value::U64(255), Value::U64(1), Value::U64(0)]),
            VarType(0x2011),
        ),
        (
            10,
            "Fixture.Ints",
            Value::Array(vec![Value::I64(-2), Value::I64(0), Value::I64(42)]),
            Value::Array(vec![Value::I64(17), Value::I64(-19)]),
            VarType(0x2003),
        ),
        (
            11,
            "Fixture.Strings",
            Value::Array(vec![
                Value::Str("РУГ 中文".into()),
                Value::Str(String::new()),
            ]),
            Value::Array(vec![
                Value::Str("中文".into()),
                Value::Str("Русский".into()),
            ]),
            VarType(0x2008),
        ),
        (
            12,
            "Fixture.Mixed",
            Value::Array(vec![
                Value::I64(-42),
                Value::Str("РУГ 中文".into()),
                Value::Array(vec![Value::U64(2), Value::U64(255)]),
            ]),
            Value::Array(vec![
                Value::I64(7),
                Value::Str("中文".into()),
                Value::Array(vec![Value::U64(9)]),
            ]),
            VarType(0x200c),
        ),
    ];
    let specs = array_items
        .iter()
        .map(|(n, id, _, _, _)| ItemSpec {
            key: ItemKey::new(*n).unwrap(),
            item_id: (*id).into(),
        })
        .collect();
    assert!(
        matches!(command(&gateway, 200, CommandKind::AddItems { items: specs }).kind,
        EventKind::AddItems { results } if results.len() == 4 && results.iter().all(|(_, r)| r.is_ok()))
    );
    for (index, (n, _, initial, written, vt)) in array_items.iter().enumerate() {
        let key = ItemKey::new(*n).unwrap();
        assert!(
            matches!(command(&gateway, 201 + index as u64 * 3, CommandKind::Read { keys: vec![key] }).kind,
            EventKind::Values { values } if values.len() == 1 && &values[0].1.value == initial && values[0].1.reason.is_none())
        );
        assert!(
            matches!(command(&gateway, 202 + index as u64 * 3, CommandKind::Write { request: WriteRequest {
            key, value: written.clone(), type_hint: Some(*vt),
        }}).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
        );
        assert!(
            matches!(command(&gateway, 203 + index as u64 * 3, CommandKind::Read { keys: vec![key] }).kind,
            EventKind::Values { values } if values.len() == 1 && &values[0].1.value == written && values[0].1.reason.is_none())
        );
    }
    let ints_key = ItemKey::new(10).unwrap();
    assert!(
        matches!(command(&gateway, 250, CommandKind::Write { request: WriteRequest {
        key: ints_key, value: Value::I64(-5), type_hint: Some(VarType(0x2003)),
    }}).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    assert!(
        matches!(command(&gateway, 251, CommandKind::Read { keys: vec![ints_key] }).kind,
        EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::Array(vec![Value::I64(-5)]))
    );
    let text_items = [
        (6, "Fixture.Text", "initial text", "written text"),
        (7, "Fixture.Unicode", "РУГ 中文", "中文 Русский"),
    ];
    let specs = text_items
        .iter()
        .map(|(n, id, _, _)| ItemSpec {
            key: ItemKey::new(*n).unwrap(),
            item_id: (*id).into(),
        })
        .chain(std::iter::once(ItemSpec {
            key: ItemKey::new(8).unwrap(),
            item_id: "Fixture.Тег".into(),
        }))
        .collect();
    assert!(
        matches!(command(&gateway, 40, CommandKind::AddItems { items: specs }).kind,
        EventKind::AddItems { results } if results.len() == 3 && results.iter().all(|(_, result)| result.is_ok()))
    );
    for (index, (n, _, initial, written)) in text_items.iter().enumerate() {
        let item = ItemKey::new(*n).unwrap();
        let read = command(
            &gateway,
            41 + index as u64 * 3,
            CommandKind::Read { keys: vec![item] },
        );
        assert!(
            matches!(read.kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::Str((*initial).into()) && values[0].1.reason.is_none())
        );
        let write = command(
            &gateway,
            42 + index as u64 * 3,
            CommandKind::Write {
                request: WriteRequest {
                    key: item,
                    value: Value::Str((*written).into()),
                    type_hint: Some(VarType::BSTR),
                },
            },
        );
        assert!(
            matches!(write.kind, EventKind::Wrote { result: Ok(outcome), .. }
            if outcome.disposition == WriteDisposition::Accepted && matches!(outcome.raw, Some(NativeWriteStatus::Da(_))))
        );
        let readback = command(
            &gateway,
            43 + index as u64 * 3,
            CommandKind::Read { keys: vec![item] },
        );
        assert!(
            matches!(readback.kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::Str((*written).into()) && values[0].1.reason.is_none())
        );
    }
    let text_key = ItemKey::new(6).unwrap();
    assert!(
        matches!(command(&gateway, 240, CommandKind::Write { request: WriteRequest {
        key: text_key, value: Value::I64(42), type_hint: Some(VarType::BSTR),
    }}).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    assert!(
        matches!(command(&gateway, 241, CommandKind::Read { keys: vec![text_key] }).kind,
        EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::Str("42".into()))
    );
    assert!(
        matches!(command(&gateway, 50, CommandKind::Read { keys: vec![ItemKey::new(8).unwrap()] }).kind,
        EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(314) && values[0].1.reason.is_none())
    );
    assert!(matches!(
        command(
            &gateway,
            4,
            CommandKind::Write {
                request: WriteRequest {
                    key,
                    value: Value::I64(77),
                    type_hint: None
                }
            }
        )
        .kind,
        EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted && matches!(outcome.raw, Some(NativeWriteStatus::Da(_)))
    ));
    assert!(
        matches!(command(&gateway, 5, CommandKind::Read { keys: vec![key] }).kind,
        EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(77) && values[0].1.reason.is_none())
    );
    assert!(matches!(
        command(&gateway, 6, CommandKind::RemoveItems { keys: vec![key] }).kind,
        EventKind::Removed { .. }
    ));
    assert!(matches!(
        command(&gateway, 7, CommandKind::Disconnect).kind,
        EventKind::Disconnected { .. }
    ));
    let endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    assert!(matches!(
        command(&gateway, 101, CommandKind::Connect { endpoint }).kind,
        EventKind::Connected { .. }
    ));
    assert!(
        matches!(command(&gateway, 104, CommandKind::Browse { node_id: String::new(),
        filter: NodeClassFilter::All, view_id: None }).kind,
        EventKind::Browse { children, .. } if children.iter().any(|node| node.name == "Fixture"))
    );
    assert!(
        matches!(command(&gateway, 102, CommandKind::Browse { node_id: branch,
        filter: NodeClassFilter::All, view_id: None }).kind,
        EventKind::Browse { children, .. } if children.iter().any(|node| node.node_id == "Fixture.Value"))
    );
    assert!(matches!(
        command(&gateway, 103, CommandKind::Disconnect).kind,
        EventKind::Disconnected { .. }
    ));
    let mut endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    endpoint.acquisition = Some(AcquisitionConfig {
        update_mode: UpdateMode::Subscription,
        interval_ms: 100,
        read_after_write: false,
        poll_freshness: PollFreshness::CacheAllowed,
    });
    assert!(matches!(
        command(&gateway, 120, CommandKind::Connect { endpoint }).kind,
        EventKind::Connected { .. }
    ));
    let epoch = RequestId::new(120).unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = next_event(&gateway) {
            if event.origin == Origin::Session(epoch)
                && matches!(
                    event.kind,
                    EventKind::SubscriptionState {
                        state: SubscriptionState::Active
                    }
                )
            {
                break;
            }
        }
        assert!(
            Instant::now() < until,
            "native DA subscription was not activated"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let sub_key = ItemKey::new(9).unwrap();
    assert!(
        matches!(command(&gateway, 121, CommandKind::AddItems { items: vec![ItemSpec {
        key: sub_key, item_id: "Fixture.Value".into(),
    }] }).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = next_event(&gateway) {
            if event.origin == Origin::Session(epoch)
                && matches!(event.kind,
                EventKind::Values { values } if values.iter().any(|(key, value)| *key == sub_key && value.value == Value::I64(77)))
            {
                break;
            }
        }
        assert!(
            Instant::now() < until,
            "native DA initial callback not delivered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(command(&gateway, 122, CommandKind::Write { request: WriteRequest {
        key: sub_key, value: Value::I64(88), type_hint: None,
    }}).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = next_event(&gateway) {
            if event.origin == Origin::Session(epoch)
                && matches!(event.kind,
                EventKind::Values { values } if values.iter().any(|(key, value)| *key == sub_key && value.value == Value::I64(88)))
            {
                break;
            }
        }
        assert!(
            Instant::now() < until,
            "native DA write DataChange not delivered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        command(
            &gateway,
            123,
            CommandKind::RemoveItems {
                keys: vec![sub_key]
            }
        )
        .kind,
        EventKind::Removed { .. }
    ));
    assert!(
        matches!(command(&gateway, 125, CommandKind::AddItems { items: vec![ItemSpec {
        key: sub_key, item_id: "Fixture.Value".into(),
    }] }).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    assert!(
        matches!(command(&gateway, 126, CommandKind::Write { request: WriteRequest {
        key: sub_key, value: Value::I64(99), type_hint: None,
    }}).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = next_event(&gateway) {
            if event.origin == Origin::Session(epoch)
                && matches!(event.kind,
                EventKind::Values { values } if values.iter().any(|(key, value)| *key == sub_key && value.value == Value::I64(99)))
            {
                break;
            }
        }
        assert!(
            Instant::now() < until,
            "native DA callback after re-registration not delivered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        command(
            &gateway,
            127,
            CommandKind::RemoveItems {
                keys: vec![sub_key]
            }
        )
        .kind,
        EventKind::Removed { .. }
    ));
    let mixed_key = ItemKey::new(11).unwrap();
    assert!(
        matches!(command(&gateway, 128, CommandKind::AddItems { items: vec![ItemSpec {
        key: mixed_key, item_id: "Fixture.Mixed".into(),
    }] }).kind, EventKind::AddItems { results } if results.len() == 1 && results[0].1.is_ok())
    );
    let mixed_value = Value::Array(vec![
        Value::I64(-1),
        Value::Str("push 中文".into()),
        Value::Array(vec![Value::U64(3)]),
    ]);
    assert!(
        matches!(command(&gateway, 129, CommandKind::Write { request: WriteRequest {
        key: mixed_key, value: mixed_value.clone(), type_hint: Some(VarType(0x200c)),
    }}).kind, EventKind::Wrote { result: Ok(outcome), .. } if outcome.disposition == WriteDisposition::Accepted)
    );
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = next_event(&gateway) {
            if event.origin == Origin::Session(epoch)
                && matches!(event.kind,
                EventKind::Values { values } if values.iter().any(|(key, value)| *key == mixed_key && value.value == mixed_value))
            {
                break;
            }
        }
        assert!(
            Instant::now() < until,
            "native DA variant array DataChange not delivered"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        command(
            &gateway,
            130,
            CommandKind::RemoveItems {
                keys: vec![mixed_key]
            }
        )
        .kind,
        EventKind::Removed { .. }
    ));
    assert!(matches!(
        command(&gateway, 124, CommandKind::Disconnect).kind,
        EventKind::Disconnected { .. }
    ));
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, opclook_backend::bff::ShutdownOutcome::Joined);
    opclook_backend::bff::finish_native_sdk(Duration::from_secs(10)).unwrap();
}
