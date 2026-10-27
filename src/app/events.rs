use super::present::{compose_server_name, localize_scan_error, operation_label};
use super::search::{merge_ua_scan_servers, merge_ua_servers, refresh_ua_combos};
use super::tree::{insert_children, replace_roots, set_children};
use super::*;
use crate::bff::model::{
    BrowseNode, Event, EventKind, ItemInfo, ItemKey, NodeKind, Operation, Protocol, Quality,
    QualityKind, RequestId, SessionId, SubscriptionState,
};
use crate::i18n::{Tr, backend_text, fmt, tr};
use chrono::Utc;
use std::time::{Duration, Instant};

impl App {
    pub(super) fn poll_events(&mut self) {
        while let Some(event) = self.gateway.try_recv() {
            self.on_event(event);
        }
    }

    /// Test-only hook to feed an event directly into the app state.
    pub fn on_event_for_test(&mut self, kind: EventKind) {
        self.on_event(Event {
            request_id: 0,
            session: 0,
            kind,
        });
    }

    /// Test-only hook to feed an event for a specific session into the app state.
    pub fn on_event_for_test_at(&mut self, session: SessionId, kind: EventKind) {
        self.on_event(Event {
            request_id: 0,
            session,
            kind,
        });
    }

    /// Test-only hook to feed an event with an explicit request id (used to
    /// exercise stale-reply rejection and request-id correlation).
    pub fn on_event_for_test_with_request(
        &mut self,
        session: SessionId,
        request_id: RequestId,
        kind: EventKind,
    ) {
        self.on_event(Event {
            request_id,
            session,
            kind,
        });
    }

    /// Test-only hook to run the per-frame update loop (browse/read/reconnect
    /// scheduling) without touching the UI.
    pub fn tick_for_test(&mut self) {
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
        let session = event.session;
        // Request-id correlation: a non-zero reply is accepted only once, and
        // only while its command is still outstanding. Superseded/session-old
        // replies are dropped before they can touch any state. `0` is reserved
        // for unsolicited pushes and worker runtime errors.
        let mut ctx = PendingCtx::None;
        if event.request_id != 0 {
            match self.pending_requests.remove(&(session, event.request_id)) {
                Some(pending) => {
                    if self.settings.options.diagnostic_trace {
                        let latency = pending.sent.elapsed().as_millis();
                        self.journal.info_l(
                            Tr::TraceReply,
                            &[
                                &event.request_id.to_string(),
                                &operation_label(self.lang(), pending.operation),
                                &session.to_string(),
                                &latency.to_string(),
                            ],
                        );
                    }
                    ctx = pending.ctx;
                }
                None => {
                    if self.settings.options.diagnostic_trace {
                        self.journal.info_l(
                            Tr::TraceDropped,
                            &[&event.request_id.to_string(), &session.to_string()],
                        );
                    }
                    return;
                }
            }
        }
        let request_id = event.request_id;
        let lang = self.lang();
        match event.kind {
            EventKind::UaServersFound {
                servers,
                host: _,
                port: _,
            } => {
                if !self.dialogs.connect.open {
                    return;
                }
                if self.dialogs.connect.ua_auto_pending > 0 {
                    // A combined auto-search is running: merge this host's
                    // result right away (127.0.0.1 and localhost usually return
                    // the same server; the visible tree must not show it twice
                    // while the remaining sub-searches are still running).
                    self.ua_auto_step_done(|d| {
                        merge_ua_servers(d, servers);
                    });
                    return;
                }
                self.dialogs.connect.ua_searching = false;
                self.dialogs.connect.ua_search_results = servers.clone();
                refresh_ua_combos(&mut self.dialogs.connect);
                self.journal
                    .info_l(Tr::ServersFoundUa, &[&servers.len().to_string()]);
            }
            EventKind::UaNetworkFound { servers } => {
                if !self.dialogs.connect.open {
                    return;
                }
                if self.dialogs.connect.ua_auto_pending > 0 {
                    // Part of the combined auto-search: keep the raw network list
                    // for the final merge.
                    self.ua_auto_step_done(|d| {
                        d.ua_network_results.extend(servers);
                    });
                    return;
                }
                self.dialogs.connect.ua_network_searching = false;
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
                if d.ua_scan_id != Some(scan_id) {
                    return;
                }
                d.ua_subnet_searching = true;
                d.ua_scan_total = total;
                d.ua_scan_checked = checked;
                d.ua_scan_found = found;
            }
            EventKind::UaScanFound { scan_id, servers } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let d = &mut self.dialogs.connect;
                if d.ua_scan_id != Some(scan_id) {
                    return;
                }
                d.ua_scan_servers.extend(servers);
                // Show scan results as they arrive, not only at completion.
                merge_ua_scan_servers(d);
            }
            EventKind::UaScanFinished {
                scan_id,
                cancelled,
                found,
                error,
            } => {
                if !self.dialogs.connect.open {
                    return;
                }
                let d = &mut self.dialogs.connect;
                if d.ua_scan_id != Some(scan_id) {
                    return;
                }
                d.ua_scan_id = None;
                d.ua_subnet_searching = false;
                d.ua_scan_found = found;
                d.ua_scan_checked = d.ua_scan_total;
                d.ua_scan_confirm = None;
                merge_ua_scan_servers(d);
                if let Some(error) = error {
                    d.ua_scan_error = Some(localize_scan_error(lang, &error));
                } else if cancelled {
                    d.ua_scan_error = None;
                    self.journal
                        .info_l(Tr::ServersFoundUa, &[&found.to_string()]);
                } else if found == 0 {
                    d.ua_scan_error = Some(tr(lang, Tr::UaScanNoResults).to_string());
                } else {
                    d.ua_scan_error = None;
                    self.journal
                        .info_l(Tr::ServersFoundUa, &[&found.to_string()]);
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
                        for s in servers {
                            let duplicate = self
                                .dialogs
                                .connect
                                .da_servers
                                .iter()
                                .zip(&self.dialogs.connect.da_server_hosts)
                                .any(|(existing, existing_host)| {
                                    existing.id == s.id && existing_host == &host
                                });
                            if !duplicate {
                                self.dialogs.connect.da_servers.push(s);
                                self.dialogs.connect.da_server_hosts.push(host.clone());
                            }
                        }
                        self.dialogs.connect.da_auto_pending =
                            self.dialogs.connect.da_auto_pending.saturating_sub(1);
                        if self.dialogs.connect.da_auto_pending == 0 {
                            self.dialogs.connect.da_pending = false;
                        }
                        if self.dialogs.connect.da_selected.is_none() {
                            self.dialogs.connect.da_selected = self
                                .dialogs
                                .connect
                                .da_servers
                                .first()
                                .map(|s| s.id.clone());
                            self.dialogs.connect.da_selected_host =
                                self.dialogs.connect.da_server_hosts.first().cloned();
                        }
                        self.journal.info_l(
                            Tr::ServersFoundDa,
                            &[&self.dialogs.connect.da_servers.len().to_string()],
                        );
                        return;
                    }
                    self.dialogs.connect.da_pending = false;
                    self.dialogs.connect.da_host = host.clone();
                    self.dialogs.connect.da_server_hosts = vec![host.clone(); servers.len()];
                    self.dialogs.connect.da_servers = servers;
                    self.dialogs.connect.da_selected = self
                        .dialogs
                        .connect
                        .da_servers
                        .first()
                        .map(|s| s.id.clone());
                    self.dialogs.connect.da_selected_host =
                        self.dialogs.connect.da_server_hosts.first().cloned();
                    self.journal.info_l(
                        Tr::ServersFoundDa,
                        &[&self.dialogs.connect.da_servers.len().to_string()],
                    );
                }
            }
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
                    view.subscription = SubscriptionState::Inactive;
                    if is_ua {
                        view.ua_session_timeout_ms = endpoint
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
                                row.quality = Quality {
                                    code: 0,
                                    kind: QualityKind::Bad,
                                    text: message.clone(),
                                };
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
                self.advance_add_children(session, request_id, &node_id, target);
            }
            EventKind::Views { children } => {
                // Seed a synthetic «Представления» root node; browsing it lists
                // the server Views and selecting a View adopts it for browsing.
                const VIEWS_MARKER: &str = "\u{1}views";
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.root_loading = false;
                    let marker_index = match view
                        .tree
                        .iter()
                        .position(|t| t.node.node_id == VIEWS_MARKER)
                    {
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
                                        node_id: VIEWS_MARKER.to_string(),
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
                if self.dialogs.add_signal.resolving != Some((session, request_id)) {
                    return;
                }
                self.dialogs.add_signal.resolving = None;
                if let Some(first) = targets.first() {
                    let display = if name.is_empty() { first.clone() } else { name };
                    self.add_items(session, vec![(first.clone(), display)]);
                    self.close_add_signal_dialog();
                } else {
                    self.dialogs.add_signal.error =
                        Some(fmt(self.lang(), Tr::NameNotResolved, &[&requested]));
                }
            }
            EventKind::Values { values } => {
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.read_in_flight = false;
                }
                for (key, value) in values {
                    if let Some(row) = self.row_position(key).and_then(|p| self.rows.get_mut(p)) {
                        // UI4: remember when the value really changed. The
                        // first value of a row (still pending / waiting) is
                        // not a change.
                        if row.is_value_change(&value.value) {
                            row.changed_at = Some(Instant::now());
                        }
                        row.value = value.value;
                        row.quality = value.quality;
                        if let Some(reason) = &value.reason {
                            // The backend's reason for a Bad value, in the
                            // interface language.
                            row.quality.text = backend_text(lang, reason);
                        }
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
            EventKind::SubscriptionState { state } => {
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.subscription = state;
                    if state == SubscriptionState::Active {
                        // The push subscription is live; clear a stale in-flight
                        // poll so the next tick does not issue a redundant Read.
                        view.read_in_flight = false;
                    }
                }
            }
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
                        lost = Some(view.endpoint.display_name());
                    }
                }
                if let Some(name) = lost {
                    // Keep rows and the tree: tags must remain visible with
                    // their last values, just marked stale while the server
                    // is unreachable.
                    self.mark_session_rows_stale(session);
                    self.journal.info_l(Tr::ConnectedInfo, &[&name, &reason]);
                    self.schedule_reconnect(session);
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
                            if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                                view.reconnect = ReconnectState::Stopped;
                            }
                        }
                        self.schedule_reconnect(session);
                    }
                    Operation::Browse => {
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.root_loading = false;
                            for item in &mut view.tree {
                                item.loading = false;
                            }
                        }
                    }
                    Operation::Read => {
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.read_in_flight = false;
                        }
                    }
                    Operation::Test => {
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.state = SessionState::Disconnected;
                            view.info = message;
                            view.read_in_flight = false;
                        }
                        // A3: the link is declared dead, so the last Good values
                        // must not stay on screen as if they were live.
                        self.mark_session_rows_stale(session);
                        self.schedule_reconnect(session);
                    }
                    Operation::SearchServers => {
                        if self.dialogs.connect.open {
                            if self.dialogs.connect.ua_auto_pending > 0 {
                                // A combined auto-search is running: don't finish
                                // it early, just fold this (failed) host step in.
                                self.ua_auto_step_done(|_| {});
                            } else {
                                self.dialogs.connect.ua_searching = false;
                                self.dialogs.connect.error = Some(message);
                            }
                        }
                    }
                    Operation::SearchNetwork => {
                        if self.dialogs.connect.open {
                            if self.dialogs.connect.ua_auto_pending > 0 {
                                self.ua_auto_step_done(|_| {});
                            } else {
                                self.dialogs.connect.ua_network_searching = false;
                                self.dialogs.connect.error = Some(message);
                            }
                        }
                    }
                    // Operations with no extra UI state transition beyond the
                    // journal line. Listed explicitly so a new variant forces a
                    // decision here.
                    Operation::ResolveBrowsePath => {
                        // U14: the error belongs in the (still open) dialog and
                        // frees its pending state.
                        if self.dialogs.add_signal.resolving == Some((session, request_id)) {
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
                                    row.quality = Quality {
                                        code: 0,
                                        kind: QualityKind::Bad,
                                        text: message.clone(),
                                    };
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
                            _ => self.release_da_discovery_routes(),
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
                row.canonical_vt = info.type_id;
            }
            row.writable = info.writable;
            row.pending = false;
            if row.quality.is_placeholder_no_data() {
                row.quality = Quality {
                    code: Quality::CODE_PLACEHOLDER_WAITING,
                    kind: QualityKind::Good,
                    text: waiting_value,
                };
            }
        }
    }
}
