use super::present::{compose_server_name, localize_scan_error, operation_label};
use super::search::{merge_ua_scan_servers, merge_ua_servers, refresh_ua_combos};
use super::tree::{insert_children, replace_roots, set_children};
use super::*;
#[cfg(feature = "test-support")]
use crate::bff::model::RequestId;
use crate::bff::model::{
    BrowseNode, Event, EventKind, ItemInfo, ItemKey, NodeKind, Operation, Origin, Owner, Protocol,
    ScanEnd, SessionId,
};
use crate::i18n::{Tr, backend_text, fmt, tr};
use chrono::Utc;
use std::time::{Duration, Instant};

impl App {
    pub(super) fn poll_events(&mut self) {
        while let Some(event) = self.engine.try_recv() {
            self.on_event(event);
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
            origin: Origin::Unsolicited,
            owner: Owner::Session(session),
            kind,
        });
    }

    /// Test-only hook to feed an event for a specific session into the app state.
    #[cfg(feature = "test-support")]
    pub fn on_event_for_test_at(&mut self, session: SessionId, kind: EventKind) {
        self.sync_fixture_sessions();
        self.on_event(Event {
            origin: Origin::Unsolicited,
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
            .set_auto_reconnect(self.settings.options.auto_reconnect);
        let ctx = match self.engine.accept(&event) {
            Ok(Some(pending)) => {
                if self.settings.options.diagnostic_trace {
                    let latency = pending.sent.elapsed().as_millis();
                    self.journal.info_l(
                        Tr::TraceReply,
                        &[
                            &match event.origin {
                                Origin::Reply(id) => id.to_string(),
                                Origin::Unsolicited => String::new(),
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
                if self.settings.options.diagnostic_trace {
                    self.journal.info_l(
                        Tr::TraceDropped,
                        &[
                            &match event.origin {
                                Origin::Reply(id) => id.to_string(),
                                Origin::Unsolicited => String::new(),
                            },
                            &owner_label(self.lang(), owner),
                        ],
                    );
                }
                return;
            }
        };
        let request_id = match event.origin {
            Origin::Reply(id) => Some(id),
            Origin::Unsolicited => None,
        };
        let lang = self.lang();
        let kind = match (event.kind, &ctx) {
            (
                EventKind::Error {
                    operation: Operation::Write,
                    message,
                },
                PendingCtx::WriteKey(key),
            ) => EventKind::Wrote {
                key: *key,
                result: Err(message),
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
        match kind {
            EventKind::UaServersFound { .. }
            | EventKind::UaNetworkFound { .. }
            | EventKind::UaScanProgress { .. }
            | EventKind::UaScanFound { .. }
            | EventKind::UaScanFinished { .. }
            | EventKind::UaEndpointsFound { .. }
            | EventKind::Servers { .. } => {}
            EventKind::Connected { endpoint, info } => {
                let info = backend_text(lang, &info);
                let is_ua = self
                    .sessions
                    .iter()
                    .find(|s| s.id == session)
                    .map(|s| s.protocol() == Protocol::OpcUa)
                    .unwrap_or(false);
                // Any other reply from the old link is now superseded.
                self.clear_pending_requests(session);
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.state = SessionState::Connected;
                    view.info = info.clone();
                    view.display_name = compose_server_name(&endpoint);
                    view.reconnect = ReconnectState::Idle;
                    view.root_loaded = false;
                    view.root_loading = true;
                    view.read_in_flight = false;
                    view.tree.clear();
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
            EventKind::AddItems { results } => {
                for (key, result) in results {
                    match result {
                        Ok(info) => self.apply_item_info(key, info),
                        Err(message) => {
                            let message = backend_text(lang, &message);
                            if let Some(row) =
                                self.row_position(key).and_then(|p| self.rows.get_mut(p))
                            {
                                row.pending = false;
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
                // U7: the reply belongs to the tree ITEM that was expanded (the
                // app remembered it per request id), not to the first node
                // that happens to share the NodeId. Unsolicited replies
                // (request id 0) fall back to the first matching node.
                let mut target: Option<u64> = None;
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    if node_id.is_empty() {
                        replace_roots(view, children);
                        view.root_loaded = true;
                        view.root_loading = false;
                    } else {
                        let item_id = match &ctx {
                            PendingCtx::TreeItem(id) => Some(*id),
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
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.root_loading = false;
                    let marker_index = match view.tree.iter().position(|t| t.node.is_views_root()) {
                        Some(index) => index,
                        None => {
                            let id = view.next_tree_id;
                            view.next_tree_id += 1;
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
                for (key, value) in values {
                    if let Some(row) = self.row_position(key).and_then(|p| self.rows.get_mut(p)) {
                        // UI4: remember when the value really changed. The
                        // first value of a row (still pending / waiting) is
                        // not a change.
                        if row.is_value_change(&value.value) {
                            row.changed_at = Some(Instant::now());
                        }
                        row.value = value.value;
                        let mut quality = value.quality;
                        if let Some(reason) = &value.reason {
                            // The backend's reason for a Bad value, in the
                            // interface language.
                            quality.text = backend_text(lang, reason);
                        }
                        row.quality = RowQuality::Server(quality);
                        row.timestamp = value.timestamp;
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
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    // Read them right away, not one interval later.
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
                if self.pending_writes > 0 {
                    self.pending_writes -= 1;
                }
                match result {
                    Ok(()) => {
                        self.write_ok += 1;
                        self.journal.info_l(Tr::WrittenValue, &[&name]);
                    }
                    Err(message) => {
                        let message = backend_text(lang, &message);
                        self.write_err += 1;
                        self.journal.error_l(Tr::WriteFailed, &[&name, &message]);
                        self.dialogs.message = Some(MessageDialog {
                            title: tr(lang, Tr::WriteError).to_string(),
                            text: fmt(lang, Tr::WriteFailedName, &[&name]),
                            details: message,
                        });
                    }
                }
                self.finish_write_batch_if_done();
            }
            EventKind::Disconnected { reason } => {
                let reason = backend_text(lang, &reason);
                self.cancel_add_children(session);
                let mut lost: Option<String> = None;
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.read_in_flight = false;
                    if view.state != SessionState::Connecting {
                        view.state = SessionState::Disconnected;
                        view.info = reason.clone();
                        lost = Some(view.endpoint().display_name());
                    }
                }
                if let Some(name) = lost {
                    self.clear_pending_requests(session);
                    // Keep rows and the tree: tags must remain visible with
                    // their last values, just marked stale while the server
                    // is unreachable.
                    self.mark_session_rows_stale(session);
                    self.journal.info_l(Tr::ConnectedInfo, &[&name, &reason]);
                    self.log_reconnect(session);
                }
            }
            EventKind::Log { message } => {
                self.journal.info(backend_text(lang, &message));
            }
            EventKind::Error { operation, message } => {
                let message = backend_text(lang, &message);
                self.journal
                    .error_l(Tr::ErrorOp, &[&operation_label(lang, operation), &message]);
                match operation {
                    Operation::Connect => {
                        self.release_da_routes_for_session(session);
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.state = SessionState::Disconnected;
                            view.info = message.clone();
                        }
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
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.root_loading = false;
                            for item in &mut view.tree {
                                item.loading = false;
                            }
                        }
                    }
                    Operation::Read => {}
                    Operation::Test => {
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.state = SessionState::Disconnected;
                            view.info = message;
                            view.read_in_flight = false;
                        }
                        // A3: the link is declared dead, so the last Good values
                        // must not stay on screen as if they were live.
                        self.mark_session_rows_stale(session);
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
                            && self.dialogs.add_signal.resolving
                                == request_id.map(|id| (session, id))
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
                self.dialogs.connect.ua_search_results = servers.clone();
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
                    self.dialogs.connect.da_host = host.clone();
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
        if self.pending_writes == 0 && self.write_total > 0 {
            let lang = self.lang();
            let counts = [self.write_ok.to_string(), self.write_total.to_string()];
            self.last_write_summary =
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
            }
            row.writable = info.writable;
            row.pending = false;
            if matches!(row.quality, RowQuality::NoData(_)) {
                row.quality = RowQuality::Waiting(waiting_value);
            }
        }
    }
}
