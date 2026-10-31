use super::present::{compose_server_name, localize_scan_error, operation_label};
use super::search::{merge_ua_scan_servers, merge_ua_servers, refresh_ua_combos};
use super::tree::{insert_children, replace_roots, set_children};
use super::*;
use crate::bff::model::{
    BrowseNode, Event, EventKind, ItemInfo, ItemKey, NodeKind, Operation, Origin, Owner, Protocol,
    RequestId, ScanEnd, SessionId,
};
use crate::i18n::{Tr, backend_text, fmt, tr};
use chrono::Utc;
use std::time::{Duration, Instant};

impl App {
    fn session_backend_text(
        &self,
        session: SessionId,
        message: &crate::bff::model::BackendMsg,
    ) -> String {
        let mut text = backend_text(self.lang(), message);
        if let Some(view) = self.session(session) {
            let endpoint = view.endpoint();
            let mut secrets = [&endpoint.user, &endpoint.password];
            secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
            for secret in secrets {
                if !secret.is_empty() {
                    text = text.replace(secret, tr(self.lang(), Tr::DiagnosticRedacted));
                }
            }
        }
        text
    }

    pub(super) fn poll_events(&mut self) {
        while let Some(event) = self.engine.try_recv() {
            self.on_event(event);
            self.drain_read_observations();
        }
        self.drain_read_observations();
    }

    /// Project the latest observation per key on the UI thread. No sink
    /// callback performs UI work or I/O; superseded and overflow counts are
    /// made visible rather than silently hidden.
    pub(super) fn drain_read_observations(&mut self) {
        use crate::bff::model::{ReadCancelReason, ReadTerminalStatus};
        let batch = self
            .read_observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain();
        if batch.replaced != 0 || batch.overflow != 0 {
            self.journal.warn_l(
                Tr::ReadTerminalSummaryLoss,
                &[&batch.replaced, &batch.overflow],
            );
        }
        for observation in batch.latest {
            let session = observation.session;
            let terminal = &observation.terminal;
            let lang = self.lang();
            let (detail, failed) = match &terminal.status {
                ReadTerminalStatus::Received(value) => {
                    let reason = value
                        .reason
                        .as_ref()
                        .map(|r| self.session_backend_text(session, r))
                        .unwrap_or_default();
                    let quality = self.session_backend_text(
                        session,
                        &crate::bff::model::BackendMsg::Raw(value.quality.text.to_string()),
                    );
                    (
                        fmt(lang, Tr::ReadTerminalReceived, &[&quality, &reason]),
                        value.quality.kind != QualityKind::Good || value.reason.is_some(),
                    )
                }
                ReadTerminalStatus::ReceivedMetadata {
                    quality, reason, ..
                } => {
                    let failed = quality.kind != QualityKind::Good || reason.is_some();
                    let reason = reason
                        .as_ref()
                        .map(|r| self.session_backend_text(session, r))
                        .unwrap_or_default();
                    let text = self.session_backend_text(
                        session,
                        &crate::bff::model::BackendMsg::Raw(quality.text.to_string()),
                    );
                    (
                        fmt(lang, Tr::ReadTerminalReceived, &[&text, &reason]),
                        failed,
                    )
                }
                ReadTerminalStatus::InitialPush {
                    quality, reason, ..
                } => {
                    let has_reason = reason.is_some();
                    let reason = reason
                        .as_ref()
                        .map(|r| self.session_backend_text(session, r))
                        .unwrap_or_default();
                    let text = self.session_backend_text(
                        session,
                        &crate::bff::model::BackendMsg::Raw(quality.text.to_string()),
                    );
                    (
                        fmt(lang, Tr::ReadTerminalInitial, &[&text, &reason]),
                        quality.kind != QualityKind::Good || has_reason,
                    )
                }
                ReadTerminalStatus::MissingResult => {
                    (tr(lang, Tr::ReadTerminalMissing).to_string(), true)
                }
                ReadTerminalStatus::ServiceError(reason) => (
                    fmt(
                        lang,
                        Tr::ReadTerminalServiceError,
                        &[&self.session_backend_text(session, reason)],
                    ),
                    true,
                ),
                ReadTerminalStatus::Timeout { seconds } => {
                    (fmt(lang, Tr::ReadTerminalTimeout, &[seconds]), true)
                }
                ReadTerminalStatus::Cancelled(reason) => {
                    let reason = tr(
                        lang,
                        match reason {
                            ReadCancelReason::Removed => Tr::ReadCancelRemoved,
                            ReadCancelReason::Disconnected => Tr::ReadCancelDisconnected,
                            ReadCancelReason::EpochChanged => Tr::ReadCancelEpoch,
                            ReadCancelReason::Cancelled => Tr::ReadCancelExplicit,
                        },
                    );
                    (fmt(lang, Tr::ReadTerminalCancelled, &[&reason]), false)
                }
                ReadTerminalStatus::DispatchFailed(reason) => (
                    fmt(
                        lang,
                        Tr::ReadTerminalDispatchFailed,
                        &[&opclook_locale::send_error_text(lang, *reason)],
                    ),
                    true,
                ),
                ReadTerminalStatus::Unavailable => {
                    (tr(lang, Tr::ReadTerminalUnavailable).to_string(), true)
                }
                ReadTerminalStatus::UnexpectedReply => {
                    (tr(lang, Tr::ReadTerminalUnexpected).to_string(), true)
                }
            };
            let request = terminal
                .request_id
                .map(|id| id.to_string())
                .unwrap_or_default();
            let epoch = observation
                .epoch
                .map(|id| id.to_string())
                .unwrap_or_default();
            let needs = terminal
                .needs
                .iter()
                .map(|need| {
                    tr(
                        lang,
                        match need {
                            crate::bff::model::ReadNeed::Initial => Tr::ReadNeedInitial,
                            crate::bff::model::ReadNeed::Poll => Tr::ReadNeedPoll,
                            crate::bff::model::ReadNeed::ExplicitRefresh => Tr::ReadNeedRefresh,
                            crate::bff::model::ReadNeed::WriteReadback => Tr::ReadNeedReadback,
                            crate::bff::model::ReadNeed::RecoverySnapshot => Tr::ReadNeedRecovery,
                            crate::bff::model::ReadNeed::Direct => Tr::ReadNeedDirect,
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let routine_poll = !failed
                && terminal
                    .needs
                    .iter()
                    .all(|need| *need == crate::bff::model::ReadNeed::Poll);
            if failed {
                self.journal.warn_l(
                    Tr::ReadTerminalLine,
                    &[&session, &epoch, &terminal.key, &request, &needs, &detail],
                );
            } else if !routine_poll {
                self.journal.info_l(
                    Tr::ReadTerminalLine,
                    &[&session, &epoch, &terminal.key, &request, &needs, &detail],
                );
            }
            // A late/removed registration may be logged, but must not mutate a
            // row belonging to another epoch or a replacement registration.
            if self.engine.connection_id(session) == observation.epoch {
                if let Some(row) = self
                    .row_position(terminal.key)
                    .and_then(|i| self.rows.get_mut(i))
                    .filter(|row| row.session == session)
                {
                    row.last_read = Some(observation.clone());
                    // Server quality is already projected from Values; do not
                    // overwrite its native Bad/Uncertain code with a summary.
                    if failed
                        && !matches!(
                            terminal.status,
                            ReadTerminalStatus::Received(_)
                                | ReadTerminalStatus::ReceivedMetadata { .. }
                                | ReadTerminalStatus::InitialPush { .. }
                        )
                    {
                        row.quality = RowQuality::Error(detail.clone());
                        row.pending = false;
                        row.invalidate_display();
                    }
                }
                if failed {
                    self.status = detail;
                }
            }
        }
    }

    /// Test-only hook to feed an event directly into the app state.
    #[cfg(feature = "test-support")]
    pub fn on_event_for_test(&mut self, kind: EventKind) {
        self.sync_fixture_sessions();
        self.on_event(Event {
            origin: Origin::Unsolicited,
            owner: Owner::Discovery,
            kind,
        });
    }

    /// Feed an event after fixture sessions have been synchronized once with
    /// `tick_for_test`; avoids rebuilding the fixture during measurements.
    #[cfg(feature = "test-support")]
    pub fn on_synced_event_for_test_at(&mut self, session: SessionId, kind: EventKind) {
        self.on_event(Event {
            origin: Origin::Session(
                self.engine
                    .connection_id_for_test(session)
                    .unwrap_or(RequestId::FIRST),
            ),
            owner: Owner::Session(session),
            kind,
        });
    }

    /// Test-only hook to feed an event for a specific session into the app state.
    #[cfg(feature = "test-support")]
    pub fn on_event_for_test_at(&mut self, session: SessionId, kind: EventKind) {
        self.sync_fixture_sessions();
        self.on_event(Event {
            origin: Origin::Session(
                self.engine
                    .connection_id_for_test(session)
                    .unwrap_or(RequestId::FIRST),
            ),
            owner: Owner::Session(session),
            kind,
        });
    }

    /// Test-only hook to feed an event with an explicit request id (used to
    /// exercise stale-reply rejection and request-id correlation).
    #[cfg(feature = "test-support")]
    pub fn on_event_for_test_with_request(
        &mut self,
        owner: impl Into<Owner>,
        request_id: RequestId,
        kind: EventKind,
    ) {
        self.sync_fixture_sessions();
        self.on_event(Event {
            origin: Origin::Reply(request_id),
            owner: owner.into(),
            kind,
        });
    }

    /// Test-only hook to run the per-frame update loop (browse/read/reconnect
    /// scheduling) without touching the UI.
    #[cfg(feature = "test-support")]
    pub fn tick_for_test(&mut self) {
        self.sync_fixture_sessions();
        self.tick();
    }

    /// Test-only: re-register a session's rows as on (re)connect.
    pub fn register_rows_for_test(&mut self, session: SessionId) {
        self.register_rows(session);
    }

    /// Test-only: make a session the active one (for reconnect-hint lookups).
    pub fn activate_session(&mut self, session: SessionId) {
        self.active_session = Some(session);
    }

    pub(super) fn on_event(&mut self, event: Event) {
        let owner = event.owner;
        // Correlated replies are accepted once while their command is outstanding;
        // unsolicited pushes and worker runtime errors have no request token.
        self.engine
            .set_auto_reconnect(self.settings.view.options.auto_reconnect);
        let mut completed_read = false;
        let ctx = match self.engine.accept(&event) {
            Ok(Some(pending)) => {
                completed_read = pending.operation == Operation::Read;
                if self.settings.view.options.diagnostic_trace {
                    let latency = pending.sent.elapsed().as_millis();
                    self.journal.info_l(
                        Tr::TraceReply,
                        &[
                            &match event.origin {
                                Origin::Reply(id) => id.to_string(),
                                Origin::Unsolicited
                                | Origin::Session(_)
                                | Origin::UaSession { .. } => String::new(),
                            },
                            &operation_label(self.lang(), pending.operation),
                            &owner_label(self.lang(), owner),
                            &latency.to_string(),
                        ],
                    );
                }
                pending.ctx
            }
            Ok(None) => PendingCtx::None,
            Err(()) => {
                if self.settings.view.options.diagnostic_trace {
                    self.journal.info_l(
                        Tr::TraceDropped,
                        &[
                            &match event.origin {
                                Origin::Reply(id) => id.to_string(),
                                Origin::Unsolicited
                                | Origin::Session(_)
                                | Origin::UaSession { .. } => String::new(),
                            },
                            &owner_label(self.lang(), owner),
                        ],
                    );
                }
                return;
            }
        };
        if completed_read {
            self.pump_add_children();
        }
        let request_id = match event.origin {
            Origin::Reply(id) => Some(id),
            Origin::Unsolicited | Origin::Session(_) | Origin::UaSession { .. } => None,
        };
        let kind = match (event.kind, &ctx) {
            (
                EventKind::Error {
                    operation: Operation::Write,
                    message,
                },
                PendingCtx::WriteKey(key),
            ) => EventKind::Wrote {
                key: *key,
                result: crate::bff::model::WriteOutcome::from_dispatch_error(message),
            },
            (kind, _) => kind,
        };
        let session = match owner {
            Owner::Session(session) => session,
            Owner::Discovery => {
                self.on_discovery_event(kind, ctx, request_id);
                return;
            }
        };
        self.project_session(session);
        if matches!(
            &kind,
            EventKind::UaSubscriptionParameters { .. }
                | EventKind::UaRecovery { .. }
                | EventKind::Connected { .. }
                | EventKind::Disconnected { .. }
                | EventKind::Error {
                    operation: Operation::Connect | Operation::Test,
                    ..
                }
        ) {
            self.project_ua_negotiated(session);
        }
        match kind {
            EventKind::Error { operation, message } => {
                self.on_operation_error(session, operation, message, ctx, request_id)
            }
            kind @ (EventKind::Connected { .. }
            | EventKind::Disconnected { .. }
            | EventKind::Log { .. }) => self.on_session_event(session, kind),
            kind => self.on_data_event(session, kind, ctx, request_id),
        }
    }

    fn on_session_event(&mut self, session: SessionId, kind: EventKind) {
        match kind {
            EventKind::Connected { endpoint, info } => {
                let info = self.session_backend_text(session, &info);
                let is_ua = self.session_protocol(session) == Some(Protocol::OpcUa);
                // Any other reply from the old link is now superseded.
                self.clear_pending_requests(session);
                if let Some(view) = self.session_mut(session) {
                    view.state = SessionState::Connected;
                    view.info.clone_from(&info);
                    view.display_name = compose_server_name(&endpoint);
                    view.reconnect = ReconnectState::Idle;
                    view.root_loaded = false;
                    view.root_loading = true;
                    view.read_in_flight = false;
                    view.clear_tree();
                    view.next_tree_id = 1;
                    if let Some(ua) = view.ua_mut() {
                        ua.ua_session_timeout_ms = endpoint
                            .ua_session_timeout
                            .unwrap_or_else(crate::settings::default_session_timeout);
                    }
                    let now = Instant::now();
                    view.last_poll = now.checked_sub(Duration::from_secs(5)).unwrap_or(now);
                    view.last_test = now;
                } else {
                    return;
                }
                self.journal
                    .info_l(Tr::ConnectedInfo, &[&endpoint.display_name(), &info]);
                self.browse(session, String::new());
                if is_ua {
                    self.configure_ua_subscription(session);
                }
                self.register_rows(session);
                // Only the attempt the dialog is waiting for closes it; a
                // background reconnect of another session must not.
                if self.dialogs.connect.connecting == Some(session) {
                    self.dialogs.connect.connecting = None;
                    self.close_connect_dialog();
                }
            }
            EventKind::Disconnected { reason } => {
                let reason = self.session_backend_text(session, &reason);
                // A late disconnect from the old link must not interrupt a
                // new connection attempt.
                let name = self
                    .session(session)
                    .filter(|view| view.state != SessionState::Connecting)
                    .map(|view| view.endpoint().display_name());
                if let Some(name) = name {
                    self.mark_link_lost(session, Some(&reason));
                    self.journal.info_l(Tr::ConnectedInfo, &[&name, &reason]);
                    self.log_reconnect(session);
                }
            }
            EventKind::Log { message } => {
                self.journal
                    .info(self.session_backend_text(session, &message));
            }
            _ => {}
        }
    }

    fn on_data_event(
        &mut self,
        session: SessionId,
        kind: EventKind,
        ctx: PendingCtx,
        request_id: Option<RequestId>,
    ) {
        let lang = self.lang();
        match kind {
            EventKind::UaRecovery { state } => {
                use crate::bff::model::{UaRecoveryMode, UaRecoveryState};
                let text = match state {
                    UaRecoveryState::ConnectionLost { status } => {
                        self.cancel_add_children(session);
                        if let Some(view) = self.session_mut(session) {
                            view.context_generation =
                                view.context_generation.and_then(|n| n.checked_add(1));
                            view.clear_tree();
                            view.root_loaded = false;
                            view.root_loading = false;
                        }
                        self.selected_tree = self.selected_tree.filter(|(id, _)| *id != session);
                        self.tree_focus = self.tree_focus.filter(|(id, _)| *id != session);
                        if self.dialogs.add_signal.session == Some(session) {
                            self.dialogs.add_signal.open = false;
                            self.dialogs.add_signal.resolving = None;
                        }
                        fmt(lang, Tr::RecoveryLost, &[&format!("0x{status:08X}")])
                    }
                    UaRecoveryState::Reconnected { mode } => {
                        if mode == UaRecoveryMode::NewSession {
                            if let Some(view) = self.session_mut(session) {
                                view.clear_tree();
                                view.root_loaded = false;
                                view.root_loading = false;
                            }
                            self.selected_tree =
                                self.selected_tree.filter(|(id, _)| *id != session);
                            self.tree_focus = self.tree_focus.filter(|(id, _)| *id != session);
                            for row in self.rows.iter_mut().filter(|row| row.session == session) {
                                row.data_type = None;
                                row.canonical_vt = None;
                                row.write_type = None;
                                row.writable = false;
                                row.ua_access_known = false;
                                row.ua_type_id = None;
                            }
                        }
                        tr(
                            lang,
                            match mode {
                                UaRecoveryMode::ReactivatedSession => Tr::RecoveryReactivated,
                                UaRecoveryMode::NewSession => Tr::RecoveryNewSession,
                            },
                        )
                        .to_string()
                    }
                    UaRecoveryState::Reconciled { missing, ambiguous } => {
                        let detail = tr(lang, Tr::RecoveryMissing).to_string();
                        for key in &missing {
                            self.journal.error(format!("{}: {}", detail, key.get()));
                            if let Some(row) = self
                                .row_position(*key)
                                .and_then(|i| self.rows.get_mut(i))
                                .filter(|r| r.session == session)
                            {
                                row.pending = false;
                                row.pending_push = None;
                                row.quality = RowQuality::Error(detail.clone());
                            }
                        }
                        fmt(
                            lang,
                            Tr::RecoveryReconciled,
                            &[
                                &missing.len().to_string(),
                                &tr(lang, if ambiguous { Tr::Yes } else { Tr::No }),
                            ],
                        )
                    }
                    UaRecoveryState::TypeMetadata { known, total } => fmt(
                        lang,
                        Tr::RecoveryMetadata,
                        &[&known.to_string(), &total.to_string()],
                    ),
                    UaRecoveryState::Stopped { status } => {
                        fmt(lang, Tr::RecoveryStopped, &[&format!("0x{status:08X}")])
                    }
                };
                self.status = text.clone();
                self.journal.warn(text);
            }
            EventKind::UaNotificationFacts { evidence, .. } => {
                use crate::bff::model::UaNotificationEvidence;
                let text = match evidence {
                    UaNotificationEvidence::SequenceGap { previous, current } => {
                        fmt(lang, Tr::UaHistoryGap, &[&previous, &current])
                    }
                    UaNotificationEvidence::SequenceUncertain { previous, current } => {
                        fmt(lang, Tr::UaHistoryUncertain, &[&previous, &current])
                    }
                    UaNotificationEvidence::DataOverflow { key, status } => fmt(
                        lang,
                        Tr::UaHistoryOverflow,
                        &[&key, &format!("0x{status:08X}")],
                    ),
                };
                self.status = text.clone();
                self.journal.warn(text);
            }
            EventKind::UaItemMetadata { facts } => {
                for fact in facts {
                    if !self.engine.session(session).is_some_and(|view| {
                        view.items.contains_key(&fact.key)
                            && !view.recovery_blocked.contains(&fact.key)
                    }) {
                        continue;
                    }
                    if let Some(row) = self
                        .row_position(fact.key)
                        .and_then(|i| self.rows.get_mut(i))
                        .filter(|row| row.session == session && !row.pending)
                    {
                        row.data_type = fact.data_type;
                        row.ua_type_id = fact.type_id;
                        row.ua_access_known = fact.user_access_level.is_some();
                        row.writable = fact
                            .user_access_level
                            .is_some_and(|value| value & 0x02 != 0);
                    }
                    if let Some(diagnostic) = fact.diagnostic {
                        self.journal
                            .warn(self.session_backend_text(session, &diagnostic));
                    }
                }
            }
            EventKind::AddItems { results } => {
                let mut seen = HashSet::new();
                let mut duplicates = HashSet::new();
                for (key, _) in &results {
                    if !seen.insert(*key) {
                        duplicates.insert(*key);
                    }
                }
                for key in &duplicates {
                    let message = tr(lang, Tr::AddDuplicateResult).to_string();
                    if let Some(row) = self
                        .row_position(*key)
                        .and_then(|i| self.rows.get_mut(i))
                        .filter(|row| row.session == session)
                    {
                        row.pending = false;
                        row.pending_push = None;
                        row.quality = RowQuality::Error(message.clone());
                        self.journal.error(message);
                    }
                }
                for (key, result) in results {
                    if duplicates.contains(&key) {
                        continue;
                    }
                    match result {
                        Ok(info) => {
                            let early = self
                                .row_position(key)
                                .and_then(|i| self.rows.get_mut(i))
                                .filter(|row| row.session == session)
                                .and_then(|row| row.pending_push.take());
                            self.apply_item_info(key, info);
                            if let Some(value) = early {
                                self.on_data_event(
                                    session,
                                    EventKind::Values {
                                        values: vec![(key, value)],
                                    },
                                    PendingCtx::None,
                                    None,
                                );
                            }
                        }
                        Err(message) => {
                            let message = self.session_backend_text(session, &message);
                            if let Some(row) =
                                self.row_position(key).and_then(|p| self.rows.get_mut(p))
                            {
                                row.pending = false;
                                row.pending_push = None;
                                row.quality = RowQuality::Error(message.clone());
                                let name = row.display_name.clone();
                                self.journal.error_l(Tr::AddFailedName, &[&name, &message]);
                            }
                        }
                    }
                }
            }
            EventKind::Removed { keys } => {
                self.remove_rows(&keys.into_iter().collect());
            }
            EventKind::Browse { node_id, children } => {
                if matches!(&ctx, PendingCtx::PaletteBrowse(expected) if expected != &node_id) {
                    self.status = tr(self.lang(), Tr::PaletteBrowseMismatch).to_string();
                    self.journal.error_l(Tr::PaletteBrowseMismatch, &[]);
                    return;
                }
                // U7: the reply belongs to the tree ITEM that was expanded (the
                // app remembered it per request id), not to the first node
                // that happens to share the NodeId. Unsolicited replies
                // (request id 0) fall back to the first matching node.
                let mut target: Option<u64> = None;
                let mut palette_id_exhausted = false;
                if let Some(view) = self.session_mut(session) {
                    if node_id.is_empty() {
                        replace_roots(view, children);
                        view.root_loaded = true;
                        view.root_loading = false;
                    } else {
                        let item_id = match &ctx {
                            PendingCtx::TreeItem(id) => Some(*id),
                            PendingCtx::PaletteBrowse(expected) if expected == &node_id => {
                                if let Some(existing) =
                                    view.tree.iter().find(|item| item.node.node_id == node_id)
                                {
                                    Some(existing.id)
                                } else if let Some(next) = view.next_tree_id.checked_add(1) {
                                    let id = view.next_tree_id;
                                    view.next_tree_id = next;
                                    view.tree.push(crate::app::TreeItem {
                                        id,
                                        parent: None,
                                        node: BrowseNode {
                                            node_id: node_id.clone(),
                                            name: node_id.clone(),
                                            kind: crate::bff::model::NodeKind::Branch,
                                            has_children: true,
                                        },
                                        depth: 0,
                                        expanded: true,
                                        loaded: false,
                                        loading: false,
                                    });
                                    view.invalidate_tree();
                                    Some(id)
                                } else {
                                    palette_id_exhausted = true;
                                    None
                                }
                            }
                            PendingCtx::PaletteBrowse(_) => None,
                            _ => view
                                .tree
                                .iter()
                                .find(|t| t.node.node_id == node_id)
                                .map(|t| t.id),
                        };
                        if let Some(item_id) = item_id {
                            if insert_children(view, item_id, children) {
                                if let Some(item) = view.tree.iter_mut().find(|t| t.id == item_id) {
                                    item.loaded = true;
                                    item.loading = false;
                                    item.expanded = true;
                                }
                                target = Some(item_id);
                            }
                        }
                    }
                }
                if palette_id_exhausted {
                    self.status = tr(self.lang(), Tr::PaletteBrowseLimit).to_string();
                    self.journal.error_l(Tr::PaletteBrowseLimit, &[]);
                }
                // Drive the recursive add-children job. The job is keyed by the
                // request id it issued, so a manual browse or a stale reply is
                // never consumed as one of its pages.
                if let Some(request_id) = request_id {
                    self.advance_add_children(session, request_id, &node_id, target);
                }
            }
            EventKind::Views { children } => {
                // Seed a synthetic «Представления» root node; browsing it lists
                // the server Views and selecting a View adopts it for browsing.
                if let Some(view) = self.session_mut(session) {
                    view.root_loading = false;
                    let marker_index = match view.tree.iter().position(|t| t.node.is_views_root()) {
                        Some(index) => index,
                        None => {
                            let id = view.next_tree_id;
                            view.next_tree_id += 1;
                            view.invalidate_tree();
                            view.tree.insert(
                                0,
                                TreeItem {
                                    id,
                                    parent: None,
                                    node: BrowseNode {
                                        node_id: BrowseNode::VIEWS_ROOT_ID.to_string(),
                                        name: tr(lang, Tr::ViewsRoot).to_string(),
                                        kind: NodeKind::Branch,
                                        has_children: true,
                                    },
                                    depth: 0,
                                    expanded: false,
                                    loaded: true,
                                    loading: false,
                                },
                            );
                            0
                        }
                    };
                    // U6: replace only the marker's own descendants. The rest of
                    // the tree (other roots and their expanded branches) stays.
                    set_children(view, marker_index, children, true);
                }
            }
            EventKind::PathResolved { targets } => {
                // U14: only the reply to the dialog's own request counts; a
                // reply that belongs to another session/request never reaches
                // here (its pending entry is not this one), and a dialog that
                // was cancelled meanwhile ignores a late answer.
                let PendingCtx::ResolvePath { requested, name } = ctx else {
                    return;
                };
                if self.dialogs.add_signal.resolving != request_id.map(|id| (session, id)) {
                    return;
                }
                self.dialogs.add_signal.resolving = None;
                if let Some(first) = targets.first() {
                    let display = if name.is_empty() { first.clone() } else { name };
                    if self.add_items(session, vec![(first.clone(), display)]) {
                        self.close_add_signal_dialog();
                    } else {
                        self.dialogs.add_signal.error = Some(self.status.clone());
                    }
                } else {
                    self.dialogs.add_signal.error =
                        Some(fmt(self.lang(), Tr::NameNotResolved, &[&requested]));
                }
            }
            EventKind::Values { values } => {
                let protocol = self.session_protocol(session);
                for (key, value) in values {
                    if self
                        .engine
                        .session(session)
                        .is_some_and(|v| v.recovery_blocked.contains(&key))
                    {
                        continue;
                    }
                    let reason_text = value
                        .reason
                        .as_ref()
                        .map(|reason| self.session_backend_text(session, reason));
                    if self.expanded_values.contains(&(key, Vec::new())) {
                        self.detail_revision = self.detail_revision.wrapping_add(1);
                    }
                    if let Some(row) = self.row_position(key).and_then(|p| self.rows.get_mut(p)) {
                        if row.session != session {
                            continue;
                        }
                        if row.pending {
                            if request_id.is_none()
                                && !value
                                    .quality
                                    .waiting_for_initial_data(protocol.unwrap_or(Protocol::OpcUa))
                            {
                                row.pending_push = Some(value);
                            }
                            continue;
                        }
                        if !row.time_selection.select(
                            &row.times,
                            &row.value,
                            row.quality.kind() == QualityKind::Good,
                            &value,
                            Instant::now(),
                        ) {
                            // A terminal Read still completes in Engine; only
                            // the displayed sample is retained here.
                            continue;
                        }
                        // UI4: remember when the value really changed. The
                        // first value of a row (still pending / waiting) is
                        // not a change.
                        if row.is_value_change(&value.value) {
                            row.changed_at = Some(Instant::now());
                        }
                        row.invalidate_display();
                        row.value = value.value;
                        let mut quality = value.quality;
                        if let Some(reason) = reason_text {
                            // The backend's reason for a Bad value, in the
                            // interface language.
                            quality.text = reason.into();
                        }
                        row.quality = RowQuality::Server(quality);
                        row.timestamp = value.timestamp;
                        row.times = value.times;
                        row.pending = false;
                    }
                }
                if !self.rows.is_empty() {
                    self.last_update = Some(Utc::now());
                }
            }
            EventKind::PollOnly { keys } => {
                // P1: the server's subscription limits are reached for these
                // items; the tick keeps reading them while push is active.
                for key in keys {
                    if let Some(row) = self.row_position(key).and_then(|p| self.rows.get_mut(p)) {
                        row.poll_only = true;
                    }
                }
                if let Some(view) = self.session_mut(session) {
                    view.last_poll = Instant::now()
                        .checked_sub(Duration::from_secs(3600))
                        .unwrap_or_else(Instant::now);
                }
            }
            EventKind::SubscriptionState { .. } => {} // Already projected from Engine.
            EventKind::Wrote { key, result } => {
                let name = self
                    .rows
                    .iter()
                    .find(|r| r.key == key)
                    .map(|r| r.display_name.clone())
                    .unwrap_or_else(|| key.to_string());
                self.write_batch
                    .completed(result.as_ref().is_ok_and(|outcome| outcome.accepted()));
                match result {
                    Ok(outcome) => {
                        let detail = self.session_backend_text(
                            session,
                            &crate::bff::model::BackendMsg::raw(
                                opclook_locale::write_outcome_text(lang, &outcome),
                            ),
                        );
                        let text = fmt(lang, Tr::WriteOutcomeForItem, &[&name, &detail]);
                        if outcome.accepted() {
                            self.journal.info(text);
                        } else if outcome.disposition
                            == crate::bff::model::WriteDisposition::Rejected
                        {
                            self.journal.error_l(Tr::WriteFailed, &[&name, &detail]);
                            self.dialogs.message = Some(MessageDialog {
                                title: tr(lang, Tr::WriteError).to_string(),
                                text,
                                details: detail,
                            });
                        } else {
                            self.journal.warn(text);
                        }
                    }
                    Err(message) => {
                        let message = self.session_backend_text(session, &message);
                        let detail = format!("{}: {}", tr(lang, Tr::WriteNotSubmitted), message);
                        self.journal.error_l(Tr::WriteFailed, &[&name, &detail]);
                        self.dialogs.message = Some(MessageDialog {
                            title: tr(lang, Tr::WriteError).to_string(),
                            text: fmt(lang, Tr::WriteNotSubmittedName, &[&name]),
                            details: detail,
                        });
                    }
                }
                self.finish_write_batch_if_done();
            }
            _ => {}
        }
    }

    fn on_operation_error(
        &mut self,
        session: SessionId,
        operation: Operation,
        message: crate::bff::model::BackendMsg,
        ctx: PendingCtx,
        request_id: Option<RequestId>,
    ) {
        let lang = self.lang();
        let message = self.session_backend_text(session, &message);
        self.journal
            .error_l(Tr::ErrorOp, &[&operation_label(lang, operation), &message]);
        match operation {
            Operation::Connect => {
                self.release_da_routes_for_session(session);
                self.mark_link_lost(session, Some(&message));
                if self.dialogs.connect.connecting == Some(session) {
                    // The failed attempt was started from the dialog:
                    // show the reason there and do not keep retrying in
                    // the background (the user decides: edit, retry or
                    // remove the half-open tab).
                    self.dialogs.connect.connecting = None;
                    self.dialogs.connect.error = Some(message.clone());
                }
                self.log_reconnect(session);
            }
            Operation::Browse => {
                if let Some(view) = self.session_mut(session) {
                    view.root_loading = false;
                    for item in &mut view.tree {
                        item.loading = false;
                    }
                }
            }
            Operation::Read => {}
            Operation::Test => {
                self.mark_link_lost(session, Some(&message));
                self.log_reconnect(session);
            }
            // Discovery operations belong to Owner::Discovery, never a session.
            Operation::SearchServers | Operation::SearchNetwork => {}
            // Operations with no extra UI state transition beyond the
            // journal line. Listed explicitly so a new variant forces a
            // decision here.
            Operation::ResolveBrowsePath => {
                // U14: the error belongs in the (still open) dialog and
                // frees its pending state.
                if request_id.is_some()
                    && self.dialogs.add_signal.resolving == request_id.map(|id| (session, id))
                {
                    self.dialogs.add_signal.resolving = None;
                    self.dialogs.add_signal.error = Some(message);
                }
            }
            Operation::AddItems => {
                // The whole command failed (or never got an answer):
                // its rows must not wait for values forever.
                if let PendingCtx::AddKeys(keys) = &ctx {
                    for key in keys {
                        if let Some(row) =
                            self.row_position(*key).and_then(|p| self.rows.get_mut(p))
                        {
                            row.pending = false;
                            row.pending_push = None;
                            row.quality = RowQuality::Error(message.clone());
                        }
                    }
                }
            }
            Operation::Disconnect
            | Operation::BrowseViews
            | Operation::Write
            | Operation::RemoveItems
            | Operation::Add
            | Operation::Remove
            | Operation::Subscription
            | Operation::UaRuntime => {}
            Operation::Discover => {
                // D4: only the host whose request failed gives its
                // temporary route back; other searches keep theirs.
                match &ctx {
                    PendingCtx::Host(host) => self.release_da_discovery_route(host),
                    _ => self.engine.discard_discovery_operation(Operation::Discover),
                }
                let d = &mut self.dialogs.connect;
                if d.da_auto_pending > 0 {
                    d.da_auto_pending -= 1;
                }
                if d.da_auto_pending == 0 {
                    d.da_pending = false;
                }
                if d.open {
                    d.error = Some(message);
                }
            }
        }
    }

    fn on_discovery_event(
        &mut self,
        kind: EventKind,
        ctx: PendingCtx,
        request_id: Option<RequestId>,
    ) {
        let lang = self.lang();
        match kind {
            EventKind::UaServersFound {
                servers,
                host: _,
                port: _,
            } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let Some(request) = request_id else {
                    return;
                };
                if self.ua_auto_step_done(request, |d| merge_ua_servers(d, servers.clone())) {
                    return;
                }
                if !matches!(self.dialogs.connect.ua_search, UaSearch::SingleHost { request: active } if active == request)
                {
                    return;
                }
                self.dialogs.connect.ua_search = UaSearch::Idle;
                self.dialogs.connect.invalidate_ua_results();
                self.dialogs.connect.ua_search_results.clone_from(&servers);
                refresh_ua_combos(&mut self.dialogs.connect);
                self.journal
                    .info_l(Tr::ServersFoundUa, &[&servers.len().to_string()]);
            }
            EventKind::UaNetworkFound { servers } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let Some(request) = request_id else {
                    return;
                };
                if self.ua_auto_step_done(request, |d| d.ua_network_results.extend(servers.clone()))
                {
                    return;
                }
                if !matches!(self.dialogs.connect.ua_search, UaSearch::Network { request: active } if active == request)
                {
                    return;
                }
                self.dialogs.connect.ua_search = UaSearch::Idle;
                let count = servers.len();
                self.dialogs.connect.ua_network_results = servers;
                // The standalone LDS/mDNS action must publish its results to
                // the list that the popup actually renders.
                super::search::merge_ua_results(&mut self.dialogs.connect);
                if count == 0 {
                    self.dialogs.connect.error =
                        Some(tr(lang, Tr::NetworkServersNotFound).to_string());
                } else {
                    self.journal
                        .info_l(Tr::ServersFoundUa, &[&count.to_string()]);
                }
            }
            EventKind::UaScanProgress {
                scan_id,
                total,
                checked,
                found,
            } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let d = &mut self.dialogs.connect;
                if d.ua_search.scan_id() != Some(scan_id) {
                    return;
                }
                d.ua_scan_total = total;
                d.ua_scan_checked = checked;
                d.ua_scan_found = found;
            }
            EventKind::UaScanFound { scan_id, servers } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let d = &mut self.dialogs.connect;
                if d.ua_search.scan_id() != Some(scan_id) {
                    return;
                }
                d.ua_scan_servers.extend(servers);
                // Show scan results as they arrive, not only at completion.
                merge_ua_scan_servers(d);
            }
            EventKind::UaScanFinished {
                scan_id,
                found,
                end,
            } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let d = &mut self.dialogs.connect;
                if d.ua_search.scan_id() != Some(scan_id) {
                    return;
                }
                d.ua_search = UaSearch::Idle;
                d.ua_scan_found = found;
                if matches!(end, ScanEnd::Completed) {
                    d.ua_scan_checked = d.ua_scan_total;
                }
                merge_ua_scan_servers(d);
                match end {
                    ScanEnd::Failed(error) => {
                        d.ua_scan_error =
                            Some(localize_scan_error(lang, &backend_text(lang, &error)))
                    }
                    ScanEnd::Cancelled => {
                        d.ua_scan_error = None;
                        self.journal
                            .info_l(Tr::ServersFoundUa, &[&found.to_string()]);
                    }
                    ScanEnd::Completed if found == 0 => {
                        d.ua_scan_error = Some(tr(lang, Tr::UaScanNoResults).to_string())
                    }
                    ScanEnd::Completed => {
                        d.ua_scan_error = None;
                        self.journal
                            .info_l(Tr::ServersFoundUa, &[&found.to_string()]);
                    }
                }
            }
            EventKind::UaEndpointsFound { endpoints, .. } => {
                if self.dialogs.connect.open {
                    self.dialogs.connect.ua_discovered_endpoints = endpoints;
                }
            }
            EventKind::Servers { host, servers } => {
                self.release_da_discovery_route(&host);
                if self.dialogs.connect.open {
                    if self.dialogs.connect.da_auto_pending > 0 {
                        // A ProgID only identifies a server within a computer.
                        // The host must survive discovery and row selection.
                        for server in servers {
                            let duplicate = self
                                .dialogs
                                .connect
                                .da_servers
                                .iter()
                                .any(|hit| hit.server.id == server.id && hit.host == host);
                            if !duplicate {
                                self.dialogs.connect.da_servers.push(super::dialogs::DaHit {
                                    server,
                                    host: host.clone(),
                                });
                            }
                        }
                        self.dialogs.connect.da_auto_pending =
                            self.dialogs.connect.da_auto_pending.saturating_sub(1);
                        if self.dialogs.connect.da_auto_pending == 0 {
                            self.dialogs.connect.da_pending = false;
                        }
                        if self.dialogs.connect.da_selected.is_none() {
                            self.dialogs.connect.da_selected =
                                self.dialogs.connect.da_servers.first().cloned();
                        }
                        self.journal.info_l(
                            Tr::ServersFoundDa,
                            &[&self.dialogs.connect.da_servers.len().to_string()],
                        );
                        return;
                    }
                    self.dialogs.connect.da_pending = false;
                    self.dialogs.connect.da_host.clone_from(&host);
                    self.dialogs.connect.da_servers = servers
                        .into_iter()
                        .map(|server| super::dialogs::DaHit {
                            server,
                            host: host.clone(),
                        })
                        .collect();
                    self.dialogs.connect.da_selected =
                        self.dialogs.connect.da_servers.first().cloned();
                    self.journal.info_l(
                        Tr::ServersFoundDa,
                        &[&self.dialogs.connect.da_servers.len().to_string()],
                    );
                }
            }
            EventKind::Log { message } => self.journal.info(backend_text(lang, &message)),
            EventKind::Error { operation, message } => {
                let message = backend_text(lang, &message);
                self.journal
                    .error_l(Tr::ErrorOp, &[&operation_label(lang, operation), &message]);
                match operation {
                    Operation::SearchServers | Operation::SearchNetwork => {
                        if self.dialogs.connect.open {
                            if let Some(request) = request_id {
                                let auto = self.ua_auto_step_done(request, |_| {});
                                let active = matches!(self.dialogs.connect.ua_search,
                                    UaSearch::SingleHost { request: id } | UaSearch::Network { request: id } if id == request);
                                if active {
                                    self.dialogs.connect.ua_search = UaSearch::Idle;
                                }
                                if auto || active {
                                    self.dialogs.connect.error = Some(message);
                                }
                            }
                        }
                    }
                    Operation::Discover => {
                        match &ctx {
                            PendingCtx::Host(host) => self.release_da_discovery_route(host),
                            _ => self.engine.discard_discovery_operation(Operation::Discover),
                        }
                        let d = &mut self.dialogs.connect;
                        d.da_auto_pending = d.da_auto_pending.saturating_sub(1);
                        if d.da_auto_pending == 0 {
                            d.da_pending = false;
                        }
                        if d.open {
                            d.error = Some(message);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Batch completed -> publish a compact summary in the status bar.
    pub(super) fn finish_write_batch_if_done(&mut self) {
        if self.write_batch.pending == 0 && self.write_batch.total > 0 {
            let lang = self.lang();
            let counts = [
                self.write_batch.ok.to_string(),
                self.write_batch.total.to_string(),
            ];
            self.write_batch.summary =
                Some(fmt(lang, Tr::WrittenSummary, &[&counts[0], &counts[1]]));
            self.journal
                .info_l(Tr::WriteDoneSummary, &[&counts[0], &counts[1]]);
        }
    }

    fn apply_item_info(&mut self, key: ItemKey, info: ItemInfo) {
        let waiting_value = tr(self.lang(), Tr::WaitingValue).to_string();
        if let Some(row) = self.row_position(key).and_then(|p| self.rows.get_mut(p)) {
            row.data_type = info.data_type;
            // For OPC UA the type_id holds a UA built-in type code, not a DA VT,
            // so keep canonical_vt (VT) only for DA sessions.
            let is_ua = self
                .sessions
                .iter()
                .find(|s| s.id == row.session)
                .map(|s| s.protocol() == Protocol::OpcUa)
                .unwrap_or(false);
            if !is_ua {
                row.canonical_vt = info.type_id.map(crate::bff::model::VarType);
            } else {
                row.ua_type_id = info.type_id;
            }
            row.ua_access_known = !is_ua || info.writable;
            row.writable = info.writable;
            row.pending = false;
            if matches!(row.quality, RowQuality::NoData(_)) {
                row.quality = RowQuality::Waiting(waiting_value);
            }
        }
    }
}
