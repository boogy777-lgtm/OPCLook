use super::present::{compose_server_name, localize_scan_error, operation_label};
use super::search::{merge_ua_scan_servers, refresh_ua_combos};
use super::tree::{insert_children, replace_roots};
use super::*;
use crate::bff::model::{
    BrowseNode, Event, EventKind, ItemInfo, ItemKey, NodeKind, Operation, Protocol, Quality,
    QualityKind, RequestId, SessionId, SubscriptionState,
};
use crate::i18n::{Tr, fmt, tr};
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

    /// Test-only: make a session the active one (for reconnect-hint lookups).
    pub fn activate_session(&mut self, session: SessionId) {
        self.active_session = Some(session);
    }

    fn on_event(&mut self, event: Event) {
        let session = event.session;
        // Request-id correlation: a non-zero reply is accepted only once, and
        // only while its command is still outstanding. Superseded/session-old
        // replies are dropped before they can touch any state. `0` is reserved
        // for unsolicited pushes and worker runtime errors.
        if event.request_id != 0 && !self.pending_requests.remove(&(session, event.request_id)) {
            return;
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
                    // A combined auto-search is running: accumulate this host's
                    // result; merge once every sub-search has replied.
                    self.ua_auto_step_done(|d| {
                        d.ua_search_results.extend(servers);
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
                if self.dialogs.connect.open {
                    self.dialogs.connect.open = false;
                }
            }
            EventKind::AddItems { results } => {
                for (key, result) in results {
                    match result {
                        Ok(info) => self.apply_item_info(key, info),
                        Err(message) => {
                            if let Some(row) = self.rows.iter_mut().find(|r| r.key == key) {
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
                for key in keys {
                    self.rows.retain(|row| row.key != key);
                }
            }
            EventKind::Browse { node_id, children } => {
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    if node_id.is_empty() {
                        replace_roots(view, children.clone());
                        view.root_loaded = true;
                        view.root_loading = false;
                    } else {
                        insert_children(view, &node_id, children.clone());
                        for item in &mut view.tree {
                            if item.node.node_id == node_id {
                                item.loaded = true;
                                item.loading = false;
                                item.expanded = true;
                            }
                        }
                    }
                }
                // Drive the recursive add-children job. The job is keyed by the
                // request id it issued, so a manual browse or a stale reply is
                // never consumed as one of its pages.
                self.advance_add_children(session, request_id, &node_id);
            }
            EventKind::Views { children } => {
                // Seed a synthetic «Представления» root node; browsing it lists
                // the server Views and selecting a View adopts it for browsing.
                const VIEWS_MARKER: &str = "\u{1}views";
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.root_loading = false;
                    if !view.tree.iter().any(|t| t.node.node_id == VIEWS_MARKER) {
                        view.tree.insert(
                            0,
                            TreeItem {
                                id: view.next_tree_id,
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
                        view.next_tree_id += 1;
                    }
                    // Replace any previously listed child Views.
                    let (_i, marker) = view
                        .tree
                        .iter()
                        .enumerate()
                        .find(|(_, t)| t.node.node_id == VIEWS_MARKER)
                        .expect("views marker seeded above");
                    let parent_id = marker.id;
                    let marker_depth = marker.depth;
                    let old_ids: Vec<u64> = view
                        .tree
                        .iter()
                        .filter(|t| {
                            t.parent == Some(parent_id)
                                || (t.node.node_id != VIEWS_MARKER && t.depth > marker_depth)
                        })
                        .map(|t| t.id)
                        .collect();
                    view.tree.retain(|t| !old_ids.contains(&t.id));
                    let parent_index = view
                        .tree
                        .iter()
                        .position(|t| t.node.node_id == VIEWS_MARKER)
                        .expect("marker survives retain");
                    let mut id = view.next_tree_id;
                    let depth = marker_depth + 1;
                    let new_items: Vec<TreeItem> = children
                        .iter()
                        .map(|node| {
                            let tree_item = TreeItem {
                                id,
                                parent: Some(parent_id),
                                node: node.clone(),
                                depth,
                                expanded: false,
                                loaded: true,
                                loading: false,
                            };
                            id += 1;
                            tree_item
                        })
                        .collect();
                    view.next_tree_id = id;
                    view.tree
                        .splice(parent_index + 1..parent_index + 1, new_items);
                }
            }
            EventKind::PathResolved { targets } => {
                if let Some((pending_session, requested, name)) = self.pending_path_resolve.take() {
                    if pending_session != session {
                        // A resolve from a different session: re-queue is not
                        // needed; just report the failure on the original dialog.
                        self.dialogs.add_signal.error =
                            Some(fmt(self.lang(), Tr::NameNotResolved, &[&requested]));
                        return;
                    }
                    if let Some(first) = targets.first() {
                        let display = if name.is_empty() { first.clone() } else { name };
                        self.add_items(session, vec![(first.clone(), display)]);
                    } else {
                        self.dialogs.add_signal.error =
                            Some(fmt(self.lang(), Tr::NameNotResolved, &[&requested]));
                    }
                }
            }
            EventKind::Values { values } => {
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.read_in_flight = false;
                }
                for (key, value) in values {
                    if let Some(row) = self.rows.iter_mut().find(|r| r.key == key) {
                        row.value = value.value;
                        row.quality = value.quality;
                        row.timestamp = value.timestamp;
                        row.pending = false;
                    }
                }
                if !self.rows.is_empty() {
                    self.last_update = Some(Utc::now());
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
                        self.write_err += 1;
                        self.journal.error_l(Tr::WriteFailed, &[&name, &message]);
                        self.dialogs.message = Some(MessageDialog {
                            title: tr(lang, Tr::WriteError).to_string(),
                            text: fmt(lang, Tr::WriteFailedName, &[&name]),
                            details: message,
                        });
                    }
                }
                // Batch completed -> publish a compact summary in the status bar.
                if self.pending_writes == 0 && self.write_total > 0 {
                    self.last_write_summary = Some(fmt(
                        lang,
                        Tr::WrittenSummary,
                        &[&self.write_ok.to_string(), &self.write_total.to_string()],
                    ));
                    self.journal.info_l(
                        Tr::WriteDoneSummary,
                        &[&self.write_ok.to_string(), &self.write_total.to_string()],
                    );
                }
            }
            EventKind::Disconnected { reason } => {
                self.cancel_add_children(session);
                if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                    view.read_in_flight = false;
                    if view.state != SessionState::Connecting {
                        view.state = SessionState::Disconnected;
                        view.info = reason.clone();
                        // Keep rows and the tree: tags must remain visible with
                        // their last values, just marked stale while the server
                        // is unreachable.
                        for row in self.rows.iter_mut().filter(|r| r.session == session) {
                            row.quality = Quality {
                                code: 0,
                                kind: QualityKind::Bad,
                                text: tr(lang, Tr::StaleData).to_string(),
                            };
                        }
                        self.journal
                            .info_l(Tr::ConnectedInfo, &[&view.endpoint.display_name(), &reason]);
                        self.schedule_reconnect(session);
                    }
                }
            }
            EventKind::Log { message } => {
                self.journal.info(message);
            }
            EventKind::Error { operation, message } => {
                // Security-resolution failures carry a stable technical detail
                // marker; wrap them in the localized message while keeping the
                // server detail. Other errors pass through unchanged.
                let message = match crate::bff::ua::security_error_detail(&message) {
                    Some(detail) => fmt(lang, Tr::SecurityResolveFailed, &[&detail]),
                    None => message,
                };
                self.journal
                    .error_l(Tr::ErrorOp, &[&operation_label(lang, operation), &message]);
                match operation {
                    Operation::Connect => {
                        self.release_da_routes_for_session(session);
                        if let Some(view) = self.sessions.iter_mut().find(|s| s.id == session) {
                            view.state = SessionState::Disconnected;
                            view.info = message.clone();
                        }
                        if self.dialogs.connect.open {
                            self.dialogs.connect.error = Some(message.clone());
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
                    Operation::Disconnect
                    | Operation::BrowseViews
                    | Operation::ResolveBrowsePath
                    | Operation::Write
                    | Operation::AddItems
                    | Operation::RemoveItems
                    | Operation::Add
                    | Operation::Remove
                    | Operation::Subscription
                    | Operation::UaRuntime => {}
                    Operation::Discover => {
                        self.release_da_discovery_routes();
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

    fn apply_item_info(&mut self, key: ItemKey, info: ItemInfo) {
        let waiting_value = tr(self.lang(), Tr::WaitingValue).to_string();
        if let Some(row) = self.rows.iter_mut().find(|r| r.key == key) {
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
