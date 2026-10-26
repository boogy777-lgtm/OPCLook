use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, Protocol,
};
use std::time::{Duration, Instant};

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "opc.tcp://milo.digitalpetri.com:62541/milo".to_string());
    let session = 1u64;
    let gateway = Gateway::new();
    let mut request = 1u64;
    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(url),
            },
        })
        .expect("gateway send");
    request += 1;

    if wait(&gateway, Duration::from_secs(40), |e| {
        matches!(
            e.kind,
            EventKind::Connected { .. } | EventKind::Error { .. }
        )
    })
    .map(|e| matches!(e.kind, EventKind::Connected { .. }))
        != Some(true)
    {
        println!("connect failed");
        return;
    }

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::AddItems {
                items: vec![
                    ItemSpec {
                        key: 1,
                        item_id: "i=2259".to_string(),
                    },
                    ItemSpec {
                        key: 2,
                        item_id: "i=2258".to_string(),
                    },
                ],
            },
        })
        .expect("gateway send");
    request += 1;
    let _ = wait(&gateway, Duration::from_secs(20), |e| {
        matches!(e.kind, EventKind::AddItems { .. } | EventKind::Error { .. })
    });

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Read { keys: vec![1, 2] },
        })
        .expect("gateway send");
    request += 1;
    let _ = wait(&gateway, Duration::from_secs(20), |e| {
        matches!(e.kind, EventKind::Values { .. } | EventKind::Error { .. })
    });

    gateway
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        })
        .expect("gateway send");
    let _ = wait(&gateway, Duration::from_secs(5), |_| false);
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
