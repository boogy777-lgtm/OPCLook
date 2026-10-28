use opcda_egui::bff::model::ItemKey;
use opcda_egui::bff::model::{
    BrowseNode, Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, NodeKind, Protocol,
    Value, WriteRequest,
};
use opclook_backend::bff::Gateway;
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";
const MAX_BROWSE: usize = 600;
const MAX_DEPTH: usize = 6;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_url() -> String {
    std::env::var("OPCUA_TEST_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn write_node_override() -> Option<String> {
    std::env::var("OPCUA_TEST_WRITE_NODE")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn wait_for(
    gateway: &Gateway,
    command: Command,
    timeout: Duration,
    predicate: impl Fn(&EventKind) -> bool,
) -> Option<Event> {
    let request_id = command.request_id;
    let owner = command.owner;
    gateway.send(command).expect("gateway send");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.origin != request_id.into() || event.owner != owner {
                continue;
            }
            if predicate(&event.kind) {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

fn connect(gateway: &Gateway, session: u64) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            owner: opcda_egui::bff::model::Owner::Session(
                opcda_egui::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(test_url()),
            },
        },
        Duration::from_secs(40),
        |kind| matches!(kind, EventKind::Connected { .. } | EventKind::Error { .. }),
    )
    .expect("connect timeout")
}

fn disconnect(gateway: &Gateway, session: u64) {
    let _ = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            owner: opcda_egui::bff::model::Owner::Session(
                opcda_egui::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(5),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    );
}

fn browse_children(gateway: &Gateway, session: u64, node_id: &str) -> Vec<BrowseNode> {
    let event = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            owner: opcda_egui::bff::model::Owner::Session(
                opcda_egui::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Browse {
                node_id: node_id.to_string(),
                filter: opcda_egui::bff::model::NodeClassFilter::All,
                view_id: None,
            },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
    )
    .expect("browse timeout");
    match event.kind {
        EventKind::Browse { children, .. } => children,
        other => panic!("browse {node_id} failed: {other:?}"),
    }
}

/// Depth-limited recursive browse collecting every Variable (leaf item) node id
/// together with its display name so the failure message can print the tree.
fn collect_items(
    gateway: &Gateway,
    session: u64,
    root: &str,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut items = Vec::new();
    let mut tree: Vec<(String, String)> = Vec::new();
    let mut stack: Vec<(String, usize)> = vec![(root.to_string(), 0)];
    let mut seen = std::collections::HashSet::new();
    while let Some((node, depth)) = stack.pop() {
        if items.len() >= MAX_BROWSE {
            break;
        }
        if depth > MAX_DEPTH || !seen.insert(node.clone()) {
            continue;
        }
        let children = browse_children(gateway, session, &node);
        tree.push((
            node.clone(),
            children
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        ));
        for child in children {
            if child.kind == NodeKind::Item {
                items.push(child.node_id.clone());
            } else if child.has_children {
                stack.push((child.node_id.clone(), depth + 1));
            }
        }
    }
    (items, tree)
}

fn add_items(gateway: &Gateway, session: u64, items: &[String]) -> Event {
    let specs: Vec<ItemSpec> = items
        .iter()
        .enumerate()
        .map(|(i, id)| ItemSpec {
            key: ItemKey::new((i + 1) as u32).unwrap(),
            item_id: id.clone(),
        })
        .collect();
    wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            owner: opcda_egui::bff::model::Owner::Session(
                opcda_egui::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems { items: specs },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::AddItems { .. }),
    )
    .expect("add items timeout")
}

fn read_value(gateway: &Gateway, session: u64, key: ItemKey) -> Value {
    let event = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            owner: opcda_egui::bff::model::Owner::Session(
                opcda_egui::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Read { keys: vec![key] },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::Values { .. }),
    )
    .expect("read timeout");
    match event.kind {
        EventKind::Values { values } => {
            values
                .into_iter()
                .find(|(k, _)| *k == key)
                .expect("missing key in read response")
                .1
                .value
        }
        other => panic!("read failed: {other:?}"),
    }
}

fn write_value(gateway: &Gateway, session: u64, key: ItemKey, value: Value) {
    let event = wait_for(
        gateway,
        Command {
            request_id: opcda_egui::bff::model::RequestId::new(next_id()),
            owner: opcda_egui::bff::model::Owner::Session(
                opcda_egui::bff::model::SessionId::new(session).unwrap(),
            ),
            protocol: Protocol::OpcUa,
            kind: CommandKind::Write {
                request: WriteRequest {
                    key,
                    value,
                    type_hint: None,
                },
            },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::Wrote { .. }),
    )
    .expect("write timeout");
    match event.kind {
        EventKind::Wrote { result, .. } => {
            assert!(result.is_ok(), "write failed: {result:?}");
        }
        other => panic!("write returned unexpected event: {other:?}"),
    }
}

fn differing_value(v: &Value) -> Value {
    match v {
        Value::Bool(b) => Value::Bool(!*b),
        Value::I64(n) => Value::I64(n.saturating_add(1)),
        Value::U64(n) => Value::U64(n.saturating_add(1)),
        Value::F64(n) => Value::F64(*n + 1.0),
        Value::Str(s) => Value::Str(format!("{s}x")),
        Value::Bytes(b) => {
            let mut c = b.clone();
            c.push(0x00);
            Value::Bytes(c)
        }
        Value::DateTime(dt) => Value::DateTime(*dt + chrono::Duration::seconds(1)),
        Value::Empty
        | Value::Other(_)
        | Value::Array(_)
        | Value::NodeId(_)
        | Value::QualifiedName { .. }
        | Value::LocalizedText { .. }
        | Value::Struct { .. } => v.clone(),
    }
}

fn find_writable(
    gateway: &Gateway,
    session: u64,
    candidates: &[String],
) -> Option<(ItemKey, String, String, Value)> {
    // Add candidates as items (this resolves DataType + AccessLevel) and inspect each
    // for writable + known scalar type. For any writable candidate, qualify it further
    // with a trial write/read-back/restore so we only target a value that actually sticks.
    let ids: Vec<String> = candidates.iter().take(80).cloned().collect();
    if ids.is_empty() {
        return None;
    }
    let add = add_items(gateway, session, &ids);
    let results = match add.kind {
        EventKind::AddItems { results } => results,
        other => panic!("unexpected add: {other:?}"),
    };
    let mut by_key: std::collections::HashMap<ItemKey, String> = std::collections::HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        by_key.insert(ItemKey::new((i + 1) as u32).unwrap(), id.clone());
    }

    for (key, info) in results {
        let Ok(info) = info else { continue };
        if !info.writable {
            continue;
        }
        let Some(_name) = info
            .data_type
            .as_ref()
            .filter(|d| d.as_str() != "Other" && d.as_str() != "Unknown")
        else {
            continue;
        };
        let Some(node) = by_key.get(&key).cloned() else {
            continue;
        };
        println!(
            "DA/UA_CANDIDATE {node} key={key} type={:?} writable=true",
            info.data_type
        );
        let original = read_value(gateway, session, key);
        if matches!(original, Value::Empty | Value::Other(_) | Value::Array(_)) {
            continue;
        }
        // Trial cycle: write a distinct value of the same type, read back, and only accept
        // the node if it holds the write; then restore the original.
        let trial = differing_value(&original);
        if trial == original {
            continue;
        }
        let wrote = wait_for(
            gateway,
            Command {
                request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                owner: opcda_egui::bff::model::Owner::Session(
                    opcda_egui::bff::model::SessionId::new(session).unwrap(),
                ),
                protocol: Protocol::OpcUa,
                kind: CommandKind::Write {
                    request: WriteRequest {
                        key,
                        value: trial.clone(),
                        type_hint: None,
                    },
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Wrote { .. }),
        );
        let accepted = match wrote.map(|e| e.kind) {
            Some(EventKind::Wrote { result, .. }) => result.is_ok(),
            _ => false,
        };
        if !accepted {
            continue;
        }
        let read_back = read_value(gateway, session, key);
        if read_back != trial {
            // Not sticky -> write back the original so we don't leave the value mutated.
            gateway
                .send(Command {
                    request_id: opcda_egui::bff::model::RequestId::new(next_id()),
                    owner: opcda_egui::bff::model::Owner::Session(
                        opcda_egui::bff::model::SessionId::new(session).unwrap(),
                    ),
                    protocol: Protocol::OpcUa,
                    kind: CommandKind::Write {
                        request: WriteRequest {
                            key,
                            value: original.clone(),
                            type_hint: None,
                        },
                    },
                })
                .expect("gateway send");
            continue;
        }
        // Restore the original before returning.
        write_value(gateway, session, key, original.clone());
        return Some((key, node, info.data_type.unwrap(), original));
    }
    None
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_write_readback_and_restore() {
    let gateway = Gateway::new();
    let session = next_id();

    let connect_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let connected = connect(&gateway, session);
        assert!(
            matches!(connected.kind, EventKind::Connected { .. }),
            "connection failed: {connected:?}"
        );

        // Resolve the node to write to: env override or a discovered writable Variable.
        let (key, node_id, data_type_name, original) =
            if let Some(override_node) = write_node_override() {
                let add = add_items(&gateway, session, &vec![override_node.clone()]);
                let info = match add.kind {
                    EventKind::AddItems { results } => results.into_iter().next().unwrap().1,
                    other => panic!("unexpected add: {other:?}"),
                };
                let info = info.expect("override node could not be added");
                assert!(
                    info.writable,
                    "override node {override_node} is not writable"
                );
                let data_type_name = info
                    .data_type
                    .clone()
                    .filter(|d| d.as_str() != "Other" && d.as_str() != "Unknown")
                    .expect("override node has no resolvable data type");
                let value = read_value(&gateway, session, ItemKey::FIRST);
                (ItemKey::FIRST, override_node, data_type_name, value)
            } else {
                let (items, tree) = collect_items(&gateway, session, "i=85");
                assert!(
                    !items.is_empty(),
                    "no Variables found in address space; browse tree:\n{}",
                    tree.iter()
                        .map(|(n, c)| format!("  {n}: {c}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                match find_writable(&gateway, session, &items) {
                    Some(found) => found,
                    None => panic!(
                        "no writable Variable found in address space with a known data type.\n\
                         Browse tree (first {count} nodes):\n{tree}",
                        count = tree.len(),
                        tree = tree
                            .iter()
                            .map(|(n, c)| format!("  {n}: {c}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ),
                }
            };

        println!("WRITABLE_NODE={node_id}");
        println!("DATA_TYPE={data_type_name}");
        println!("ORIGINAL_VALUE={original:?}");

        // Safe no-op write: the original value written back must succeed.
        write_value(&gateway, session, key, original.clone());
        assert_eq!(read_value(&gateway, session, key), original);

        // Write a different value of the same type, read back and assert equality.
        // Retry up to 3 times: the PLC cycle may revert the value concurrently.
        let different = differing_value(&original);
        let mut attempts = 0;
        let mut ok = false;
        while attempts < 3 && !ok {
            write_value(&gateway, session, key, different.clone());
            let read_back = read_value(&gateway, session, key);
            if read_back == different {
                ok = true;
            } else {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        assert!(
            ok,
            "wrote {different:?} but read back a different value after 3 attempts"
        );

        // Restore the original value (retry once in case the PLC cycle interferes).
        write_value(&gateway, session, key, original.clone());
        if read_value(&gateway, session, key) != original {
            std::thread::sleep(Duration::from_millis(300));
            write_value(&gateway, session, key, original.clone());
        }
        assert_eq!(read_value(&gateway, session, key), original);

        println!(
            "WRITE_CYCLE_OK node={node_id} type={data_type_name} orig={original:?} alt={different:?}"
        );
    }));

    disconnect(&gateway, session);
    if let Err(payload) = connect_result {
        std::panic::resume_unwind(payload);
    }
}
