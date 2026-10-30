pub use opclook_contract::model;
#[path = "../../../../../crates/backend/src/bff/softing_da.rs"]
pub mod da;
#[path = "../../../../../crates/backend/src/bff/da_router.rs"]
pub mod da_router;
#[path = "../../../../../crates/backend/src/bff/filetime.rs"]
pub mod filetime;
#[path = "../../../../../crates/backend/src/bff/softing_runtime.rs"]
pub mod softing_runtime;
#[path = "../../../../../crates/backend/src/bff/vt.rs"]
pub mod vt;
use crate::sync::mpsc::UnboundedSender;
use std::{sync::mpsc::Receiver, thread::JoinHandle};
#[derive(Debug, PartialEq)]
pub enum ShutdownOutcome {
    Joined,
    TimedOut,
    Panicked,
}
pub struct WorkerHandle {
    pub tx: Option<UnboundedSender<model::Command>>,
    pub join: Option<JoinHandle<ShutdownOutcome>>,
    pub done: Receiver<()>,
}
impl WorkerHandle {
    pub fn new(
        tx: UnboundedSender<model::Command>,
        join: JoinHandle<ShutdownOutcome>,
        done: Receiver<()>,
    ) -> Self {
        Self {
            tx: Some(tx),
            join: Some(join),
            done,
        }
    }
    pub fn closed(done: Receiver<()>) -> Self {
        Self {
            tx: None,
            join: None,
            done,
        }
    }
}

#[path = "../../../../../crates/backend/src/bff/softing_scalar.rs"]
pub mod softing_scalar;

#[path = "../../../../../crates/backend/src/bff/da_clsid.rs"]
pub mod da_clsid;
