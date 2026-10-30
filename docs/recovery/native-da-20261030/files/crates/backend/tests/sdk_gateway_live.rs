//! Strict fixture test for the opt-in DA SDK Gateway. Requires the registered
//! fixture and fails if it is absent; it is never silently skipped.
#![cfg(all(windows, feature = "native-da-preview"))]
use opclook_backend::bff::Gateway;
use opclook_contract::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NativeWriteStatus,
    Origin, Owner, Protocol, RequestId, SessionId, Value, VarType, WriteDisposition, WriteRequest,
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
    let endpoint = EndpointSpec::da("localhost", "{7D4BD1B1-7422-48C0-AD58-601B701449C6}");
    assert!(matches!(
        command(&gateway, 1, CommandKind::Connect { endpoint }).kind,
        EventKind::Connected { .. }
    ));
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
    let shutdown = gateway.shutdown();
    assert_eq!(shutdown.da, opclook_backend::bff::ShutdownOutcome::Joined);
    opclook_backend::bff::finish_native_sdk(Duration::from_secs(10)).unwrap();
}
