use super::*;
use crate::bff::model::{CommandKind, EndpointSpec, Protocol, Quality, QualityKind, SessionId};
use crate::constants::{RECONNECT_BASE_DELAY_SECS, RECONNECT_MAX_ATTEMPTS};
use crate::i18n::{Tr, fmt, tr};
use std::time::{Duration, Instant};

impl App {
    /// Unique identity used for endpoint deduplication. Two sessions are the
    /// same container iff they share the protocol and the address (ProgID for
    /// DA, endpoint URL for UA); the DA `host` is just a computer-name filter,
    /// not part of the server's identity.
    fn dedup_key(endpoint: &EndpointSpec) -> (Protocol, String) {
        match endpoint.protocol {
            Protocol::OpcDa => (
                endpoint.protocol,
                format!(
                    "{}|{}",
                    endpoint.host.trim().to_ascii_lowercase(),
                    endpoint.address.trim().to_ascii_lowercase()
                ),
            ),
            Protocol::OpcUa => (endpoint.protocol, endpoint.address.trim().to_string()),
        }
    }

    /// Open (create or restore) the tab for an endpoint.
    ///
    /// Endpoint-dedup rule: if a container for the same endpoint already exists
    /// (connected, connecting or disconnected), do NOT create a duplicate
    /// session — activate the existing view. If UA credentials or security
    /// settings changed, reconnect the existing session with the new contract.
    /// Return false when the requested operation was rejected so the caller
    /// can keep its dialog open and show the error.
    pub fn open_tab_for_endpoint(&mut self, endpoint: EndpointSpec) -> bool {
        let key = Self::dedup_key(&endpoint);
        if let Some(id) = self
            .sessions
            .iter()
            .find(|s| Self::dedup_key(&s.endpoint) == key)
            .map(|s| s.id)
        {
            let mut reconfigure = false;
            if let Some(view) = self.sessions.iter_mut().find(|s| s.id == id) {
                view.visible = true;
                if endpoint.protocol == Protocol::OpcUa {
                    reconfigure = view.endpoint.user != endpoint.user
                        || view.endpoint.password != endpoint.password
                        || view.endpoint.ua_use_certificate != endpoint.ua_use_certificate
                        || view.endpoint.ua_security_policy != endpoint.ua_security_policy
                        || view.endpoint.ua_security_mode != endpoint.ua_security_mode
                        || view.endpoint.ua_trust_server_cert != endpoint.ua_trust_server_cert
                        || view.endpoint.ua_session_timeout != endpoint.ua_session_timeout;
                    if reconfigure {
                        let mut updated = endpoint;
                        // Existing sessions keep their selected NIC until
                        // explicitly removed; reauthentication does not rebind.
                        updated.ua_bind = view.endpoint.ua_bind.clone();
                        view.endpoint = updated;
                    }
                }
            }
            self.active_session = Some(id);
            self.selected_tree = None;
            if reconfigure {
                self.reconnect_session(id);
            }
            return true;
        }
        // Hard UA session cap. Reopening/reconnecting an existing container is
        // handled above and is never rejected.
        if endpoint.protocol == Protocol::OpcUa
            && self.ua_session_count() >= crate::constants::MAX_UA_SESSIONS
        {
            let lang = self.lang();
            let message = fmt(
                lang,
                Tr::UaSessionLimitReached,
                &[&crate::constants::MAX_UA_SESSIONS.to_string()],
            );
            self.status = message.clone();
            self.journal.error_l(
                Tr::UaSessionLimitReached,
                &[&crate::constants::MAX_UA_SESSIONS.to_string()],
            );
            if self.dialogs.connect.open {
                self.dialogs.connect.error = Some(message);
            }
            return false;
        }

        let mut endpoint = endpoint;
        let mut da_prepared = None;
        if endpoint.protocol == Protocol::OpcDa {
            match self.prepare_da_route_for_host(&endpoint.host) {
                Ok(prepared) => da_prepared = Some(prepared),
                Err(error) => {
                    let message = error.localized(self.lang());
                    self.status = message.clone();
                    self.journal.error(message.clone());
                    if self.dialogs.connect.open {
                        self.dialogs.connect.error = Some(message);
                    }
                    return false;
                }
            }
        } else if endpoint.ua_bind.is_none() {
            match self.ua_binding_for_endpoint(&endpoint.address) {
                Ok(binding) => endpoint.ua_bind = Some(binding),
                Err(message) => {
                    self.status = message.clone();
                    self.journal.error(message.clone());
                    if self.dialogs.connect.open {
                        self.dialogs.connect.error = Some(message);
                    }
                    return false;
                }
            }
        }
        let id = self.next_session;
        self.next_session += 1;
        self.settings.remember(&endpoint);
        self.settings.save();
        self.journal.info_l(
            Tr::ConnectingTo,
            &[&endpoint.display_name(), &endpoint.protocol.to_string()],
        );
        self.sessions
            .push(SessionView::new(id, endpoint.clone(), self.lang()));
        if let Some(prepared) = da_prepared {
            let host = endpoint.host.clone();
            self.hold_da_route(id, &host, prepared);
        }
        self.active_session = Some(id);
        self.selected_tree = None;
        self.send(id, endpoint.protocol, CommandKind::Connect { endpoint });
        true
    }

    /// Binding for a UA endpoint: loopback always uses loopback, remote
    /// endpoints require an explicitly selected adapter.
    fn ua_binding_for_endpoint(
        &mut self,
        url: &str,
    ) -> Result<crate::bff::net::NetworkBinding, String> {
        let lang = self.lang();
        let host = super::present::ua_url_host(url).unwrap_or_default();
        if super::present::is_loopback_host(&host) {
            return Ok(if host.contains(':') {
                crate::bff::net::NetworkBinding::loopback_for(std::net::IpAddr::V6(
                    std::net::Ipv6Addr::LOCALHOST,
                ))
            } else {
                crate::bff::net::NetworkBinding::loopback()
            });
        }
        self.current_network_binding()
            .ok_or_else(|| tr(lang, Tr::SelectAdapterRequired).to_string())
    }

    /// Connect to an endpoint, opening its tab. Alias for `open_tab_for_endpoint`
    /// (the «Подключиться» button uses the same dedup/restore semantics as the
    /// double-click on a server row).
    pub fn connect_endpoint(&mut self, endpoint: EndpointSpec) -> bool {
        self.open_tab_for_endpoint(endpoint)
    }

    /// Disconnect a session from its server. The session container (endpoint,
    /// tab, rows, tree) is KEPT: only the link is dropped, rows are marked
    /// stale and the tab stays visible + active. This is the only non-destructive
    /// teardown path; `remove_session` fully drops the container.
    pub fn disconnect_session(&mut self, session: SessionId) {
        let lang = self.lang();
        self.cancel_add_children(session);
        // A new lifecycle begins: discard replies still in flight for the old
        // link (only the Disconnect sent below may come back).
        self.clear_pending_requests(session);
        let (protocol, endpoint_name) = {
            let Some(view) = self.sessions.iter().find(|s| s.id == session) else {
                return;
            };
            (view.protocol(), view.endpoint.display_name())
        };
        self.send(session, protocol, CommandKind::Disconnect);
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.state = SessionState::Disconnected;
            // A manual disconnect is not a lost link: Stopped prevents both the
            // backend's Disconnected event and later link-loss paths from
            // arming auto-reconnect until the user resumes/reconnects.
            view.reconnect = ReconnectState::Stopped;
            view.read_in_flight = false;
        }
        // Keep rows and the tree: tags remain visible with their last values,
        // just marked stale while the server is unreachable.
        for row in self.rows.iter_mut().filter(|r| r.session == session) {
            row.quality = Quality {
                code: 0,
                kind: QualityKind::Bad,
                text: tr(lang, Tr::StaleData).to_string(),
            };
        }
        self.release_da_routes_for_session(session);
        self.journal
            .info_l(Tr::DisconnectingFrom, &[&endpoint_name]);
    }

    /// Fully drop a hidden session container (the «Удалить из списка» path).
    /// This disconnects the server and removes the session and all its rows.
    /// It is the only destructive way to remove a container.
    pub fn remove_session(&mut self, session: SessionId) {
        self.cancel_add_children(session);
        self.clear_pending_requests(session);
        let (protocol, endpoint_name) = {
            let Some(view) = self.sessions.iter().find(|s| s.id == session) else {
                return;
            };
            (view.protocol(), view.endpoint.display_name())
        };
        self.send(session, protocol, CommandKind::Disconnect);
        self.rows.retain(|row| row.session != session);
        self.selected_rows
            .retain(|key| self.rows.iter().any(|row| row.key == *key));
        self.selected_tree = self.selected_tree.filter(|(sid, _)| *sid != session);
        self.tree_focus = self.tree_focus.filter(|(sid, _)| *sid != session);
        let rows_len = self.rows.len();
        if self.table_focus.map(|i| i >= rows_len).unwrap_or(false) {
            self.table_focus = None;
            self.table_anchor = None;
        }
        let closed_idx = self.sessions.iter().position(|s| s.id == session);
        self.sessions.retain(|s| s.id != session);
        self.release_da_routes_for_session(session);
        if self.active_session == Some(session) {
            self.active_session = None;
            if let Some(idx) = closed_idx {
                self.activate_nearest_visible_around(idx);
            } else {
                self.active_session = self.sessions.iter().find(|s| s.visible).map(|s| s.id);
            }
        }
        self.journal
            .info_l(Tr::DisconnectingFrom, &[&endpoint_name]);
    }

    /// Hide a session's view (close its tab). The container and its rows/tree
    /// stay in memory; a Connected container keeps running (closing a tab is not
    /// disconnecting). If the closed tab was active, the nearest remaining
    /// visible tab becomes active (or none).
    pub fn close_tab(&mut self, session: SessionId) {
        let Some(idx) = self.sessions.iter().position(|s| s.id == session) else {
            return;
        };
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.visible = false;
        }
        if self.active_session == Some(session) {
            self.active_session = None;
            self.activate_nearest_visible_around(idx);
        }
    }

    /// Make a session's container visible + active (restore its tab) without
    /// reconnecting. Used when clicking a server in the left-panel server list.
    pub fn activate_container(&mut self, session: SessionId) {
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.visible = true;
        }
        self.active_session = Some(session);
        self.selected_tree = None;
    }

    /// Activate the nearest visible tab around `closed_idx` (searching right
    /// then left), else the first visible tab, else none.
    fn activate_nearest_visible_around(&mut self, closed_idx: usize) {
        let n = self.sessions.len();
        for offset in 1..=n {
            let right = closed_idx + offset;
            let left = closed_idx as isize - offset as isize;
            if right < n && self.sessions[right].visible {
                self.active_session = Some(self.sessions[right].id);
                return;
            }
            if left >= 0 && self.sessions[left as usize].visible {
                self.active_session = Some(self.sessions[left as usize].id);
                return;
            }
        }
        self.active_session = self.sessions.iter().find(|s| s.visible).map(|s| s.id);
    }

    /// Automatic reconnect fired by the timer. The `Waiting` attempt number is
    /// kept during the attempt so the counter keeps showing it; a failure
    /// schedules the next attempt, a `Connected` moves to `Idle`.
    pub(super) fn attempt_auto_reconnect(&mut self, session: SessionId) {
        let Some((endpoint, attempt)) =
            self.sessions
                .iter()
                .find(|s| s.id == session)
                .and_then(|view| match view.reconnect {
                    ReconnectState::Waiting { attempt, .. } => {
                        Some((view.endpoint.clone(), attempt))
                    }
                    _ => None,
                })
        else {
            return;
        };
        if attempt > RECONNECT_MAX_ATTEMPTS {
            if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                view.reconnect = ReconnectState::Exhausted;
            }
            self.journal.warn_l(Tr::AttemptsExhausted20, &[]);
            return;
        }
        if endpoint.protocol == Protocol::OpcDa && !self.has_da_route_hold(session) {
            match self.prepare_da_route_for_host(&endpoint.host) {
                Ok(prepared) => self.hold_da_route(session, &endpoint.host, prepared),
                Err(error) => {
                    let message = error.localized(self.lang());
                    if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                        view.state = SessionState::Disconnected;
                        view.info = message.clone();
                        view.reconnect = ReconnectState::Stopped;
                        view.read_in_flight = false;
                    }
                    self.journal.error(message);
                    return;
                }
            }
        }
        let lang = self.lang();
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        let ReconnectState::Waiting { .. } = view.reconnect else {
            return;
        };
        view.state = SessionState::Connecting;
        view.info = tr(lang, Tr::Reconnecting).to_string();
        view.read_in_flight = false;
        let protocol = endpoint.protocol;
        // Old replies belong to the previous link and must not mutate this one.
        self.clear_pending_requests(session);
        self.send(session, protocol, CommandKind::Disconnect);
        self.send(session, protocol, CommandKind::Connect { endpoint });
        self.journal
            .info_l(Tr::AutoReconnectAtt, &[&attempt.to_string()]);
    }

    pub fn reconnect_session(&mut self, session: SessionId) {
        let Some(endpoint) = self
            .sessions
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.endpoint.clone())
        else {
            return;
        };
        if endpoint.protocol == Protocol::OpcDa && !self.has_da_route_hold(session) {
            match self.prepare_da_route_for_host(&endpoint.host) {
                Ok(prepared) => self.hold_da_route(session, &endpoint.host, prepared),
                Err(error) => {
                    let message = error.localized(self.lang());
                    if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                        view.state = SessionState::Disconnected;
                        view.info = message.clone();
                        view.reconnect = ReconnectState::Stopped;
                        view.read_in_flight = false;
                    }
                    self.journal.error(message);
                    return;
                }
            }
        }
        let lang = self.lang();
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        view.state = SessionState::Connecting;
        view.info = tr(lang, Tr::Reconnecting).to_string();
        view.reconnect = ReconnectState::Idle;
        view.read_in_flight = false;
        let protocol = endpoint.protocol;
        self.clear_pending_requests(session);
        self.send(session, protocol, CommandKind::Disconnect);
        self.send(session, protocol, CommandKind::Connect { endpoint });
        self.journal.info_l(Tr::ReconnectRestoring, &[]);
    }

    /// Stop the automatic reconnect loop for a session (manual reconnect is still allowed).
    pub fn stop_reconnect(&mut self, session: SessionId) {
        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
            view.reconnect = ReconnectState::Stopped;
            self.journal.info_l(Tr::AutoReconnectStoppedByUser, &[]);
        }
    }

    /// Re-arm automatic reconnect after a manual "Возобновить попытки": reset
    /// the state to Idle and, for a disconnected session, schedule attempt 1.
    pub fn resume_reconnect(&mut self, session: SessionId) {
        let auto = self.settings.options.auto_reconnect;
        let disconnected = {
            let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
                return;
            };
            let disconnected = view.state == SessionState::Disconnected;
            view.reconnect = ReconnectState::Idle;
            disconnected
        };
        if disconnected && auto {
            self.schedule_reconnect(session);
        }
    }

    /// Textual reconnect status for the status bar (attempt/backoff) if any.
    pub fn reconnect_hint(&self) -> Option<String> {
        let session = self.active_session?;
        let view = self.sessions.iter().find(|s| s.id == session)?;
        let lang = self.lang();
        let cap = RECONNECT_MAX_ATTEMPTS;
        if matches!(view.reconnect, ReconnectState::Exhausted) {
            return Some(tr(lang, Tr::AttemptsExhausted).to_string());
        }
        let aware =
            view.state == SessionState::Disconnected && self.settings.options.auto_reconnect;
        if !aware {
            return None;
        }
        match view.reconnect {
            ReconnectState::Waiting { at, attempt } => {
                let attempts = attempt.min(cap).max(1);
                let secs = at
                    .saturating_duration_since(Instant::now())
                    .as_secs()
                    .min(3600);
                Some(fmt(
                    lang,
                    Tr::ReconnectBackoff,
                    &[&attempts.to_string(), &cap.to_string(), &secs.to_string()],
                ))
            }
            // A manual stop is a deliberate decision: no status warning.
            ReconnectState::Stopped => None,
            // Idle but disconnected (e.g. auto-reconnect just re-armed and no
            // timer has been scheduled yet) keeps the historical "stopped" line.
            ReconnectState::Idle => Some(fmt(
                lang,
                Tr::ReconnectStopped,
                &[&"1".to_string(), &cap.to_string()],
            )),
            ReconnectState::Exhausted => unreachable!("handled above"),
        }
    }

    pub(super) fn schedule_reconnect(&mut self, session: SessionId) {
        if !self.settings.options.auto_reconnect {
            return;
        }
        let cap = RECONNECT_MAX_ATTEMPTS;
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        let base = match view.reconnect {
            // Manual stop / exhausted require an explicit resume or reconnect.
            ReconnectState::Stopped | ReconnectState::Exhausted => return,
            ReconnectState::Waiting { attempt, .. } => attempt,
            ReconnectState::Idle => 0,
        };
        if base >= cap {
            view.reconnect = ReconnectState::Exhausted;
            self.journal.warn_l(
                Tr::AttemptsExhaustedFor,
                &[&cap.to_string(), &view.endpoint.display_name()],
            );
            return;
        }
        let attempt = base + 1;
        let delay = reconnect_delay(attempt);
        view.reconnect = ReconnectState::Waiting {
            at: Instant::now() + delay,
            attempt,
        };
        self.journal.info_l(
            Tr::ConnLostReconnect,
            &[
                &attempt.to_string(),
                &cap.to_string(),
                &delay.as_secs().to_string(),
            ],
        );
    }
}

/// Exponential reconnect backoff: 1s, 2s, 4s, 8s, ... for attempt 1, 2, 3, 4.
/// Overflow-safe for absurdly large attempt numbers (kept to u64 seconds).
pub(super) fn reconnect_delay(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1);
    let secs = RECONNECT_BASE_DELAY_SECS
        .checked_shl(shift)
        .unwrap_or(u64::MAX);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod popup_identity_tests {
    use super::*;

    #[test]
    fn da_progid_identity_includes_source_computer() {
        let first = EndpointSpec::da("plant-a", "Vendor.OPC");
        let second = EndpointSpec::da("plant-b", "Vendor.OPC");
        assert_ne!(App::dedup_key(&first), App::dedup_key(&second));
        assert_eq!(
            App::dedup_key(&first),
            App::dedup_key(&EndpointSpec::da("PLANT-A", "vendor.opc"))
        );
    }
}
