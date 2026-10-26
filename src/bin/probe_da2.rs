use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    Command, CommandKind, DataValue, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NodeKind,
    Protocol, ServerEntry,
};
use std::time::{Duration, Instant};

fn main() {
    let gateway = Gateway::new();
    let mut request = 1u64;
    let session = 7u64;

    gateway
        .send(Command {
            request_id: request,
            session: 0,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Discover {
                host: String::new(),
            },
        })
        .expect("gateway send");
    request += 1;

    let Some(servers) = wait(&gateway, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Servers { .. } | EventKind::Error { .. }
        )
    }) else {
        println!("discover timeout");
        return;
    };
    let EventKind::Servers { servers, .. } = servers.kind else {
        println!("discover failed: {servers:?}");
        return;
    };
    let Some(ServerEntry { id, .. }) = servers.first() else {
        println!("no DA servers");
        return;
    };
    println!("Using server {id}");

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::da(String::new(), id.clone()),
            },
        })
        .expect("gateway send");
    request += 1;

    let Some(connected) = wait(&gateway, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Connected { .. } | EventKind::Error { .. }
        )
    }) else {
        println!("connect timeout");
        return;
    };
    if !matches!(connected.kind, EventKind::Connected { .. }) {
        println!("connect failed: {:?}", connected.kind);
        return;
    }

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Browse {
                node_id: String::new(),
                filter: None,
                view_id: None,
            },
        })
        .expect("gateway send");
    request += 1;

    let Some(root) = wait(&gateway, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Browse { .. } | EventKind::Error { .. }
        )
    }) else {
        println!("browse timeout");
        return;
    };
    let EventKind::Browse { children, .. } = root.kind else {
        println!("browse failed: {:?}", root.kind);
        return;
    };
    println!("Root nodes: {}", children.len());
    for node in children.iter().take(20) {
        println!("  {:?} {} ({})", node.kind, node.name, node.node_id);
    }

    let branch = children
        .iter()
        .find(|node| node.kind == NodeKind::Branch)
        .map(|node| node.node_id.clone());
    let mut item = children
        .iter()
        .find(|node| node.kind == NodeKind::Item)
        .map(|node| (node.node_id.clone(), node.name.clone()));

    if item.is_none() {
        if let Some(branch) = branch {
            gateway
                .send(Command {
                    request_id: request,
                    session,
                    protocol: Protocol::OpcDa,
                    kind: CommandKind::Browse {
                        node_id: branch.clone(),
                        filter: None,
                        view_id: None,
                    },
                })
                .expect("gateway send");
            request += 1;
            if let Some(event) = wait(&gateway, Duration::from_secs(20), |event| {
                matches!(
                    event.kind,
                    EventKind::Browse { .. } | EventKind::Error { .. }
                )
            }) {
                if let EventKind::Browse { children, .. } = event.kind {
                    println!("Branch \"{branch}\": {}", children.len());
                    for node in children.iter().take(20) {
                        println!("  {:?} {} ({})", node.kind, node.name, node.node_id);
                    }
                    item = children
                        .iter()
                        .find(|node| node.kind == NodeKind::Item)
                        .map(|node| (node.node_id.clone(), node.name.clone()));
                }
            }
        }
    }

    let Some((item_id, _name)) = item else {
        println!("no leaf found");
        disconnect(&gateway, session, request);
        return;
    };

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcDa,
            kind: CommandKind::AddItems {
                items: vec![ItemSpec {
                    key: 1,
                    item_id: item_id.clone(),
                }],
            },
        })
        .expect("gateway send");
    request += 1;
    if let Some(event) = wait(&gateway, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::AddItems { .. } | EventKind::Error { .. }
        )
    }) {
        println!("AddItems: {:?}", event.kind);
    }

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Read {
                keys: vec![1 as ItemKey],
            },
        })
        .expect("gateway send");
    request += 1;
    if let Some(event) = wait(&gateway, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Values { .. } | EventKind::Error { .. }
        )
    }) {
        if let EventKind::Values { values } = event.kind {
            for (
                key,
                DataValue {
                    value,
                    quality,
                    timestamp,
                },
            ) in values
            {
                println!(
                    "Read key {key}: {value} quality={} ({:?}) time={timestamp:?}",
                    quality.text, quality.kind
                );
            }
        } else {
            println!("read failed: {:?}", event.kind);
        }
    }

    disconnect(&gateway, session, request);
    std::thread::sleep(Duration::from_millis(500));
}

fn disconnect(gateway: &Gateway, session: u64, request: u64) {
    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Disconnect,
        })
        .expect("gateway send");
    let _ = wait(gateway, Duration::from_secs(5), |event| {
        matches!(
            event.kind,
            EventKind::Disconnected { .. } | EventKind::Error { .. }
        )
    });
}

fn wait(gateway: &Gateway, timeout: Duration, predicate: impl Fn(&Event) -> bool) -> Option<Event> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = gateway.try_recv() {
            println!("EVENT: {:?}", event.kind);
            if predicate(&event) {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}
