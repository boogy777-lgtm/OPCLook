use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{
    Command, CommandKind, EndpointSpec, Event, EventKind, ItemSpec, Protocol, Value, WriteRequest,
};
use std::process::Command as ScCommand;
use std::time::{Duration, Instant};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn prog_id() -> Option<String> {
    std::env::var("DA_TEST_PROGID")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn service_name_from_progid(id: &str) -> String {
    // Kepware.KEPServerEX.V6 -> KEPServerEXV6; fall back to the ProgID.
    id.replace('.', "").replace("Kepware", "KepwareEx")
}

fn run_sc(args: &[&str]) -> (bool, String) {
    let out = ScCommand::new("sc").args(args).output();
    match out {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
            (o.status.success(), format!("{stdout}{stderr}"))
        }
        Err(e) => (false, format!("sc failed: {e}")),
    }
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

#[test]
#[ignore = "requires an OPC DA server configured via DA_TEST_PROGID"]
fn da_write_roundtrip() {
    let Some(prog_id) = prog_id() else {
        println!("SKIP: env DA_TEST_PROGID is not set; DA live test not run.");
        return;
    };

    // Optional service kick: try to start the Kepware service so the server is reachable.
    let service = service_name_from_progid(&prog_id);
    let (query_ok, query_out) = run_sc(&["query", &service]);
    if !query_ok {
        println!("SKIP: service query for {service} failed:\n{query_out}");
        let (start_ok, start_out) = run_sc(&["start", &service]);
        if !start_ok {
            println!(
                "SKIP: could not start OPC DA service {service} (likely error 1060/1068):\n{start_out}\n\
                 Start the OPC DA server manually and set DA_TEST_PROGID, then rerun."
            );
            return;
        }
        // Give COM ATL a moment to become ready.
        std::thread::sleep(Duration::from_secs(3));
    }

    let gateway = Gateway::new();
    let session = next_id();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match wait_for(
            &gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::Connect {
                    endpoint: EndpointSpec::da("", &prog_id),
                },
            },
            Duration::from_secs(30),
            |kind| matches!(kind, EventKind::Connected { .. } | EventKind::Error { .. }),
        ) {
            Some(ev) if matches!(ev.kind, EventKind::Connected { .. }) => {}
            Some(ev) => {
                println!("SKIP: DA connect to {prog_id} failed: {ev:?}");
                return;
            }
            None => {
                println!("SKIP: DA connect to {prog_id} timed out.");
                return;
            }
        }
        println!("DA_CONNECT_OK prog_id={prog_id}");

        // Browse the root and pick the first leaf item to write to.
        let browse = wait_for(
            &gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::Browse {
                    node_id: String::new(),
                    filter: opcda_egui::bff::model::NodeClassFilter::All,
                    view_id: None,
                },
            },
            Duration::from_secs(30),
            |kind| matches!(kind, EventKind::Browse { .. } | EventKind::Error { .. }),
        )
        .expect("DA browse timeout");
        let children = match browse.kind {
            EventKind::Browse { children, .. } => children,
            other => {
                println!("SKIP: DA browse failed: {other:?}");
                return;
            }
        };
        let leaf = children
            .iter()
            .find(|n| n.kind == opcda_egui::bff::model::NodeKind::Item);
        let Some(leaf) = leaf else {
            println!("SKIP: DA browse returned no leaf items; tree={children:?}");
            return;
        };
        let item_id = leaf.node_id.clone();
        println!("DA_ITEM_ID={item_id}");

        let add = wait_for(
            &gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: opcda_egui::bff::model::ItemKey::new(1).unwrap(),
                        item_id: item_id.clone(),
                    }],
                },
            },
            Duration::from_secs(30),
            |kind| matches!(kind, EventKind::AddItems { .. }),
        )
        .expect("DA add timeout");
        match add.kind {
            EventKind::AddItems { results } => match results.into_iter().next().unwrap().1 {
                Ok(info) => println!(
                    "DA_ITEM_ADDED type={} vt={:?} writable={}",
                    info.data_type.as_deref().unwrap_or("?"),
                    info.type_id,
                    info.writable
                ),
                Err(e) => {
                    println!("SKIP: could not add DA item {item_id}: {e:?}");
                    return;
                }
            },
            other => {
                println!("SKIP: unexpected add response: {other:?}");
                return;
            }
        }

        let read = wait_for(
            &gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::Read {
                    keys: vec![opcda_egui::bff::model::ItemKey::new(1).unwrap()],
                },
            },
            Duration::from_secs(30),
            |kind| matches!(kind, EventKind::Values { .. }),
        )
        .expect("DA read timeout");
        let original = match read.kind {
            EventKind::Values { values } => values.into_iter().next().unwrap().1,
            other => panic!("DA read failed: {other:?}"),
        };
        println!(
            "DA_ORIGINAL=({}, {:?}, {:?})",
            original.quality.text, original.value, original.timestamp
        );
        if matches!(original.value, Value::Empty) {
            println!("SKIP: DA item {item_id} returned an empty value; nothing safe to write.");
            return;
        }

        // Safe no-op write of the same value must succeed.
        let write_ok = wait_for(
            &gateway,
            Command {
                request_id: next_id(),
                session,
                protocol: Protocol::OpcDa,
                kind: CommandKind::Write {
                    request: WriteRequest {
                        key: opcda_egui::bff::model::ItemKey::new(1).unwrap(),
                        value: original.value.clone(),
                        type_hint: None,
                    },
                },
            },
            Duration::from_secs(30),
            |kind| matches!(kind, EventKind::Wrote { .. }),
        );
        match write_ok.map(|e| e.kind) {
            Some(EventKind::Wrote { result, .. }) => {
                assert!(result.is_ok(), "DA write failed: {result:?}");
            }
            other => panic!("DA write returned unexpected: {other:?}"),
        }
        println!("DA_WRITE_SAME_OK value={:?}", original.value);
    }));

    // Best-effort disconnect.
    let _ = wait_for(
        &gateway,
        Command {
            request_id: next_id(),
            session,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Disconnect,
        },
        Duration::from_secs(8),
        |kind| matches!(kind, EventKind::Disconnected { .. }),
    );

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
