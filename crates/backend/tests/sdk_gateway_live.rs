//! Strict fixture test for the opt-in DA SDK Gateway. Requires the registered
//! fixture and fails if it is absent; it is never silently skipped.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::Gateway;
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NativeWriteStatus,
    NodeClassFilter, NodeKind, Origin, Owner, Protocol, RequestId, SessionId, Value, VarType, WriteDisposition, WriteRequest,
};
use std::time::{Duration, Instant};

fn reply(gateway: &Gateway, id: RequestId) -> Event {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(event) = gateway.try_recv() {
            if event.origin == Origin::Reply(id) {
                return event;
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
    let request_id = RequestId::new(id).unwrap();
    gateway
        .send(Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Session(SessionId::FIRST),
            request_id: Some(request_id),
            kind,
        })
        .unwrap();
    reply(gateway, request_id)
}
#[test]
fn sdk_gateway_live() {
    let mut gateway = Gateway::new();
    let discovery_id = RequestId::new(90).unwrap();
    gateway.send(Command { protocol: Protocol::OpcDa, owner: Owner::Discovery,
        request_id: Some(discovery_id), kind: CommandKind::Discover { host: "localhost".into() } }).unwrap();
    assert!(matches!(reply(&gateway, discovery_id).kind, EventKind::Servers { servers, .. }
        if servers.iter().any(|server| server.id.eq_ignore_ascii_case("OPCLook.NativeDaFixture.1"))));
    let endpoint = EndpointSpec::da("localhost", "OPCLook.NativeDaFixture.1");
    assert!(matches!(
        command(&gateway, 1, CommandKind::Connect { endpoint }).kind,
        EventKind::Connected { .. }
    ));
    let browse = command(&gateway, 91, CommandKind::Browse {
        node_id: String::new(), filter: NodeClassFilter::All, view_id: None,
    });
    let branch = match browse.kind {
        EventKind::Browse { children, .. } => children.into_iter()
            .find(|child| child.name == "Fixture" && child.kind == NodeKind::Branch)
            .expect("SDK Browse root must contain Fixture branch").node_id,
        other => panic!("SDK Browse root failed: {other:?}"),
    };
    let browse = command(&gateway, 92, CommandKind::Browse {
        node_id: branch.clone(), filter: NodeClassFilter::All, view_id: None,
    });
    match browse.kind {
        EventKind::Browse { children, .. } => {
            assert!(children.iter().any(|node| node.name == "Value" && node.node_id == "Fixture.Value" && node.kind == NodeKind::Item));
            assert!(children.iter().any(|node| node.name == "Тег" && node.node_id == "Fixture.Тег" && node.kind == NodeKind::Item));
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
    let text_items = [
        (6, "Fixture.Text", "initial text", "written text"),
        (7, "Fixture.Unicode", "РУГ 中文", "中文 Русский"),
    ];
    let specs = text_items.iter().map(|(n, id, _, _)| ItemSpec {
        key: ItemKey::new(*n).unwrap(), item_id: (*id).into(),
    }).chain(std::iter::once(ItemSpec { key: ItemKey::new(8).unwrap(), item_id: "Fixture.Тег".into() })).collect();
    assert!(matches!(command(&gateway, 40, CommandKind::AddItems { items: specs }).kind,
        EventKind::AddItems { results } if results.len() == 3 && results.iter().all(|(_, result)| result.is_ok())));
    for (index, (n, _, initial, written)) in text_items.iter().enumerate() {
        let item = ItemKey::new(*n).unwrap();
        let read = command(&gateway, 41 + index as u64 * 3, CommandKind::Read { keys: vec![item] });
        assert!(matches!(read.kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::Str((*initial).into()) && values[0].1.reason.is_none()));
        let write = command(&gateway, 42 + index as u64 * 3, CommandKind::Write { request: WriteRequest {
            key: item, value: Value::Str((*written).into()), type_hint: Some(VarType::BSTR),
        }});
        assert!(matches!(write.kind, EventKind::Wrote { result: Ok(outcome), .. }
            if outcome.disposition == WriteDisposition::Accepted && matches!(outcome.raw, Some(NativeWriteStatus::Da(_)))));
        let readback = command(&gateway, 43 + index as u64 * 3, CommandKind::Read { keys: vec![item] });
        assert!(matches!(readback.kind, EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::Str((*written).into()) && values[0].1.reason.is_none()));
    }
    assert!(matches!(command(&gateway, 50, CommandKind::Read { keys: vec![ItemKey::new(8).unwrap()] }).kind,
        EventKind::Values { values } if values.len() == 1 && values[0].1.value == Value::I64(314) && values[0].1.reason.is_none()));
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
    assert!(matches!(command(&gateway, 101, CommandKind::Connect { endpoint }).kind, EventKind::Connected { .. }));
    assert!(matches!(command(&gateway, 104, CommandKind::Browse { node_id: String::new(),
        filter: NodeClassFilter::All, view_id: None }).kind,
        EventKind::Browse { children, .. } if children.iter().any(|node| node.name == "Fixture")));
    assert!(matches!(command(&gateway, 102, CommandKind::Browse { node_id: branch,
        filter: NodeClassFilter::All, view_id: None }).kind,
        EventKind::Browse { children, .. } if children.iter().any(|node| node.node_id == "Fixture.Value")));
    assert!(matches!(command(&gateway, 103, CommandKind::Disconnect).kind, EventKind::Disconnected { .. }));
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, opclook_backend::bff::ShutdownOutcome::Joined);
    opclook_backend::bff::finish_native_sdk(Duration::from_secs(10)).unwrap();
}
