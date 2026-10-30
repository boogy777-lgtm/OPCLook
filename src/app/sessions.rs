use super::*;
use crate::bff::model::{CommandKind, EndpointSpec, Protocol, SessionId};
use crate::constants::RECONNECT_MAX_ATTEMPTS;
#[cfg(test)]
use crate::constants::{RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY};
use crate::i18n::{Tr, fmt, tr};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
enum UaUrlKey {
    Valid(crate::bff::model::UaUrlIdentity),
    Invalid(String),
}

#[derive(Debug, PartialEq, Eq)]
enum EndpointKey {
    Da {
        host: String,
        address: String,
        acquisition: Option<crate::bff::model::AcquisitionConfig>,
    },
    Ua {
        url: UaUrlKey,
        policy: crate::bff::model::UaSecurityPolicy,
        mode: crate::bff::model::UaSecurityMode,
        acquisition: Option<crate::bff::model::AcquisitionConfig>,
    },
}

impl App {
    /// Unique identity used for endpoint deduplication. Two sessions are the
    /// same container iff they share the protocol and the address (ProgID for
    /// DA, endpoint URL for UA); the DA `host` is just a computer-name filter,
    /// not part of the server's identity.
    ///
    /// V2: a UA endpoint is the URL together with its security policy and
    /// mode (as the server tree lists them), so two endpoints of one server
    /// are two sessions, each with its own tags, instead of one session that
    /// reconnects with the other security.
    fn dedup_key(endpoint: &EndpointSpec) -> EndpointKey {
        match endpoint.protocol {
            Protocol::OpcDa => EndpointKey::Da {
                host: endpoint.host.trim().to_ascii_lowercase(),
                address: endpoint.address.trim().to_ascii_lowercase(),
                acquisition: endpoint.acquisition.clone(),
            },
            Protocol::OpcUa => {
                let url = match endpoint.address.parse::<crate::bff::model::UaUrl>() {
                    Ok(url) => UaUrlKey::Valid(url.identity()),
                    Err(_) => UaUrlKey::Invalid(endpoint.address.clone()),
                };
                EndpointKey::Ua {
                    url,
                    policy: endpoint.ua_security_policy,
                    mode: endpoint.ua_security_mode,
                    acquisition: endpoint.acquisition.clone(),
                }
            }
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
        if opclook_engine::validate_acquisition(&endpoint).is_err() {
            let message = tr(self.lang(), Tr::AcquisitionInvalid).to_string();
            self.status.clone_from(&message);
            self.journal.error(message.clone());
            if self.dialogs.connect.open {
                self.dialogs.connect.error = Some(message);
            }
            return false;
        }
        if endpoint.protocol == Protocol::OpcUa
            && endpoint
                .address
                .parse::<crate::bff::model::UaUrl>()
                .is_err()
        {
            let message = tr(self.lang(), Tr::HintUaUrl).to_string();
            self.status.clone_from(&message);
            self.journal.error(message.clone());
            if self.dialogs.connect.open {
                self.dialogs.connect.error = Some(message);
            }
            return false;
        }
        let key = Self::dedup_key(&endpoint);
        if let Some(id) = self
            .sessions
            .iter()
            .find(|s| Self::dedup_key(s.endpoint()) == key)
            .map(|s| s.id)
        {
            let mut reconfigure = false;
            if let Some(view) = self.sessions.iter_mut().find(|s| s.id == id) {
                view.visible = true;
                if endpoint.protocol == Protocol::OpcUa {
                    reconfigure = view.endpoint().user != endpoint.user
                        || view.endpoint().password != endpoint.password
                        || view.endpoint().ua_use_certificate != endpoint.ua_use_certificate
                        || view.endpoint().ua_security_policy != endpoint.ua_security_policy
                        || view.endpoint().ua_security_mode != endpoint.ua_security_mode
                        || view.endpoint().ua_trust_server_cert != endpoint.ua_trust_server_cert
                        || view.endpoint().ua_session_timeout != endpoint.ua_session_timeout;
                    if reconfigure {
                        let mut updated = endpoint;
                        // Existing sessions keep their selected NIC until
                        // explicitly removed; reauthentication does not rebind.
                        updated.ua_bind.clone_from(&view.endpoint().ua_bind);
                        view.update_endpoint(updated);
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
            self.status.clone_from(&message);
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
        if endpoint.protocol == Protocol::OpcUa && endpoint.ua_bind.is_none() {
            match self.ua_binding_for_endpoint(&endpoint.address) {
                Ok(binding) => endpoint.ua_bind = Some(binding),
                Err(message) => {
                    self.status.clone_from(&message);
                    self.journal.error(message.clone());
                    if self.dialogs.connect.open {
                        self.dialogs.connect.error = Some(message);
                    }
                    return false;
                }
            }
        }
        let Some(id) = self.next_session else {
            let message = tr(self.lang(), Tr::SessionIdsExhausted).to_string();
            self.status.clone_from(&message);
            self.journal.error(message.clone());
            if self.dialogs.connect.open {
                self.dialogs.connect.error = Some(message);
            }
            return false;
        };
        self.next_session = id.successor();
        self.settings.remember(&endpoint);
        self.persist_settings();
        self.journal.info_l(
            Tr::ConnectingTo,
            &[&endpoint.display_name(), &endpoint.protocol.to_string()],
        );
        self.sessions
            .push(SessionView::new(id, endpoint.clone(), self.lang()));
        self.active_session = Some(id);
        self.selected_tree = None;
        if endpoint.protocol == Protocol::OpcDa {
            if let Err(error) =
                self.send_da_prepared(id, CommandKind::Connect { endpoint }, PendingCtx::None)
            {
                let message = error.localized(self.lang());
                self.status.clone_from(&message);
                self.journal.error(message.clone());
                if let Some(view) = self.sessions.iter_mut().find(|view| view.id == id) {
                    view.state = SessionState::Disconnected;
                    view.info.clone_from(&message);
                }
                if self.dialogs.connect.open {
                    self.dialogs.connect.error = Some(message);
                }
                return false;
            }
        } else {
            if self
                .send(id, endpoint.protocol, CommandKind::Connect { endpoint })
                .is_none()
            {
                if let Some(view) = self.sessions.iter_mut().find(|view| view.id == id) {
                    view.state = SessionState::Disconnected;
                    view.info.clone_from(&self.status);
                }
                if self.dialogs.connect.open {
                    self.dialogs.connect.error = Some(self.status.clone());
                }
                return false;
            }
        }
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

    /// «Подключиться» from the connect dialog. The dialog is NOT closed after
    /// the command is sent: it stays open (spinner on the button) until this
    /// attempt reports `Connected` (closes) or `Error` (message shown inside
    /// the dialog). When no `Connect` is in flight afterwards (an already live
    /// container was just activated) the dialog closes at once. A disconnected
    /// container that is reused is reconnected, because the user explicitly
    /// asked to connect. Returns `false` when the attempt was rejected and the
    /// error is shown in the dialog.
    pub fn connect_from_dialog(&mut self, endpoint: EndpointSpec) -> bool {
        let key = Self::dedup_key(&endpoint);
        let was_disconnected = self
            .sessions
            .iter()
            .find(|s| Self::dedup_key(s.endpoint()) == key)
            .is_some_and(|s| s.state == SessionState::Disconnected);
        if !self.open_tab_for_endpoint(endpoint) {
            return false;
        }
        let Some(id) = self.active_session else {
            self.close_connect_dialog();
            return true;
        };
        let mut state = self.sessions.iter().find(|s| s.id == id).map(|s| s.state);
        if was_disconnected && state == Some(SessionState::Disconnected) {
            self.reconnect_session(id);
            state = self.sessions.iter().find(|s| s.id == id).map(|s| s.state);
        }
        match state {
            Some(SessionState::Connecting) => {
                self.dialogs.connect.connecting = Some(id);
                self.dialogs.connect.error = None;
                true
            }
            Some(SessionState::Disconnected) => {
                // The command could not even be dispatched (dead worker, DA
                // route refused): the reason is already in `view.info`.
                let info = self
                    .sessions
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.info.clone())
                    .unwrap_or_default();
                self.dialogs.connect.error = Some(info);
                false
            }
            _ => {
                self.close_connect_dialog();
                true
            }
        }
    }

    /// Disconnect a session from its server. The session container (endpoint,
    /// tab, rows, tree) is KEPT: only the link is dropped, rows are marked
    /// stale and the tab stays visible + active. This is the only non-destructive
    /// teardown path; `remove_session` fully drops the container.
    pub fn disconnect_session(&mut self, session: SessionId) {
        if !self.engine.has_request_capacity() {
            let message = tr(self.lang(), Tr::RequestIdsExhausted).to_string();
            self.status.clone_from(&message);
            self.journal.error(message);
            return;
        }
        self.cancel_add_children(session);
        if self.dialogs.connect.connecting == Some(session) {
            self.dialogs.connect.connecting = None;
        }
        // A new lifecycle begins: discard replies still in flight for the old
        // link (only the Disconnect sent below may come back).
        let (protocol, endpoint_name) = {
            let Some(view) = self.session(session) else {
                return;
            };
            (view.protocol(), view.endpoint().display_name())
        };
        self.engine.manual_disconnect(session);
        self.project_session(session);
        self.mark_link_lost(session, None);
        if self
            .send(session, protocol, CommandKind::Disconnect)
            .is_none()
        {
            return;
        }
        self.release_da_routes_for_session(session);
        self.journal
            .info_l(Tr::DisconnectingFrom, &[&endpoint_name]);
    }

    /// Fully drop a hidden session container (the «Удалить из списка» path).
    /// This disconnects the server and removes the session and all its rows.
    /// It is the only destructive way to remove a container.
    pub fn remove_session(&mut self, session: SessionId) {
        if !self.engine.has_request_capacity() {
            let message = tr(self.lang(), Tr::RequestIdsExhausted).to_string();
            self.status.clone_from(&message);
            self.journal.error(message);
            return;
        }
        self.cancel_add_children(session);
        if self.dialogs.connect.connecting == Some(session) {
            self.dialogs.connect.connecting = None;
        }
        self.clear_pending_requests(session);
        let (protocol, endpoint_name) = {
            let Some(view) = self.session(session) else {
                return;
            };
            (view.protocol(), view.endpoint().display_name())
        };
        if self
            .send(session, protocol, CommandKind::Disconnect)
            .is_none()
        {
            return;
        }
        self.rows.retain(|row| row.session != session);
        self.rows_changed();
        let existing: std::collections::HashSet<_> = self.rows.iter().map(|row| row.key).collect();
        self.selected_rows.retain(|key| existing.contains(key));
        self.selected_tree = self.selected_tree.filter(|(sid, _)| *sid != session);
        self.tree_focus = self.tree_focus.filter(|(sid, _)| *sid != session);
        let rows_len = self.rows.len();
        if self.table_focus.map(|i| i >= rows_len).unwrap_or(false) {
            self.table_focus = None;
            self.table_anchor = None;
        }
        if self.active_session == Some(session) {
            self.active_session = None;
            if let Some(idx) = self.sessions.iter().position(|s| s.id == session) {
                self.activate_nearest_visible_around(idx);
                if self.active_session == Some(session) {
                    self.active_session = None;
                }
            }
        }
        self.sessions.retain(|s| s.id != session);
        self.engine.remove_session(session);
        self.release_da_routes_for_session(session);
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
        if let Some(view) = self.session_mut(session) {
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
        if let Some(view) = self.session_mut(session) {
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
            self.session(session).and_then(|view| match view.reconnect {
                ReconnectState::Waiting { attempt, .. } => Some((view.endpoint().clone(), attempt)),
                _ => None,
            })
        else {
            return;
        };
        if attempt > RECONNECT_MAX_ATTEMPTS {
            if let Some(view) = self.session_mut(session) {
                view.reconnect = ReconnectState::Exhausted;
            }
            self.journal.warn_l(Tr::AttemptsExhausted20, &[]);
            return;
        }
        if !self.engine.has_request_capacity() {
            self.engine.stop_reconnect(session);
            self.project_session(session);
            let message = tr(self.lang(), Tr::RequestIdsExhausted).to_string();
            self.status.clone_from(&message);
            self.journal.error(message);
            return;
        }
        let lang = self.lang();
        let Some(view) = self.session_mut(session) else {
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
        self.cancel_write_confirmation_for_session(session);
        if let Err(cause) = self.engine.disconnect_untracked(session, protocol) {
            let message = opclook_locale::send_error_text(self.lang(), cause).to_string();
            self.engine.manual_disconnect(session);
            if let Some(view) = self.session_mut(session) {
                view.state = SessionState::Disconnected;
                view.info.clone_from(&message);
            }
            self.status.clone_from(&message);
            self.journal.error(message);
            return;
        }
        self.clear_pending_requests(session);
        if protocol == Protocol::OpcDa {
            if let Err(error) =
                self.send_da_prepared(session, CommandKind::Connect { endpoint }, PendingCtx::None)
            {
                let message = error.localized(self.lang());
                self.engine.manual_disconnect(session);
                if let Some(view) = self.session_mut(session) {
                    view.state = SessionState::Disconnected;
                    view.info.clone_from(&message);
                }
                self.journal.error(message);
            }
        } else {
            if self
                .send(session, protocol, CommandKind::Connect { endpoint })
                .is_none()
            {
                self.engine.manual_disconnect(session);
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.state = SessionState::Disconnected;
                    view.info.clone_from(&self.status);
                }
                return;
            }
        }
        self.journal
            .info_l(Tr::AutoReconnectAtt, &[&attempt.to_string()]);
    }

    /// The toolbar «Подключиться» (A7). With a disconnected
    /// active session it reconnects that session (the dialog would only make the
    /// user retype what is already known); in every other case, including the
    /// first start with no sessions, it opens the connect dialog.
    pub fn toolbar_connect(&mut self) {
        if self
            .ui_action_unavailable(super::actions::UiAction::OpenConnectDialog)
            .is_some()
        {
            return;
        }
        let disconnected = self
            .active_session
            .and_then(|id| self.sessions.iter().find(|s| s.id == id))
            .filter(|s| s.state == SessionState::Disconnected)
            .map(|s| s.id);
        match disconnected {
            Some(id) => self.reconnect_session(id),
            None => {
                let _ = self.execute_ui_action(super::actions::UiAction::OpenConnectDialog);
            }
        }
    }

    pub fn reconnect_session(&mut self, session: SessionId) {
        if !self.engine.has_request_capacity() {
            let message = tr(self.lang(), Tr::RequestIdsExhausted).to_string();
            self.status.clone_from(&message);
            self.journal.error(message);
            return;
        }
        self.engine.resume_reconnect(session);
        let Some(endpoint) = self.session(session).map(|s| s.endpoint().clone()) else {
            return;
        };
        let lang = self.lang();
        let Some(view) = self.session_mut(session) else {
            return;
        };
        view.state = SessionState::Connecting;
        view.info = tr(lang, Tr::Reconnecting).to_string();
        view.reconnect = ReconnectState::Idle;
        view.read_in_flight = false;
        let protocol = endpoint.protocol;
        self.cancel_write_confirmation_for_session(session);
        if let Err(cause) = self.engine.disconnect_untracked(session, protocol) {
            let message = opclook_locale::send_error_text(self.lang(), cause).to_string();
            self.engine.manual_disconnect(session);
            if let Some(view) = self.session_mut(session) {
                view.state = SessionState::Disconnected;
                view.info.clone_from(&message);
            }
            self.status.clone_from(&message);
            self.journal.error(message);
            return;
        }
        self.clear_pending_requests(session);
        if protocol == Protocol::OpcDa {
            if let Err(error) =
                self.send_da_prepared(session, CommandKind::Connect { endpoint }, PendingCtx::None)
            {
                let message = error.localized(self.lang());
                self.engine.manual_disconnect(session);
                if let Some(view) = self.session_mut(session) {
                    view.state = SessionState::Disconnected;
                    view.info.clone_from(&message);
                }
                self.journal.error(message);
            }
        } else {
            if self
                .send(session, protocol, CommandKind::Connect { endpoint })
                .is_none()
            {
                self.engine.manual_disconnect(session);
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.state = SessionState::Disconnected;
                    view.info.clone_from(&self.status);
                }
                return;
            }
        }
        self.journal.info_l(Tr::ReconnectRestoring, &[]);
    }

    /// Stop the automatic reconnect loop for a session (manual reconnect is still allowed).
    pub fn stop_reconnect(&mut self, session: SessionId) {
        self.engine.stop_reconnect(session);
        if let Some(view) = self.session_mut(session) {
            view.reconnect = ReconnectState::Stopped;
            self.journal.info_l(Tr::AutoReconnectStoppedByUser, &[]);
        }
    }

    /// Re-arm automatic reconnect after a manual "Возобновить попытки": reset
    /// the state to Idle and, for a disconnected session, schedule attempt 1.
    pub fn resume_reconnect(&mut self, session: SessionId) {
        self.engine.resume_reconnect(session);
        let auto = self.settings.view.options.auto_reconnect;
        let disconnected = {
            let Some(view) = self.session_mut(session) else {
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
        let view = self.session(session)?;
        let lang = self.lang();
        let cap = RECONNECT_MAX_ATTEMPTS;
        if matches!(view.reconnect, ReconnectState::Exhausted) {
            return Some(tr(lang, Tr::AttemptsExhausted).to_string());
        }
        let aware =
            view.state == SessionState::Disconnected && self.settings.view.options.auto_reconnect;
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

    pub(super) fn log_reconnect(&mut self, session: SessionId) {
        let Some(view) = self.session(session) else {
            return;
        };
        match view.reconnect {
            ReconnectState::Waiting { at, attempt } => self.journal.info_l(
                Tr::ConnLostReconnect,
                &[
                    &attempt.to_string(),
                    &RECONNECT_MAX_ATTEMPTS.to_string(),
                    &at.saturating_duration_since(Instant::now())
                        .as_secs()
                        .to_string(),
                ],
            ),
            ReconnectState::Exhausted => self.journal.warn_l(
                Tr::AttemptsExhaustedFor,
                &[
                    &RECONNECT_MAX_ATTEMPTS.to_string(),
                    &view.endpoint().display_name(),
                ],
            ),
            _ => {}
        }
    }

    pub(super) fn schedule_reconnect(&mut self, session: SessionId) {
        if !self.settings.view.options.auto_reconnect {
            return;
        }
        let cap = RECONNECT_MAX_ATTEMPTS;
        let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return;
        };
        self.engine
            .set_auto_reconnect(self.settings.view.options.auto_reconnect);
        let (attempt, delay) = match self.engine.schedule_reconnect(session, Instant::now()) {
            opclook_engine::ReconnectScheduled::Unchanged => return,
            opclook_engine::ReconnectScheduled::Exhausted => {
                self.journal.warn_l(
                    Tr::AttemptsExhaustedFor,
                    &[&cap.to_string(), &view.endpoint().display_name()],
                );
                view.reconnect = ReconnectState::Exhausted;
                return;
            }
            opclook_engine::ReconnectScheduled::Waiting { attempt, delay } => (attempt, delay),
        };
        if let Some(runtime) = self.engine.session(session) {
            view.reconnect = runtime.reconnect;
        }
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

/// Exponential reconnect backoff: 1s, 2s, 4s, 8s, ... for attempt 1, 2, 3, 4,
/// capped at `RECONNECT_MAX_DELAY`. Overflow-safe for absurd attempt
/// numbers.
#[cfg(test)]
pub(super) fn reconnect_delay(attempt: u32) -> Duration {
    opclook_engine::reconnect_delay(attempt, RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY)
}

#[cfg(test)]
mod popup_identity_tests {
    use super::*;

    #[test]
    fn ua_session_identity_normalizes_host_and_port_but_preserves_resource() {
        let a = EndpointSpec::ua("opc.tcp://HOST/A?x=/");
        assert_eq!(
            App::dedup_key(&a),
            App::dedup_key(&EndpointSpec::ua("opc.tcp://host:4840/A/?x=/"))
        );
        assert_ne!(
            App::dedup_key(&a),
            App::dedup_key(&EndpointSpec::ua("opc.tcp://host/a?x=/"))
        );
        assert_ne!(
            App::dedup_key(&a),
            App::dedup_key(&EndpointSpec::ua("opc.tcp://host/A?x="))
        );
        assert_ne!(
            App::dedup_key(&a),
            App::dedup_key(&EndpointSpec::ua("opc.tcp://host:4850/A?x=/"))
        );
    }

    #[test]
    fn legacy_auto_security_forms_have_the_same_session_identity() {
        let original = EndpointSpec::ua("opc.tcp://host:4840");
        let base = serde_json::to_value(&original).unwrap();
        for value in [serde_json::Value::Null, serde_json::json!("Auto")] {
            let mut legacy = base.clone();
            legacy["ua_security_policy"] = value.clone();
            legacy["ua_security_mode"] = value;
            let restored: EndpointSpec = serde_json::from_value(legacy).unwrap();
            assert_eq!(App::dedup_key(&original), App::dedup_key(&restored));
        }
        let mut secured = original.clone();
        secured.ua_security_policy = crate::bff::model::UaSecurityPolicy::Basic256Sha256;
        secured.ua_security_mode = crate::bff::model::UaSecurityMode::SignAndEncrypt;
        assert_ne!(App::dedup_key(&original), App::dedup_key(&secured));
    }

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
