// OPC DA is COM-only: the real worker and the VARIANT layer exist on Windows;
// other hosts (Linux check stand) get a worker that answers every command
// with an "unsupported on this platform" error.
#[cfg(windows)]
pub mod da;
#[cfg(not(windows))]
#[path = "da_unsupported.rs"]
pub mod da;
#[cfg(windows)]
pub mod da_variant;
/// Non-Windows: only the platform-neutral part of the VARIANT layer.
#[cfg(not(windows))]
pub mod da_variant {
    pub use super::vt::vt_name;
}
pub mod da_clsid;
pub mod mdns;
pub mod model;
pub mod net;
pub mod ua;
pub mod vt;

pub use model::*;

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

/// Total budget for joining both workers during shutdown. Each worker gets the
/// remaining time, so a stuck worker cannot block process exit indefinitely.
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// One backend worker: its command channel, join handle and a completion
/// signal used to implement a bounded join on a plain `std::thread`.
pub(crate) struct WorkerHandle {
    tx: Option<UnboundedSender<Command>>,
    join: Option<JoinHandle<()>>,
    done: Receiver<()>,
}

impl WorkerHandle {
    pub(crate) fn new(
        tx: UnboundedSender<Command>,
        join: JoinHandle<()>,
        done: Receiver<()>,
    ) -> Self {
        Self {
            tx: Some(tx),
            join: Some(join),
            done,
        }
    }

    /// A handle without a worker (thread creation failed): `send` reports a
    /// closed gateway and `shutdown` has nothing to join.
    #[cfg(not(windows))]
    pub(crate) fn closed(done: Receiver<()>) -> Self {
        Self {
            tx: None,
            join: None,
            done,
        }
    }

    /// Close the command channel and wait at most `timeout` for the worker loop
    /// to finish and perform its graceful teardown. Returns true when the
    /// thread was joined; on timeout the handle is detached rather than blocking
    /// the caller forever.
    fn shutdown(&mut self, timeout: Duration) -> bool {
        drop(self.tx.take());
        let Some(join) = self.join.take() else {
            return true;
        };
        match self.done.recv_timeout(timeout) {
            Ok(()) => {
                let _ = join.join();
                true
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = join.join();
                true
            }
            Err(RecvTimeoutError::Timeout) => {
                // Dropping the JoinHandle detaches the thread; the bounded wait
                // is what `App::on_exit` is promised.
                drop(join);
                false
            }
        }
    }
}

/// Protocol-neutral backend-for-frontend.
///
/// The UI and the script host talk only to this gateway. Each session is
/// routed to exactly one protocol adapter; the adapter owns all COM / async
/// resources on its own worker thread.
pub struct Gateway {
    da: WorkerHandle,
    ua: WorkerHandle,
    events: Receiver<Event>,
}

impl Gateway {
    pub fn new() -> Self {
        let (event_tx, events) = std::sync::mpsc::channel::<Event>();
        let da = da::spawn(event_tx.clone());
        let ua = ua::spawn(event_tx);
        Self { da, ua, events }
    }

    /// Route a command to its protocol worker. A closed channel (worker died or
    /// gateway already shut down) is surfaced so the caller can journal it.
    pub fn send(&self, command: Command) -> Result<(), String> {
        let worker = match command.protocol {
            Protocol::OpcDa => &self.da,
            Protocol::OpcUa => &self.ua,
        };
        let tx = worker
            .tx
            .as_ref()
            .ok_or_else(|| "gateway is shut down".to_string())?;
        tx.send(command)
            .map_err(|_| "worker command channel is closed".to_string())
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    /// Close both command channels and join the workers with a bounded wait.
    /// Workers run their own graceful teardown (UA CloseSession/CloseSecureChannel,
    /// DA RemoveGroup) after the command loop ends. Idempotent.
    pub fn shutdown(&mut self) {
        let deadline = Instant::now() + WORKER_SHUTDOWN_TIMEOUT;
        let remaining = || deadline.saturating_duration_since(Instant::now());
        self.da.shutdown(remaining());
        self.ua.shutdown(remaining());
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.shutdown();
    }
}
