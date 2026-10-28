// OPC DA is COM-only: the real worker and the VARIANT layer exist on Windows;
// other hosts (Linux check stand) get a worker that answers every command
// with an "unsupported on this platform" error.
#[cfg(windows)]
mod com;
#[cfg(windows)]
pub mod da;
#[cfg(not(windows))]
#[path = "da_unsupported.rs"]
pub mod da;
#[cfg(windows)]
pub mod da_variant;
#[cfg(any(windows, test))]
mod filetime;
/// Non-Windows: only the platform-neutral part of the VARIANT layer.
#[cfg(not(windows))]
pub mod da_variant {
    pub use super::vt::vt_name;
}
pub mod da_clsid;
pub mod da_router;
pub mod da_status;
pub mod mdns;
pub use opclook_contract::model;
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
    join: Option<JoinHandle<ShutdownOutcome>>,
    done: Receiver<()>,
}

impl WorkerHandle {
    pub(crate) fn new(
        tx: UnboundedSender<Command>,
        join: JoinHandle<ShutdownOutcome>,
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
    pub(crate) fn closed(done: Receiver<()>) -> Self {
        Self {
            tx: None,
            join: None,
            done,
        }
    }

    /// Wait for actual thread termination, not merely the completion signal:
    /// thread-local destructors and COM cleanup can run after that signal.
    fn shutdown(&mut self, timeout: Duration) -> ShutdownOutcome {
        drop(self.tx.take());
        let Some(join) = self.join.take() else {
            return ShutdownOutcome::Joined;
        };
        let deadline = Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(Instant::now);
        let mut signal_open = true;
        loop {
            if join.is_finished() {
                return join.join().unwrap_or(ShutdownOutcome::Panicked);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return ShutdownOutcome::TimedOut;
            }
            let wait = remaining.min(Duration::from_millis(5));
            if signal_open {
                match self.done.recv_timeout(wait) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => signal_open = false,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            } else {
                std::thread::sleep(wait);
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
    pub fn send(&self, command: Command) -> Result<(), SendError> {
        let worker = match command.protocol {
            Protocol::OpcDa => &self.da,
            Protocol::OpcUa => &self.ua,
        };
        let tx = worker.tx.as_ref().ok_or(SendError::GatewayClosed)?;
        tx.send(command).map_err(|_| SendError::WorkerClosed)
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    /// Close both command channels and join the workers with a bounded wait.
    /// Workers run their own graceful teardown (UA CloseSession/CloseSecureChannel,
    /// DA RemoveGroup) after the command loop ends. Idempotent.
    pub fn shutdown(&mut self) -> GatewayShutdown {
        self.shutdown_with_timeout(WORKER_SHUTDOWN_TIMEOUT)
    }

    fn shutdown_with_timeout(&mut self, timeout: Duration) -> GatewayShutdown {
        drop(self.da.tx.take());
        drop(self.ua.tx.take());
        let deadline = Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(Instant::now);
        let remaining = || deadline.saturating_duration_since(Instant::now());
        GatewayShutdown {
            da: self.da.shutdown(remaining()),
            ua: self.ua.shutdown(remaining()),
        }
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use std::sync::mpsc;

    fn worker(f: impl FnOnce(mpsc::Sender<()>) + Send + 'static) -> WorkerHandle {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::channel();
        WorkerHandle::new(
            tx,
            std::thread::spawn(move || {
                f(done_tx);
                ShutdownOutcome::Joined
            }),
            done_rx,
        )
    }

    #[test]
    fn closed_worker_has_typed_send_error() {
        let (event_tx, events) = mpsc::channel();
        drop(event_tx);
        let (_, done1) = mpsc::channel();
        let (_, done2) = mpsc::channel();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        drop(rx);
        let mut da = WorkerHandle::closed(done1);
        da.tx = Some(tx);
        let gateway = Gateway {
            da,
            ua: WorkerHandle::closed(done2),
            events,
        };
        let command = Command {
            protocol: Protocol::OpcDa,
            owner: Owner::Discovery,
            request_id: None,
            kind: CommandKind::Disconnect,
        };
        assert_eq!(gateway.send(command), Err(SendError::WorkerClosed));
    }

    #[test]
    fn early_signal_does_not_make_join_unbounded() {
        for signal in [true, false] {
            let (release_tx, release_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let mut handle = worker(move |done| {
                if signal {
                    done.send(()).unwrap();
                }
                drop(done);
                ready_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
            ready_rx.recv().unwrap();
            let (result_tx, result_rx) = mpsc::channel();
            let waiter = std::thread::spawn(move || {
                result_tx
                    .send(handle.shutdown(Duration::from_millis(50)))
                    .unwrap();
            });
            let result = result_rx.recv_timeout(Duration::from_secs(2));
            release_tx.send(()).unwrap();
            waiter.join().unwrap();
            assert_eq!(result.unwrap(), ShutdownOutcome::TimedOut);
        }
    }

    #[test]
    fn both_channels_close_before_waiting_for_hung_worker() {
        let (da_tx, _da_rx) = tokio::sync::mpsc::unbounded_channel();
        let (da_done_tx, da_done_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let da = std::thread::spawn(move || {
            release_rx.recv().unwrap();
            drop(da_done_tx);
            ShutdownOutcome::Joined
        });
        let (ua_tx, mut ua_rx) = tokio::sync::mpsc::unbounded_channel::<Command>();
        let (ua_done_tx, ua_done_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let ua = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            assert!(ua_rx.blocking_recv().is_none());
            drop(ua_done_tx);
            ShutdownOutcome::Joined
        });
        ready_rx.recv().unwrap();
        let (_, events) = mpsc::channel();
        let mut gateway = Gateway {
            da: WorkerHandle::new(da_tx, da, da_done_rx),
            ua: WorkerHandle::new(ua_tx, ua, ua_done_rx),
            events,
        };
        let result = gateway.shutdown_with_timeout(Duration::from_millis(500));
        release_tx.send(()).unwrap();
        assert_eq!(
            result,
            GatewayShutdown {
                da: ShutdownOutcome::TimedOut,
                ua: ShutdownOutcome::Joined
            }
        );
    }

    #[test]
    fn reports_join_and_panic() {
        let mut joined = worker(|_| {});
        assert_eq!(
            joined.shutdown(Duration::from_secs(1)),
            ShutdownOutcome::Joined
        );
        let mut panicked = worker(|_| panic!("worker failure"));
        assert_eq!(
            panicked.shutdown(Duration::from_secs(1)),
            ShutdownOutcome::Panicked
        );
        assert_eq!(panicked.shutdown(Duration::ZERO), ShutdownOutcome::Joined);
    }
}
