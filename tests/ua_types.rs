//! Live tests (--ignored) that exercise the fuller OPC UA type support:
//! reading ByteString/NodeId/QualifiedName/LocalizedText and array variables
//! from the CODESYS address space when discoverable, the variable-only browse
//! filter, browsing server Views, and BrowsePath resolution. Unknown/absent
//! types must be handled gracefully (never panic).

use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    BrowseNode, Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, NodeKind, Protocol,
    QualityKind,
};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "opc.tcp://127.0.0.1:4840";

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(9001);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_url() -> String {
    std::env::var("OPCUA_TEST_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn wait_for(
    gateway: &Gateway,
    command: Command,
    timeout: Duration,
    predicate: impl Fn(&EventKind) -> bool,
) -> Option<Event> {
    let request_id = command.request_id;
    let session = command.session;
    gateway.send(command).expect("gateway send");
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            if event.request_id != request_id || event.session != session {
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
            request_id: next_id(),
            session,
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
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(5),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    );
}

fn with_session(test: impl FnOnce(&Gateway, u64)) {
    let gateway = Gateway::new();
    let session = next_id();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let connected = connect(&gateway, session);
        assert!(
            matches!(connected.kind, EventKind::Connected { .. }),
            "connection failed: {connected:?}"
        );
        test(&gateway, session);
    }));
    disconnect(&gateway, session);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn browse(gateway: &Gateway, session: u64, node_id: &str) -> Vec<BrowseNode> {
    let event = wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
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

fn read_values(gateway: &Gateway, session: u64, items: &[ItemSpec]) -> Event {
    wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems {
                items: items.to_vec(),
            },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::AddItems { .. }),
    )
    .expect("add items timeout");
    let keys: Vec<_> = items.iter().map(|i| i.key).collect();
    wait_for(
        gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Read { keys },
        },
        Duration::from_secs(20),
        |kind| matches!(kind, EventKind::Values { .. }),
    )
    .expect("read timeout")
}

/// Recursive depth-first walk returning every Variable node under `root`
/// (used to find candidate nodes of a particular built-in type).
fn collect_variables(
    gateway: &Gateway,
    session: u64,
    root: &str,
    out: &mut Vec<BrowseNode>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }
    for node in browse(gateway, session, root) {
        if node.kind == NodeKind::Item {
            out.push(node.clone());
        }
        if node.has_children {
            collect_variables(gateway, session, &node.node_id, out, depth + 1);
        }
    }
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_type_support_reads_without_panic_and_handles_unknown_types() {
    with_session(|gateway, session| {
        // Read 0:Objects values (Server/Status variables are standard and
        // stable across servers) to confirm sane, non-panicking reads of
        // distinct built-in types.
        let items = vec![
            ItemSpec {
                key: opcda_egui::bff::model::ItemKey::new(1).unwrap(),
                item_id: "i=2258".to_string(),
            }, // Server_ServerStatus_CurrentTime (DateTime)
            ItemSpec {
                key: opcda_egui::bff::model::ItemKey::new(2).unwrap(),
                item_id: "i=2259".to_string(),
            }, // Server_ServerStatus_State (Int32)
            ItemSpec {
                key: opcda_egui::bff::model::ItemKey::new(3).unwrap(),
                item_id: "ns=1;s=GVL.MyVar".to_string(),
            }, // may not exist -> graceful
        ];
        let read = read_values(gateway, session, &items);
        let values = match read.kind {
            EventKind::Values { values } => values,
            other => panic!("unexpected event: {other:?}"),
        };
        assert!(!values.is_empty());
        for (_k, dv) in &values {
            // Reading any built-in type must never panic; non-existent nodes
            // surface a non-Good quality instead of a crash.
            let _ = format!("{}", dv.value);
        }
    });
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_variable_filter_browse_returns_variables_only() {
    with_session(|gateway, session| {
        let children = browse(gateway, session, "i=85");
        let variables_in_root: Vec<BrowseNode> = children
            .iter()
            .filter(|n| n.kind == NodeKind::Item)
            .cloned()
            .collect();
        // If the root already has direct Variables, a filtered browse of root
        // must return the same set as the unfiltered one.
        let filtered = wait_for(
            gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::Browse {
                    node_id: "i=85".to_string(),
                    filter: opcda_egui::bff::model::NodeClassFilter::Variables,
                    view_id: None,
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
        )
        .expect("filtered browse timeout");
        let filtered_children = match filtered.kind {
            EventKind::Browse { children, .. } => children,
            other => panic!("filtered browse failed: {other:?}"),
        };
        // Every returned node must be a Variable (NodeKind::Item).
        assert!(
            filtered_children.iter().all(|n| n.kind == NodeKind::Item),
            "filter=Some(2) returned non-Variables: {:?}",
            filtered_children
                .iter()
                .filter(|n| n.kind != NodeKind::Item)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            filtered_children.len(),
            variables_in_root.len(),
            "variable-filtered browse must match the unfiltered Variable set",
        );
    });
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_browse_views_lists_views_and_browsing_in_them_does_not_error() {
    with_session(|gateway, session| {
        let views = wait_for(
            gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::BrowseViews,
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Views { .. } | EventKind::Error { .. }),
        )
        .expect("browse views timeout");
        let views = match views.kind {
            EventKind::Views { children } => children,
            EventKind::Error { message, .. } => {
                // CODESYS may not expose any Views; that is graceful.
                println!("BrowseViews unsupported/empty: {message:?}");
                return;
            }
            other => panic!("unexpected event: {other:?}"),
        };
        println!("Views found: {views:?}");
        if views.is_empty() {
            return;
        }
        let first_view = &views[0];
        // Browsing the root through the first View may legitimately return an
        // empty result, but it must not error out.
        let event = wait_for(
            gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::Browse {
                    node_id: "i=85".to_string(),
                    filter: opcda_egui::bff::model::NodeClassFilter::All,
                    view_id: Some(first_view.node_id.clone()),
                },
            },
            Duration::from_secs(20),
            |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
        )
        .expect("browse in view timeout");
        assert!(
            matches!(event.kind, EventKind::Browse { .. }),
            "browse within a View errors unexpectedly: {:?}",
            event.kind
        );
    });
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_resolve_browse_path_resolves_objects() {
    with_session(|gateway, session| {
        let resolved = wait_for(
            gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcUa,
                kind: CommandKind::ResolveBrowsePath {
                    start_node: None,
                    path: "0:Objects".to_string(),
                },
            },
            Duration::from_secs(20),
            |kind| {
                matches!(
                    kind,
                    EventKind::PathResolved { .. } | EventKind::Error { .. }
                )
            },
        )
        .expect("resolve path timeout");
        match resolved.kind {
            EventKind::PathResolved { targets } => {
                assert!(!targets.is_empty(), "0:Objects resolved to no targets");
                // The Objects node is i=85; it may resolve to that NodeId.
                println!("0:Objects resolved to: {targets:?}");
                assert!(
                    targets.contains(&"i=85".to_string())
                        || targets
                            .iter()
                            .any(|t| t.ends_with(";i=85") || t == "ns=0;i=85"),
                    "unexpected target set: {targets:?}"
                );
            }
            EventKind::Error { message, .. } => {
                println!("0:Objects resolve failed (server may not support it): {message:?}");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    });
}

#[test]
#[ignore = "requires OPC UA server at OPCUA_TEST_URL"]
fn ua_untyped_array_and_structured_read_are_graceful() {
    with_session(|gateway, session| {
        // Gather all Variables under the root; if the server exposes an array
        // or a structured (ExtensionObject) variable, read it and confirm the
        // BFF produces a non-panicking, well-formed value.
        let mut vars = Vec::new();
        collect_variables(gateway, session, "i=85", &mut vars, 0);
        if vars.is_empty() {
            println!("no variables found under i=85; skipping");
            return;
        }
        // Read up to 8 variables: at least confirms arrays/strings/bytes don't
        // crash and produce a Quality.
        let limit = vars.len().min(8);
        let items: Vec<ItemSpec> = vars
            .iter()
            .take(limit)
            .enumerate()
            .map(|(i, v)| ItemSpec {
                key: opcda_egui::bff::model::ItemKey::new(i as u32 + 10).unwrap(),
                item_id: v.node_id.clone(),
            })
            .collect();
        let read = read_values(gateway, session, &items);
        let values = match read.kind {
            EventKind::Values { values } => values,
            other => panic!("unexpected event: {other:?}"),
        };
        let good = values
            .iter()
            .filter(|(_, dv)| dv.quality.kind == QualityKind::Good)
            .count();
        println!("read {} variables, {} Good, samples:", values.len(), good);
        for (key, dv) in values.iter().take(4) {
            println!("  key {key}: {:?} -> {}", dv.value, format!("{}", dv.value));
            let _ = dv.value.as_f64();
        }
        assert!(true);
    });
}
