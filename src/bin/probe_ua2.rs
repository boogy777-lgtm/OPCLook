mod probe_support;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, Protocol,
};
use probe_support::Probe;
use std::time::{Duration, Instant};

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "opc.tcp://milo.digitalpetri.com:62541/milo".to_string());
    let session = 1u64;
    let probe = Probe::new();
    let mut request = 1u64;
    probe
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(url),
            },
        })
        .expect("probe send");
    request += 1;

    if wait(&probe, Duration::from_secs(40), |e| {
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

    probe
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
        .expect("probe send");
    request += 1;
    let _ = wait(&probe, Duration::from_secs(20), |e| {
        matches!(e.kind, EventKind::AddItems { .. } | EventKind::Error { .. })
    });

    probe
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Read { keys: vec![1, 2] },
        })
        .expect("probe send");
    request += 1;
    let _ = wait(&probe, Duration::from_secs(20), |e| {
        matches!(e.kind, EventKind::Values { .. } | EventKind::Error { .. })
    });

    probe
        .send(Command {
            request_id: request,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Disconnect,
        })
        .expect("probe send");
    let _ = wait(&probe, Duration::from_secs(5), |_| false);
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
