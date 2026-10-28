mod probe_support;
use opcda_egui::bff::model::{Command, CommandKind, Protocol};
use probe_support::Probe;
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let probe = Probe::new();
    probe.send(Command {
        request_id: Some(opcda_egui::bff::model::RequestId::FIRST),
        owner: opcda_egui::bff::model::Owner::Discovery,
        protocol: Protocol::OpcDa,
        kind: CommandKind::Discover {
            host: String::new(),
        },
    })?;

    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(event) = probe.try_recv() {
            println!("{event:?}");
            match event.kind {
                opcda_egui::bff::model::EventKind::Servers { .. }
                | opcda_egui::bff::model::EventKind::Error { .. } => break,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
