use super::*;
use crate::bff::model::{CommandKind, Protocol, SessionId};
use crate::constants::DA_ROUTE_REFRESH_SECS;
use crate::i18n::{Tr, tr};
use std::time::{Duration, Instant};

impl App {
    /// UA-only: tell the worker the publishing interval (mirrors the app update
    /// interval) and the absolute deadband to use for the push subscription. Sent
    /// on every connect so a recreated session re-creates the subscription with
    /// the current values.
    pub(super) fn configure_ua_subscription(&mut self, session: SessionId) {
        let interval_ms = self.settings.options.update_interval_secs.max(1) * 1000;
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
    pub(super) fn tick(&mut self) {
        self.drain_settings_notices();
        let now = Instant::now();
        let options = &self.settings.options;
        let interval = Duration::from_secs(options.update_interval_secs.max(1));
        let test_interval = Duration::from_secs(options.test_interval_secs.max(1));
        let op_timeout = Duration::from_secs(options.operation_timeout_secs.max(1));
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
                    if self.sessions[index].read_in_flight
                        && now.duration_since(self.sessions[index].last_poll) > op_timeout
                    {
                        self.sessions[index].read_in_flight = false;
                        self.journal.warn(tr(self.lang(), Tr::ReadTimeout));
                    }
                    if !self.sessions[index].read_in_flight
                        && now >= self.sessions[index].last_poll + interval
                        && !self.sessions[index].root_loading
                        && !self.sessions[index].tree.iter().any(|item| item.loading)
                        && !(protocol == Protocol::OpcUa
                            && self.sessions[index].subscription == SubscriptionState::Active)
                    {
                        let keys: Vec<ItemKey> = self
                            .rows
                            .iter()
                            .filter(|row| row.session == id)
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
