//! OPC DA worker for non-Windows hosts (Linux check stand).
//!
//! OPC DA is COM-only, so there is nothing to talk to here. The worker keeps
//! the same `spawn` signature and command channel as the Windows `da.rs`, and
//! answers every request that expects a reply with `EventKind::Error` — a
//! command is never dropped silently and nothing panics.

use super::model::{BackendMsg, Command, CommandKind, Event, EventKind};
use std::sync::mpsc::Sender;

pub(crate) fn spawn(event_tx: Sender<Event>) -> super::WorkerHandle {
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<Command>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let join = std::thread::Builder::new()
        .name("opc-da-unsupported".to_string())
        .spawn(move || {
            while let Some(command) = cmd_rx.blocking_recv() {
                reply(command, &event_tx);
            }
            let _ = done_tx.send(());
        });
    match join {
        Ok(join) => super::WorkerHandle::new(cmd_tx, join, done_rx),
        // The thread could not be created: hand out a handle with a closed
        // command channel so `Gateway::send` reports the failure to the caller.
        Err(_) => super::WorkerHandle::closed(done_rx),
    }
}

fn reply(command: Command, tx: &Sender<Event>) {
    match command.kind {
        // Commands the Windows DA worker also answers with silence.
        CommandKind::Disconnect
        | CommandKind::ConfigureUaSubscription { .. }
        | CommandKind::ScanUaNetwork { .. }
        | CommandKind::CancelUaScan { .. } => {}
        kind => {
            let _ = tx.send(Event {
                origin: command.request_id.into(),
                owner: command.owner,
                kind: EventKind::Error {
                    operation: kind.operation(),
                    message: BackendMsg::DaOnlyOnWindows,
                },
            });
        }
    }
}
