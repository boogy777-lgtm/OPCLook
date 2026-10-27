use super::*;
use crate::bff::model::{BackendMsg, Event, EventKind, Operation, RequestId};
use crate::bff::model::{CommandKind, Protocol, SessionId};
use crate::constants::{DA_ROUTE_REFRESH_SECS, REQUEST_MIN_SECS, REQUEST_SLOW_MIN_SECS};
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
        let expired: Vec<(SessionId, RequestId, Operation, PendingCtx, u64)> = self
            .pending_requests
            .iter()
            .filter_map(|((session, request_id), pending)| {
                let limit = request_timeout(pending.operation, configured);
                (now.saturating_duration_since(pending.sent) > limit).then(|| {
                    (
                        *session,
                        *request_id,
                        pending.operation,
                        pending.ctx.clone(),
                        limit.as_secs(),
                    )
                })
            })
            .collect();
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
                    self.pending_requests.remove(&(session, request_id));
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
                    self.pending_requests.remove(&(session, disconnect));
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
        let auto_reconnect = options.auto_reconnect;

        if now.duration_since(self.last_da_route_refresh)
            >= Duration::from_secs(DA_ROUTE_REFRESH_SECS)
        {
            if self.da_routes.has_owned_routes() {
                self.da_routes.refresh();
            }
            self.last_da_route_refresh = now;
        }

        let mut reads: Vec<(SessionId, Protocol, Vec<ItemKey>)> = Vec::new();
        let mut tests: Vec<(SessionId, Protocol)> = Vec::new();
        let mut reconnects: Vec<SessionId> = Vec::new();

        for index in 0..self.sessions.len() {
            let id = self.sessions[index].id;
            let protocol = self.sessions[index].protocol();
            match self.sessions[index].state {
                SessionState::Connected => {
                    // With an active UA push subscription only the items no
                    // subscription takes (P1, server limits) are polled.
                    let push_active = protocol == Protocol::OpcUa
                        && self.sessions[index].subscription == SubscriptionState::Active;
                    if !self.sessions[index].read_in_flight
                        && now >= self.sessions[index].last_poll + interval
                        && !self.sessions[index].root_loading
                        && !self.sessions[index].tree.iter().any(|item| item.loading)
                    {
                        let keys: Vec<ItemKey> = self
                            .rows
                            .iter()
                            .filter(|row| row.session == id && (!push_active || row.poll_only))
                            .map(|row| row.key)
                            .collect();
                        if !keys.is_empty() {
                            self.sessions[index].read_in_flight = true;
                            self.sessions[index].last_poll = now;
                            reads.push((id, protocol, keys));
                        }
                    }
                    if self.settings.options.test_interval_secs > 0
                        && now >= self.sessions[index].last_test + test_interval
                    {
                        self.sessions[index].last_test = now;
                        tests.push((id, protocol));
                    }
                }
                SessionState::Disconnected => {
                    if auto_reconnect
                        && matches!(
                            self.sessions[index].reconnect,
                            ReconnectState::Waiting { at, .. } if now >= at
                        )
                    {
                        reconnects.push(id);
                    }
                }
                SessionState::Connecting => {}
            }
        }

        for (session, protocol, keys) in reads {
            self.send(session, protocol, CommandKind::Read { keys });
        }
        for (session, protocol) in tests {
            self.send(session, protocol, CommandKind::Test);
        }
        for session in reconnects {
            self.attempt_auto_reconnect(session);
        }
    }
}

/// How long a request may stay unanswered. The configured operation timeout
/// applies to everything; connecting and the discovery/search operations are
/// legitimately slow (retries, several endpoints), so they get at least 45 s,
/// and nothing expires sooner than 5 s whatever the setting says.
pub(super) fn request_timeout(operation: Operation, configured_secs: u64) -> Duration {
    let floor = match operation {
        Operation::Connect
        | Operation::Discover
        | Operation::SearchServers
        | Operation::SearchNetwork => REQUEST_SLOW_MIN_SECS,
        _ => REQUEST_MIN_SECS,
    };
    Duration::from_secs(configured_secs.max(floor))
}
