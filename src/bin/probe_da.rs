use opcda_egui::bff::Gateway;
use opcda_egui::bff::model::{Command, CommandKind, Protocol};
use std::time::{Duration, Instant};

fn main() {
    let gateway = Gateway::new();
    gateway
        .send(Command {
            request_id: 1,
            session: 0,
            protocol: Protocol::OpcDa,
            kind: CommandKind::Discover {
                host: String::new(),
            },
        })
        .expect("gateway send");

    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(event) = gateway.try_recv() {
            println!("{event:?}");
            match event.kind {
                opcda_egui::bff::model::EventKind::Servers { .. }
                | opcda_egui::bff::model::EventKind::Error { .. } => break,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
