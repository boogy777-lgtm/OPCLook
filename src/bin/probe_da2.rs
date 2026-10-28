mod probe_support;
use opcda_egui::bff::model::{
    Command, CommandKind, DataValue, EndpointSpec, Event, EventKind, ItemKey, ItemSpec, NodeKind,
    Protocol, ServerEntry,
};
use probe_support::Probe;
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let probe = Probe::new();
    let mut request = opcda_egui::bff::model::RequestId::FIRST;
    let session = 7u64;

    probe.send(Command {
        request_id: Some(request),
        session: 0,
        protocol: Protocol::OpcDa,
        kind: CommandKind::Discover {
            host: String::new(),
        },
    })?;
    request = request.successor().ok_or("request identifiers exhausted")?;

    let Some(servers) = wait(&probe, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Servers { .. } | EventKind::Error { .. }
        )
    }) else {
        println!("discover timeout");
        return Ok(());
    };
    let EventKind::Servers { servers, .. } = servers.kind else {
        println!("discover failed: {servers:?}");
        return Ok(());
    };
    let Some(ServerEntry { id, .. }) = servers.first() else {
        println!("no DA servers");
        return Ok(());
    };
    println!("Using server {id}");

    probe.send(Command {
        request_id: Some(request),
        session,
        protocol: Protocol::OpcDa,
        kind: CommandKind::Connect {
            endpoint: EndpointSpec::da(String::new(), id.clone()),
        },
    })?;
    request = request.successor().ok_or("request identifiers exhausted")?;

    let Some(connected) = wait(&probe, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Connected { .. } | EventKind::Error { .. }
        )
    }) else {
        println!("connect timeout");
        return Ok(());
    };
    if !matches!(connected.kind, EventKind::Connected { .. }) {
        println!("connect failed: {:?}", connected.kind);
        return Ok(());
    }

    probe.send(Command {
        request_id: Some(request),
        session,
        protocol: Protocol::OpcDa,
        kind: CommandKind::Browse {
            node_id: String::new(),
            filter: opcda_egui::bff::model::NodeClassFilter::All,
            view_id: None,
        },
    })?;
    request = request.successor().ok_or("request identifiers exhausted")?;

    let Some(root) = wait(&probe, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::Browse { .. } | EventKind::Error { .. }
        )
    }) else {
        println!("browse timeout");
        return Ok(());
    };
    let EventKind::Browse { children, .. } = root.kind else {
        println!("browse failed: {:?}", root.kind);
        return Ok(());
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
            probe.send(Command {
                request_id: Some(request),
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::Browse {
                    node_id: branch.clone(),
                    filter: opcda_egui::bff::model::NodeClassFilter::All,
                    view_id: None,
                },
            })?;
            request = request.successor().ok_or("request identifiers exhausted")?;
            if let Some(event) = wait(&probe, Duration::from_secs(20), |event| {
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
        disconnect(&probe, session, request)?;
        return Ok(());
    };

    probe.send(Command {
        request_id: Some(request),
        session,
        protocol: Protocol::OpcDa,
        kind: CommandKind::AddItems {
            items: vec![ItemSpec {
                key: opcda_egui::bff::model::ItemKey::FIRST,
                item_id: item_id.clone(),
            }],
        },
    })?;
    request = request.successor().ok_or("request identifiers exhausted")?;
    if let Some(event) = wait(&probe, Duration::from_secs(20), |event| {
        matches!(
            event.kind,
            EventKind::AddItems { .. } | EventKind::Error { .. }
        )
    }) {
        println!("AddItems: {:?}", event.kind);
    }

    probe.send(Command {
        request_id: Some(request),
        session,
        protocol: Protocol::OpcDa,
        kind: CommandKind::Read {
            keys: vec![ItemKey::FIRST],
        },
    })?;
    request = request.successor().ok_or("request identifiers exhausted")?;
    if let Some(event) = wait(&probe, Duration::from_secs(20), |event| {
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
                    ..
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

    disconnect(&probe, session, request)?;
    std::thread::sleep(Duration::from_millis(500));
    Ok(())
}

fn disconnect(
    probe: &Probe,
    session: u64,
    request: opcda_egui::bff::model::RequestId,
) -> Result<(), String> {
    probe.send(Command {
        request_id: Some(request),
        session,
        protocol: Protocol::OpcDa,
        kind: CommandKind::Disconnect,
    })?;
    let _ = wait(probe, Duration::from_secs(5), |event| {
        matches!(
            event.kind,
            EventKind::Disconnected { .. } | EventKind::Error { .. }
        )
    });
    Ok(())
}

fn wait(probe: &Probe, timeout: Duration, predicate: impl Fn(&Event) -> bool) -> Option<Event> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        while let Some(event) = probe.try_recv() {
            println!("EVENT: {:?}", event.kind);
            if predicate(&event) {
                return Some(event);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}
