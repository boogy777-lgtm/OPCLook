mod probe_support;
use opclook::bff::model::{
    Command, CommandKind, EndpointSpec, EventKind, ItemKey, ItemSpec, Protocol, WriteRequest,
};
use probe_support::Probe;
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "opc.tcp://milo.digitalpetri.com:62541/milo".to_string());
    let session = opclook::bff::model::SessionId::FIRST;
    let probe = Probe::new();
    probe.send(Command {
        request_id: Some(opclook::bff::model::RequestId::FIRST),
        owner: session.into(),
        protocol: Protocol::OpcUa,
        kind: CommandKind::Connect {
            endpoint: EndpointSpec::ua(url.clone()),
        },
    })?;

    let mut request = opclook::bff::model::RequestId::FIRST
        .successor()
        .ok_or("request identifiers exhausted")?;
    let mut connected = false;
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut next_step = Instant::now();
    while Instant::now() < deadline {
        while let Some(event) = probe.try_recv() {
            println!("EVENT: {event:?}");
            match event.kind {
                EventKind::Connected { .. } => {
                    connected = true;
                    probe.send(Command {
                        request_id: Some(request),
                        owner: session.into(),
                        protocol: Protocol::OpcUa,
                        kind: CommandKind::Browse {
                            node_id: String::new(),
                            filter: opclook::bff::model::NodeClassFilter::All,
                            view_id: None,
                        },
                    })?;
                    request = request.successor().ok_or("request identifiers exhausted")?;
                    probe.send(Command {
                        request_id: Some(request),
                        owner: session.into(),
                        protocol: Protocol::OpcUa,
                        kind: CommandKind::AddItems {
                            items: vec![ItemSpec {
                                key: opclook::bff::model::ItemKey::FIRST,
                                item_id: "i=2259".to_string(),
                            }],
                        },
                    })?;
                    request = request.successor().ok_or("request identifiers exhausted")?;
                    next_step = Instant::now() + Duration::from_secs(5);
                }
                EventKind::AddItems { .. } | EventKind::Browse { .. } => {
                    next_step = Instant::now() + Duration::from_secs(2);
                }
                EventKind::Error { .. } => {
                    if !connected {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        if connected && Instant::now() >= next_step {
            probe.send(Command {
                request_id: Some(request),
                owner: session.into(),
                protocol: Protocol::OpcUa,
                kind: CommandKind::Read {
                    keys: vec![ItemKey::FIRST],
                },
            })?;
            request = request.successor().ok_or("request identifiers exhausted")?;
            probe.send(Command {
                request_id: Some(request),
                owner: session.into(),
                protocol: Protocol::OpcUa,
                kind: CommandKind::Write {
                    request: WriteRequest {
                        key: opclook::bff::model::ItemKey::FIRST,
                        value: opclook::bff::model::Value::I64(0),
                        type_hint: None,
                    },
                },
            })?;
            request = request.successor().ok_or("request identifiers exhausted")?;
            std::thread::sleep(Duration::from_secs(3));
            probe.send(Command {
                request_id: Some(request),
                owner: session.into(),
                protocol: Protocol::OpcUa,
                kind: CommandKind::Disconnect,
            })?;
            std::thread::sleep(Duration::from_secs(1));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("timeout");
    Ok(())
}
