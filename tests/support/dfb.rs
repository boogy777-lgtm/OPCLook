//! DFB-style functional block for protocol integration tests.
//!
//! One block, one input, one output:
//!
//! ```text
//! Command --[input]--> Gateway --[output]--> Event
//! ```
//!
//! Tests talk to this trait instead of reaching into `Gateway` internals or
//! re-implementing connect/wait helpers. The trait is intentionally test-only;
//! production code keeps the concrete `Gateway` facade.

use opclook_backend::bff::{Command, Event, Gateway};
use std::time::{Duration, Instant};

/// A single functional block with explicit input and output handles.
///
/// `input` is the only way to submit work; `output` is the only way to observe
/// results. `shutdown` ends the block's lifecycle. Implementations must not
/// expose mutable internal state to tests.
pub trait Dfb {
    type Input;
    type Output;

    /// Submit one input value. Returns the transport/dispatch error, if any.
    fn input(&mut self, input: Self::Input) -> Result<(), String>;

    /// Wait up to `timeout` for the next output value.
    fn output(&mut self, timeout: Duration) -> Option<Self::Output>;

    /// Stop the block and release its workers.
    fn shutdown(&mut self);
}

impl Dfb for Gateway {
    type Input = Command;
    type Output = Event;

    fn input(&mut self, input: Self::Input) -> Result<(), String> {
        self.send(input).map_err(|error| error.to_string())
    }

    fn output(&mut self, timeout: Duration) -> Option<Self::Output> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self.try_recv() {
                return Some(event);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn shutdown(&mut self) {
        Gateway::shutdown(self);
    }
}
