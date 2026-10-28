//! Gateway lifecycle invariants: a closed command channel is reported instead
//! of silently dropping commands, and shutdown joins the workers within a bound.

mod support;

use opclook_backend::bff::{Command, CommandKind, Gateway, Protocol, SendError};
use std::time::{Duration, Instant};
use support::dfb::Dfb;

fn test_command() -> Command {
    Command {
        request_id: opcda_egui::bff::model::RequestId::new(1),
        owner: opcda_egui::bff::model::Owner::Session(opcda_egui::bff::model::SessionId::FIRST),
        protocol: Protocol::OpcUa,
        kind: CommandKind::Test,
    }
}

#[test]
fn send_after_shutdown_returns_error() {
    let mut gateway = Gateway::new();
    Dfb::shutdown(&mut gateway);

    assert_eq!(gateway.send(test_command()), Err(SendError::GatewayClosed));
    assert!(gateway.input(test_command()).is_err());
    assert!(
        gateway.output(Duration::from_millis(10)).is_none(),
        "a shut-down block must not emit protocol output"
    );
}

#[test]
fn shutdown_joins_workers_without_hanging_and_is_idempotent() {
    let mut gateway = Gateway::new();

    let started = Instant::now();
    Dfb::shutdown(&mut gateway);
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "gateway shutdown took too long: {elapsed:?}"
    );

    // A second call is a no-op, not a panic or a second long wait.
    Dfb::shutdown(&mut gateway);
}
