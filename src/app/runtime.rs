use super::*;
use crate::bff::model::{BackendMsg, Event, EventKind, Operation};
use crate::bff::model::{CommandKind, Protocol, SessionId};
use crate::constants::DA_ROUTE_REFRESH_SECS;
use crate::i18n::{Tr, fmt};
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
    /// K1: a command whose reply never came must not leave its spinner and
    /// "in flight" flags set forever. Every outstanding request older than its
    /// limit is completed with a localized "no response" error through the
    /// ordinary error path (so every operation resets its own UI state); late
    /// replies are then dropped as unknown. Operations that the backend does not
    /// always answer (`Disconnect` of an absent session, a successful
    /// `ConfigureUaSubscription`) just expire silently.
    pub(super) fn expire_pending_requests(&mut self, now: Instant) {
        let configured = self.settings.options.operation_timeout_secs;
        let expired = self.engine.expired_requests(now, configured);
        let lang = self.lang();
        for (session, request_id, operation, ctx, secs) in expired {
            let message = BackendMsg::raw(fmt(lang, Tr::NoReplyFromServer, &[&secs.to_string()]));
            let kind = match (operation, ctx) {
                (
                    Operation::Disconnect
                    | Operation::Subscription
                    | Operation::UaRuntime
                    | Operation::Add
                    | Operation::Remove,
                    _,
                ) => {
                    self.engine.discard_pending(session, request_id);
                    continue;
                }
                (Operation::Write, PendingCtx::WriteKey(key)) => EventKind::Wrote {
                    key,
                    result: Err(message),
                },
                (operation, _) => EventKind::Error { operation, message },
            };
            self.on_event(Event {
                request_id,
                session,
                kind,
            });
            if operation == Operation::Connect {
                // The worker may still finish this Connect after we gave up.
                // Tell it to drop the session so no orphan stays connected on
                // the server; its reply belongs to an abandoned attempt and is
                // not awaited (the session is already shown as disconnected).
                if let Some(protocol) = self
                    .sessions
                    .iter()
                    .find(|view| view.id == session)
                    .map(|view| view.protocol())
                {
                    let disconnect = self.send(session, protocol, CommandKind::Disconnect);
                    self.engine.discard_pending(session, disconnect);
                }
            }
        }
    }

    pub(super) fn tick(&mut self) {
        self.drain_settings_notices();
        let now = Instant::now();
        self.expire_pending_requests(now);
        let options = &self.settings.options;
        let interval = Duration::from_secs(options.update_interval_secs.max(1));
        let test_interval = Duration::from_secs(options.test_interval_secs.max(1));
        self.engine.set_auto_reconnect(options.auto_reconnect);

        if now.duration_since(self.last_da_route_refresh)
            >= Duration::from_secs(DA_ROUTE_REFRESH_SECS)
        {
            if self.da_routes.has_owned_routes() {
                self.da_routes.refresh();
            }
            self.last_da_route_refresh = now;
        }

        let test_interval = (self.settings.options.test_interval_secs > 0).then_some(test_interval);
        for work in self.engine.due(now, interval, test_interval) {
            match work {
                opclook_engine::Due::Read {
                    session,
                    protocol,
                    keys,
                } => {
                    self.project_session(session);
                    self.send(session, protocol, CommandKind::Read { keys });
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
