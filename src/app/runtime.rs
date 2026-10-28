use super::*;
#[cfg(test)]
use crate::bff::model::Operation;
use crate::bff::model::{CommandKind, Protocol, SessionId};
use std::time::{Duration, Instant};

impl App {
    /// UA-only: tell the worker the publishing interval (mirrors the app update
    /// interval) and the absolute deadband to use for the push subscription. Sent
    /// on every connect so a recreated session re-creates the subscription with
    /// the current values.
    pub(super) fn configure_ua_subscription(&mut self, session: SessionId) {
        let interval_ms = self
            .settings
            .options
            .update_interval_secs
            .max(1)
            .saturating_mul(1000);
        let deadband = self.settings.options.ua_deadband;
        self.send(
            session,
            Protocol::OpcUa,
            CommandKind::ConfigureUaSubscription {
                publishing_interval_ms: interval_ms,
                deadband,
            },
        );
    }
    /// Ask Engine to expire requests; ordinary correlated events update the UI.
    pub(super) fn expire_pending_requests(&mut self, now: Instant) {
        self.engine
            .expire_requests(now, self.settings.options.operation_timeout_secs);
        self.poll_events();
    }

    pub(super) fn tick(&mut self) {
        self.drain_settings_notices();
        let now = Instant::now();
        self.expire_pending_requests(now);
        let options = &self.settings.options;
        let interval = Duration::from_secs(options.update_interval_secs.max(1));
        let test_interval = Duration::from_secs(options.test_interval_secs.max(1));
        self.engine.set_auto_reconnect(options.auto_reconnect);

        let test_interval = (self.settings.options.test_interval_secs > 0).then_some(test_interval);
        for work in self.engine.due(now, interval, test_interval) {
            match work {
                opclook_engine::Due::Read {
                    session,
                    protocol,
                    keys,
                } => {
                    self.project_session(session);
                    if self
                        .send(session, protocol, CommandKind::Read { keys })
                        .is_none()
                    {
                        self.project_session(session);
                    }
                }
                opclook_engine::Due::Test { session, protocol } => {
                    self.project_session(session);
                    self.send(session, protocol, CommandKind::Test);
                }
                opclook_engine::Due::Reconnect { session } => {
                    self.project_session(session);
                    self.attempt_auto_reconnect(session);
                }
            }
        }
    }
}

/// How long a request may stay unanswered. The configured operation timeout
/// applies to everything; connecting and the discovery/search operations are
/// legitimately slow (retries, several endpoints), so they get at least 45 s,
/// and nothing expires sooner than 5 s whatever the setting says.
#[cfg(test)]
pub(super) fn request_timeout(operation: Operation, configured_secs: u64) -> Duration {
    opclook_engine::request_timeout(operation, configured_secs)
}
