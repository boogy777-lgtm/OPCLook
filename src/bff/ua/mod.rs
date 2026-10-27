mod discovery;
mod io;
#[cfg(test)]
mod live_tests;
mod scanner;
mod security;
mod session;
#[cfg(test)]
mod tests;
mod trust;
mod values;
mod worker;

use std::sync::mpsc::Sender;

use crate::bff::model::Event;

pub use scanner::SCAN_SUBNET_TOO_LARGE;
pub use security::{
    SECURITY_ERROR_PREFIX, SECURITY_MODE_AUTO, SECURITY_MODE_NONE, SECURITY_MODE_SIGN,
    SECURITY_MODE_SIGN_AND_ENCRYPT, SECURITY_POLICY_AUTO, SECURITY_POLICY_BASIC256_SHA256,
    SECURITY_POLICY_NONE, resolve_security, security_error, security_error_detail,
};
pub use session::{client_cert_subject, pki_dir_public};

pub(crate) fn spawn(event_tx: Sender<Event>) -> super::WorkerHandle {
    worker::spawn(event_tx)
}
