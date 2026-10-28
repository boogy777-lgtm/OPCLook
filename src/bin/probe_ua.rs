mod probe_support;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, EventKind, ItemKey, ItemSpec, Protocol, WriteRequest,
};
use probe_support::Probe;
use std::time::{Duration, Instant};

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "opc.tcp://milo.digitalpetri.com:62541/milo".to_string());
    let session = 1;
    let probe = Probe::new();
    probe
        .send(Command {
            request_id: 1,
            session,
            protocol: Protocol::OpcUa,
            kind: CommandKind::Connect {
                endpoint: EndpointSpec::ua(url.clone()),
            },
        })
        .expect("probe send");

    let mut request = 2;
    let mut connected = false;
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut next_step = Instant::now();
    while Instant::now() < deadline {
        while let Some(event) = probe.try_recv() {
            println!("EVENT: {event:?}");
            match event.kind {
                EventKind::Connected { .. } => {
                    connected = true;
                    probe
                        .send(Command {
                            request_id: request,
                            session,
                            protocol: Protocol::OpcUa,
                            kind: CommandKind::Browse {
                                node_id: String::new(),
                                filter: None,
                                view_id: None,
                            },
                        })
                        .expect("probe send");
                    request += 1;
                    probe
                        .send(Command {
                            request_id: request,
                            session,
                            protocol: Protocol::OpcUa,
                            kind: CommandKind::AddItems {
                                items: vec![ItemSpec {
                                    key: 1,
                                    item_id: "i=2259".to_string(),
                                }],
                            },
                        })
                        .expect("probe send");
                    request += 1;
                    next_step = Instant::now() + Duration::from_secs(5);
                }
                EventKind::AddItems { .. } | EventKind::Browse { .. } => {
                    next_step = Instant::now() + Duration::from_secs(2);
                }
                EventKind::Error { .. } => {
                    if !connected {
                        return;
                    }
                }
                _ => {}
            }
        }
        if connected && Instant::now() >= next_step {
            probe
                .send(Command {
                    request_id: request,
                    session,
                    protocol: Protocol::OpcUa,
                    kind: CommandKind::Read {
                        keys: vec![1 as ItemKey],
                    },
                })
                .expect("probe send");
            request += 1;
            probe
                .send(Command {
                    request_id: request,
                    session,
                    protocol: Protocol::OpcUa,
                    kind: CommandKind::Write {
                        request: WriteRequest {
                            key: 1,
                            value: opcda_egui::bff::model::Value::I64(0),
                            type_hint: None,
                        },
                    },
                })
                .expect("probe send");
            request += 1;
            std::thread::sleep(Duration::from_secs(3));
            probe
                .send(Command {
                    request_id: request,
                    session,
                    protocol: Protocol::OpcUa,
                    kind: CommandKind::Disconnect,
                })
                .expect("probe send");
            std::thread::sleep(Duration::from_secs(1));
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("timeout");
}
