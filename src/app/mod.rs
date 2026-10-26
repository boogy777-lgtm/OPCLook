use crate::bff::Gateway;
use crate::bff::model::{
    BrowseNode, Command, CommandKind, EndpointSpec, ItemKey, Protocol, Quality, QualityKind,
    RequestId, SessionId, SubscriptionState, Value,
};
use crate::bff::net::NetworkBinding;
use crate::constants::{
    OPERATION_TIMEOUT_MAX_SECS, OPERATION_TIMEOUT_MIN_SECS, TEST_INTERVAL_MAX_SECS,
    TEST_INTERVAL_MIN_SECS, UPDATE_INTERVAL_MAX_SECS, UPDATE_INTERVAL_MIN_SECS,
};
use crate::da_route::{DaRouteError, DaRouteManager, PreparedRoutes};
use crate::i18n::{Lang, Tr, tr};
use crate::journal::Journal;
use crate::net::{
    AdapterInfo, AdapterSelection, CachedAdapter, NetworkLease, NetworkState, SelectionState,
    build_adapter_cache, inventory_adapters, reconcile_selection,
};
use crate::settings::{CachedAdapters, Options, Settings};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::time::{Duration, Instant};

mod dialogs;
mod events;
mod present;
mod runtime;
mod search;
mod sessions;
mod state;
mod table;
mod tree;

#[cfg(test)]
mod tests;

pub use dialogs::{
    AboutDialog, AddSignalDialog, ColumnsDialog, ConfirmAction, ConfirmDialog, ConnectDialog,
    Dialogs, MessageDialog, OptionsDialog, PendingWrites, WriteDialog,
};
pub use present::compose_server_name;
pub use state::{
    AddChildrenJob, BottomTab, CsvImportJob, CsvImportRow, ReconnectState, Row, SessionState,
    SessionView, TreeItem, WriteKind, WriteTypeChoice, write_type_choices,
};

use self::present::{install_fonts, is_loopback_host, operation_label};

pub struct App {
    pub gateway: Gateway,
    pub settings: Settings,
    /// Lazily populated adapter inventory + current network lease.
    pub network: NetworkState,
    /// DA host-route steering (helper client + refcounted temporary routes).
    pub da_routes: DaRouteManager,
    da_route_holds: Vec<DaRouteHold>,
    last_da_route_refresh: Instant,
    pub sessions: Vec<SessionView>,
    pub rows: Vec<Row>,
    pub journal: Journal,
    pub dialogs: Dialogs,
    pub active_session: Option<SessionId>,
    pub selected_tree: Option<(SessionId, u64)>,
    pub selected_rows: HashSet<ItemKey>,
    pub script_text: String,
    pub status: String,
    pub table_focus: Option<usize>,
    pub table_anchor: Option<usize>,
    pub tree_focus: Option<(SessionId, u64)>,
    /// Whether the tag tree owns keyboard focus for navigation. Cleared when a
    /// text field or a modal dialog captures the keyboard.
    pub tree_focused: bool,
    pub last_update: Option<DateTime<Utc>>,
    /// Selected bottom tab (only relevant when both panels are enabled).
    pub bottom_tab: BottomTab,
    /// Session-info popup opened from the status bar indicator.
    pub status_popup: bool,
    /// Write progress for the pending-write status indicator.
    pub pending_writes: usize,
    pub write_total: usize,
    pub write_ok: usize,
    pub write_err: usize,
    pub last_write_summary: Option<String>,
    /// Active (or finished) modal CSV import job.
    pub csv_import: Option<CsvImportJob>,
    /// Active recursive «Добавить дочерние» job (one per session).
    pub add_children: Option<AddChildrenJob>,
    next_session: SessionId,
    next_key: ItemKey,
    next_request: u64,
    /// Monotonic id for streaming subnet scans (independent of request ids).
    next_scan: u64,
    /// Outstanding `(session, request_id)` pairs. Non-zero events are accepted
    /// only while their pair is present; the pair is removed on receipt. A
    /// session's entries are cleared whenever a new connect/disconnect lifecycle
    /// starts, so a reply from a superseded connection cannot mutate this one.
    pending_requests: HashSet<(SessionId, RequestId)>,
    /// Pending "Добавить по имени" BrowsePath resolution: (session, requested
    /// name, display name). Consumed when `PathResolved` arrives.
    pending_path_resolve: Option<(SessionId, String, String)>,
}

struct DaRouteHold {
    session: SessionId,
    host: String,
    prepared: PreparedRoutes,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self::new_inner(Settings::load());
        app.journal.info_l(Tr::AppStarted, &[]);
        app.journal.info_l(Tr::OriginNote, &[]);
        app
    }

    /// Headless test constructor: default egui fonts (no font files) and
    /// default settings (no user config). Does not differ for the binary.
    pub fn new_test(ctx: &egui::Context) -> Self {
        Self::new_test_with_settings(ctx, Settings::default())
    }

    /// Headless test constructor with explicit settings (no user config).
    pub fn new_test_with_settings(ctx: &egui::Context, settings: Settings) -> Self {
        ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self::new_inner(settings);
        app.journal.info_l(Tr::AppStarted, &[]);
        app.journal.info_l(Tr::OriginNote, &[]);
        app
    }

    /// Current UI language (backed by settings, so it survives restarts).
    pub fn lang(&self) -> Lang {
        self.settings.lang()
    }

    /// Switch the UI language on the fly and persist it.
    pub fn set_lang(&mut self, lang: Lang) {
        if self.settings.lang() == lang {
            return;
        }
        self.settings.set_lang(lang);
        self.settings.save();
        let msg = crate::i18n::fmt(lang, Tr::OptionsSaved, &[]);
        self.status = msg;
    }

    fn new_inner(settings: Settings) -> Self {
        let script_text = settings.script.clone();
        let lang = settings.lang();
        let mut app = Self {
            gateway: Gateway::new(),
            settings,
            network: NetworkState::default(),
            da_routes: DaRouteManager::new(),
            da_route_holds: Vec::new(),
            last_da_route_refresh: Instant::now(),
            sessions: Vec::new(),
            rows: Vec::new(),
            journal: Journal::default(),
            dialogs: Dialogs::default(),
            active_session: None,
            selected_tree: None,
            selected_rows: HashSet::new(),
            script_text,
            status: tr(lang, Tr::Ready).to_string(),
            table_focus: None,
            table_anchor: None,
            tree_focus: None,
            tree_focused: false,
            last_update: None,
            bottom_tab: BottomTab::Journal,
            status_popup: false,
            pending_writes: 0,
            write_total: 0,
            write_ok: 0,
            write_err: 0,
            last_write_summary: None,
            csv_import: None,
            add_children: None,
            next_session: 1,
            next_key: 1,
            next_request: 1,
            next_scan: 1,
            pending_requests: HashSet::new(),
            pending_path_resolve: None,
        };
        app.dialogs.options.draft = app.settings.options.clone();
        app.init_network_from_cache();
        app
    }

    pub fn save_settings(&mut self) {
        self.settings.script = self.script_text.clone();
        self.settings.save();
    }

    /// Populate the network state from `settings.cached_adapters` without
    /// querying Windows. Missing/old configs simply start with an empty list.
    fn init_network_from_cache(&mut self) {
        let cached = self.settings.cached_adapters.adapters.clone();
        for warning in self.settings.cached_adapters.rejected.clone() {
            self.journal.warn(format!("network cache: {warning}"));
        }
        self.journal
            .info(format!("network cache loaded: {} adapter(s)", cached.len()));
        for adapter in &cached {
            self.journal_cached_adapter(adapter);
        }
        let report = self.network.load_cache(&cached);
        for rejection in &report.rejected {
            self.journal.warn(format!(
                "network cache: skipped id={} name=\"{}\" reason={}",
                rejection.id,
                rejection.name,
                rejection.reason.as_str()
            ));
        }
        let changed = self.sync_network_from_settings();
        if changed {
            self.settings.save();
        }
    }

    /// Explicit «Обновить»: fully replace the cache with the current Windows
    /// inventory. This is the only caller of `GetAdaptersAddresses`.
    pub fn refresh_network(&mut self) {
        let lang = self.lang();
        self.journal.info("network refresh: start");
        match inventory_adapters() {
            Ok(inventory) => self.apply_network_inventory(inventory),
            Err(err) => {
                // Keep the previous cache and lease; only surface the failure.
                self.network.error = Some(err.to_string());
                self.network.refresh_failed = true;
                self.status = tr(lang, Tr::NetworkRefreshFailed).to_string();
                self.journal
                    .error(format!("network refresh: failed: {err}"));
            }
        }
    }

    /// Apply a freshly read inventory. Public so integration tests can exercise
    /// the same replacement path without touching the real Windows API.
    pub fn apply_network_inventory(&mut self, inventory: Vec<AdapterInfo>) {
        self.journal.info(format!(
            "network refresh: inventory {} adapter(s)",
            inventory.len()
        ));
        for adapter in &inventory {
            self.journal_network_adapter(adapter);
        }
        let build = build_adapter_cache(&inventory);
        for skip in &build.skipped {
            self.journal_network_adapter(&skip.adapter);
            self.journal.info(format!(
                "network adapter skipped: id={} name=\"{}\" reason={}",
                skip.adapter.id,
                skip.adapter.name,
                skip.reason.as_str()
            ));
        }
        let cached = build.cached;
        self.network
            .set_adapters(cached.iter().map(CachedAdapter::to_info).collect());
        self.settings.cached_adapters = CachedAdapters::from_vec(cached);
        let _ = self.sync_network_from_settings();
        self.settings.save();
        self.journal.info(format!(
            "network refresh: done, {} usable adapter(s)",
            self.network.adapters.len()
        ));
        self.dialogs.connect.error = None;
    }

    fn journal_network_adapter(&mut self, adapter: &AdapterInfo) {
        let addresses = adapter
            .addresses
            .iter()
            .map(|a| a.display_with_prefix())
            .collect::<Vec<_>>()
            .join(",");
        let gateways = adapter
            .gateways
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let kind = if adapter.is_loopback {
            "loopback"
        } else if adapter.is_physical {
            "physical"
        } else {
            "virtual"
        };
        self.journal.info(format!(
            "network adapter: id={} name=\"{}\" description=\"{}\" state={} addresses=[{}] gateways=[{}] metric={} type={}",
            adapter.id,
            adapter.name,
            adapter.description,
            adapter.state.as_str(),
            addresses,
            gateways,
            adapter.metric,
            kind
        ));
    }

    fn journal_cached_adapter(&mut self, adapter: &CachedAdapter) {
        self.journal_network_adapter(&adapter.to_info());
    }

    /// Reconcile the saved GUID/IP with the current cache. Returns true when
    /// settings changed (the caller persists them). A stale GUID is kept until
    /// the user presses «Забыть», but produces no lease and no IP.
    fn sync_network_from_settings(&mut self) -> bool {
        let requested = self.settings.adapter_selection();
        let preferred_ip = self.settings.selected_local_ip();
        let resolution = reconcile_selection(&self.network.adapters, &requested, preferred_ip);
        let mut changed = false;
        match resolution.state {
            SelectionState::Selected => {
                let id_text = resolution.selection.adapter_id().map(|id| id.to_string());
                let ip_text = resolution.local_ip.map(|ip| ip.to_string());
                if self.settings.preferred_adapter != id_text
                    || self.settings.selected_local_ip != ip_text
                {
                    self.settings
                        .set_adapter_selection(resolution.selection.clone());
                    self.settings.set_selected_local_ip(resolution.local_ip);
                    changed = true;
                }
                self.network
                    .resolve(resolution.selection.clone(), resolution.local_ip);
                self.journal.info(format!(
                    "network selection: adapter={} ip={} binding={} lease_generation={}",
                    id_text.unwrap_or_default(),
                    ip_text.unwrap_or_default(),
                    self.binding_summary(),
                    self.network.generation
                ));
            }
            SelectionState::Stale => {
                self.network.resolve(AdapterSelection::None, None);
                let id = self.settings.preferred_adapter.clone().unwrap_or_default();
                self.journal.warn(format!(
                    "network selection stale: saved adapter {id} is unavailable"
                ));
            }
            SelectionState::NotSelected => {
                if self.settings.preferred_adapter.is_some()
                    || self.settings.selected_local_ip.is_some()
                {
                    self.settings.set_adapter_selection(AdapterSelection::None);
                    self.settings.set_selected_local_ip(None);
                    changed = true;
                }
                self.network.resolve(AdapterSelection::None, None);
                self.journal.info("network selection: none");
            }
        }
        changed
    }

    fn binding_summary(&self) -> String {
        match self.network.lease.as_ref() {
            Some(lease) => {
                let binding = lease.network_binding();
                format!(
                    "ip={} ipv4_if={:?} ipv6_if={:?} scope={:?}",
                    binding.local_ip,
                    binding.ipv4_if_index,
                    binding.ipv6_if_index,
                    binding.scope_id
                )
            }
            None => "none".to_string(),
        }
    }

    /// Binding captured from the current lease. Never refreshes the inventory:
    /// the cache is populated at startup and only replaced by «Обновить».
    pub fn current_network_binding(&mut self) -> Option<NetworkBinding> {
        self.network
            .lease
            .as_ref()
            .map(NetworkLease::network_binding)
    }

    /// Discovery binding for a host: loopback/link-local names always use the
    /// loopback binding and never a physical adapter's local IP.
    pub(super) fn search_binding(&mut self, host: &str) -> Option<NetworkBinding> {
        if is_loopback_host(host) {
            Some(NetworkBinding::loopback())
        } else {
            self.current_network_binding()
        }
    }

    /// Persist the adapter/IP choice and resolve a fresh lease against the
    /// current cache. Existing sessions keep their captured binding.
    pub fn apply_network_selection(
        &mut self,
        selection: AdapterSelection,
        local_ip: Option<std::net::IpAddr>,
    ) {
        if selection.is_none() {
            self.settings
                .save_network_selection(AdapterSelection::None, None);
            self.network.resolve(AdapterSelection::None, None);
            self.journal.info("network selection: cleared");
            return;
        }
        // A saved IP survives only while it belongs to the target adapter;
        // otherwise the first suitable address wins.
        let preferred_ip = local_ip.or_else(|| self.settings.selected_local_ip());
        let resolution = reconcile_selection(&self.network.adapters, &selection, preferred_ip);
        match resolution.state {
            SelectionState::Selected => {
                self.settings
                    .save_network_selection(resolution.selection.clone(), resolution.local_ip);
                self.network
                    .resolve(resolution.selection.clone(), resolution.local_ip);
                self.journal.info(format!(
                    "network selection: adapter={} ip={} binding={} lease_generation={}",
                    self.settings.preferred_adapter.clone().unwrap_or_default(),
                    self.settings.selected_local_ip.clone().unwrap_or_default(),
                    self.binding_summary(),
                    self.network.generation
                ));
            }
            _ => {
                self.network.resolve(AdapterSelection::None, None);
                let id = self
                    .settings
                    .preferred_adapter
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                self.journal.warn(format!(
                    "network selection stale: adapter {id} is unavailable"
                ));
            }
        }
    }

    pub fn update(&mut self, ui: &mut egui::Ui) {
        self.poll_events();
        self.tick();
        crate::ui::draw(self, ui);
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    pub(super) fn send(
        &mut self,
        session: SessionId,
        protocol: Protocol,
        kind: CommandKind,
    ) -> u64 {
        let request_id = self.next_request;
        self.next_request += 1;
        let operation = kind.operation();
        // Scan commands report progress/termination through their own
        // `scan_id`-correlated streaming events (request_id 0), so they must not
        // reserve a request-id pending slot that no reply will ever consume.
        let streaming = matches!(
            &kind,
            CommandKind::ScanUaNetwork { .. } | CommandKind::CancelUaScan { .. }
        );
        if request_id != 0 && !streaming {
            self.pending_requests.insert((session, request_id));
        }
        if let Err(message) = self.gateway.send(Command {
            request_id,
            session,
            protocol,
            kind,
        }) {
            self.pending_requests.remove(&(session, request_id));
            // A closed command channel means the protocol worker is gone, so
            // every session routed to it is dead. Surface the failure instead of
            // silently dropping the command; do not arm reconnect (a reconnect
            // command could never reach the dead worker).
            let lang = self.lang();
            self.journal
                .error_l(Tr::ErrorOp, &[&operation_label(lang, operation), &message]);
            let mut dead: Vec<SessionId> = Vec::new();
            for view in self
                .sessions
                .iter_mut()
                .filter(|view| view.protocol() == protocol)
            {
                view.state = SessionState::Disconnected;
                view.info = message.clone();
                view.read_in_flight = false;
                view.root_loading = false;
                view.subscription = SubscriptionState::Inactive;
                for item in &mut view.tree {
                    item.loading = false;
                }
                dead.push(view.id);
            }
            // Replies can never arrive for a dead worker: drop every pending id
            // (including any queued by an earlier session of that protocol).
            for id in &dead {
                self.clear_pending_requests(*id);
                self.release_da_routes_for_session(*id);
            }
            if self
                .add_children
                .as_ref()
                .is_some_and(|job| dead.contains(&job.session))
            {
                self.add_children = None;
            }
        }
        request_id
    }

    /// Forget every outstanding request of `session`. Called when a new
    /// connect/disconnect lifecycle starts so stale replies from the previous
    /// link are rejected by `on_event`.
    pub(super) fn clear_pending_requests(&mut self, session: SessionId) {
        self.pending_requests.retain(|(sid, _)| *sid != session);
    }

    /// Test-only: outstanding request ids of `session` (sorted, deterministic).
    pub fn pending_request_ids_for_test(&self, session: SessionId) -> Vec<RequestId> {
        let mut ids: Vec<RequestId> = self
            .pending_requests
            .iter()
            .filter_map(|(sid, rid)| (*sid == session).then_some(*rid))
            .collect();
        ids.sort_unstable();
        ids
    }
}

impl App {
    /// Test-only seeding: append a connected session view.
    pub fn seed_session(&mut self, id: SessionId, endpoint: EndpointSpec) {
        self.sessions.push(SessionView {
            id,
            display_name: compose_server_name(&endpoint),
            endpoint,
            state: SessionState::Connected,
            visible: true,
            info: tr(Lang::Ru, Tr::Ready).to_string(),
            tree: Vec::new(),
            next_tree_id: 1,
            root_loaded: true,
            root_loading: false,
            read_in_flight: false,
            last_poll: Instant::now(),
            last_test: Instant::now(),
            reconnect: ReconnectState::Idle,
            ua_view_id: None,
            ua_browse_vars_only: false,
            ua_session_timeout_ms: crate::settings::default_session_timeout(),
            subscription: SubscriptionState::Inactive,
        });
        self.active_session = Some(id);
        self.selected_tree = None;
    }

    /// Test-only seeding: append a tree node and return its id.
    pub fn seed_tree_item(
        &mut self,
        session: SessionId,
        parent: Option<u64>,
        depth: usize,
        node: BrowseNode,
    ) -> u64 {
        let view = self
            .sessions
            .iter_mut()
            .find(|s| s.id == session)
            .expect("seed session not found");
        let id = view.next_tree_id;
        view.next_tree_id += 1;
        view.tree.push(TreeItem {
            id,
            parent,
            node,
            depth,
            expanded: false,
            loaded: true,
            loading: false,
        });
        id
    }

    /// Test-only seeding: append a fully-formed row with a value.
    pub fn seed_row(
        &mut self,
        session: SessionId,
        item_id: &str,
        name: &str,
        value: Value,
    ) -> ItemKey {
        let server_label = self
            .sessions
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.endpoint.display_name())
            .unwrap_or_default();
        let key = self.next_key;
        self.next_key += 1;
        self.rows.push(Row {
            key,
            session,
            item_id: item_id.to_string(),
            display_name: name.to_string(),
            server_label,
            data_type: None,
            canonical_vt: None,
            write_type: None,
            writable: false,
            value,
            quality: Quality {
                code: 0xC0,
                kind: QualityKind::Good,
                text: "GOOD".to_string(),
            },
            timestamp: None,
            description: String::new(),
            pending: false,
        });
        key
    }

    /// Test-only seeding: append a fully-formed writable row with a value.
    pub fn seed_writable_row(
        &mut self,
        session: SessionId,
        item_id: &str,
        name: &str,
        value: Value,
    ) -> ItemKey {
        let key = self.seed_row(session, item_id, name, value);
        if let Some(row) = self.rows.iter_mut().find(|r| r.key == key) {
            row.writable = true;
        }
        key
    }
}

impl App {
    pub fn active_sessions(&self) -> Vec<(SessionId, String)> {
        self.sessions
            .iter()
            .filter(|s| s.visible)
            .map(|s| {
                (
                    s.id,
                    format!("{} ({})", s.endpoint.display_name(), s.endpoint.protocol),
                )
            })
            .collect()
    }

    /// All OPC UA session containers, including hidden and disconnected ones.
    /// DA sessions are never counted against the UA cap.
    pub fn ua_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.protocol() == Protocol::OpcUa)
            .count()
    }

    pub fn da_route_status_line(&self) -> String {
        let status = self.da_routes.status().localized(self.lang()).to_string();
        match self.da_routes.last_error() {
            Some(error) => format!("{status} — {}", error.localized(self.lang())),
            None => status,
        }
    }

    fn local_ips(&self) -> Vec<std::net::IpAddr> {
        self.network
            .adapters
            .iter()
            .flat_map(|adapter| adapter.addresses.iter().map(|address| address.ip))
            .collect()
    }

    pub(super) fn prepare_da_route_for_host(
        &mut self,
        host: &str,
    ) -> Result<PreparedRoutes, DaRouteError> {
        let adapter = self
            .network
            .lease
            .as_ref()
            .map(|lease| lease.adapter.clone());
        let local_ips = self.local_ips();
        let result = self
            .da_routes
            .prepare_host(host, adapter.as_ref(), &local_ips);
        let status = self.da_routes.status();
        match &result {
            Ok(prepared) => self.journal.info(format!(
                "DA route: status={status:?} host=\"{host}\" targets=[{}]",
                prepared
                    .targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )),
            Err(error) => self.journal.error(format!(
                "DA route: status={status:?} host=\"{host}\" error={error:?}"
            )),
        }
        result
    }

    pub(super) fn hold_da_route(
        &mut self,
        session: SessionId,
        host: &str,
        prepared: PreparedRoutes,
    ) {
        if prepared.is_empty() {
            return;
        }
        self.da_route_holds.push(DaRouteHold {
            session,
            host: host.trim().to_string(),
            prepared,
        });
    }

    pub(super) fn release_da_routes_for_session(&mut self, session: SessionId) {
        let mut index = 0;
        while index < self.da_route_holds.len() {
            if self.da_route_holds[index].session == session {
                let hold = self.da_route_holds.remove(index);
                self.da_routes.release(hold.prepared);
            } else {
                index += 1;
            }
        }
    }

    pub(super) fn release_da_discovery_route(&mut self, host: &str) {
        let host = host.trim();
        let mut index = 0;
        while index < self.da_route_holds.len() {
            let hold = &self.da_route_holds[index];
            if hold.session == 0 && hold.host.eq_ignore_ascii_case(host) {
                let hold = self.da_route_holds.remove(index);
                self.da_routes.release(hold.prepared);
            } else {
                index += 1;
            }
        }
    }

    pub(super) fn release_da_discovery_routes(&mut self) {
        let mut index = 0;
        while index < self.da_route_holds.len() {
            if self.da_route_holds[index].session == 0 {
                let hold = self.da_route_holds.remove(index);
                self.da_routes.release(hold.prepared);
            } else {
                index += 1;
            }
        }
    }

    pub(super) fn has_da_route_hold(&self, session: SessionId) -> bool {
        self.da_route_holds
            .iter()
            .any(|hold| hold.session == session)
    }
}

impl App {
    /// Open the connect dialog. The reference behavior mirrors the original
    /// client: search starts immediately from an empty address (this computer +
    /// local network for DA), and the host/port fields are NOT pre-filled.
    pub fn open_connect_dialog(&mut self) {
        // Never leave a previous popup's scan running behind a closed dialog.
        self.cancel_ua_subnet_scan();
        // Discard replies from a search owned by the previous popup instance.
        self.clear_pending_requests(0);
        let d = &mut self.dialogs.connect;
        d.open = true;
        d.network_tab = false;
        d.error = None;
        // Empty by design: no persistence restores host/port/URL into the
        // fields. The servers list is what the user picks from.
        d.da_host.clear();
        d.da_manual.clear();
        d.da_servers.clear();
        d.da_server_hosts.clear();
        d.da_pending = false;
        d.da_selected_host = None;
        d.ua_search_host.clear();
        d.ua_search_port.clear();
        d.ua_url.clear();
        d.ua_searching = false;
        // Reuse discovered UA results across popup visits. A new search
        // explicitly replaces them; closing the popup does not.
        d.da_selected = None;
        d.ua_auto_pending = 0;
        d.da_auto_pending = 0;
        d.focus_pending = true;
        d.ua_user.clear();
        d.ua_password.clear();
        d.ua_security_policy = self.settings.ua_security_policy;
        d.ua_security_mode = self.settings.ua_security_mode;
        d.ua_trust_server_cert = self.settings.ua_trust_server_cert;
        d.ua_identity_index = 0;
        d.ua_session_timeout = self.settings.ua_session_timeout_ms;
        d.ua_network_searching = false;
        d.ua_subnet_searching = false;
        d.ua_scan_id = None;
        d.ua_scan_checked = 0;
        d.ua_scan_total = 0;
        d.ua_scan_found = 0;
        d.ua_scan_error = None;
        d.ua_scan_confirm = None;
        // DA tab is the default: start a fresh, correctly host-scoped discovery.
        self.discover_da_all();
    }

    /// Close the connect popup and invalidate all search results still in
    /// flight. Network settings themselves are applied immediately and are not
    /// rolled back by closing the dialog.
    pub fn close_connect_dialog(&mut self) {
        self.cancel_ua_subnet_scan();
        self.clear_pending_requests(0);
        let d = &mut self.dialogs.connect;
        d.open = false;
        d.ua_searching = false;
        d.ua_network_searching = false;
        d.ua_auto_pending = 0;
        d.da_auto_pending = 0;
        d.da_pending = false;
        d.ua_scan_confirm = None;
    }

    pub fn open_options_dialog(&mut self) {
        self.dialogs.options.draft = self.settings.options.clone();
        self.dialogs.options.open = true;
        self.dialogs.options.focus_pending = true;
    }

    pub fn apply_options(&mut self) {
        let draft = clamp_options(self.dialogs.options.draft.clone());
        self.settings.options = draft;
        self.settings.save();
        self.dialogs.options.open = false;
        // Reconfigure UA push subscriptions so a newly created subscription
        // honors the updated interval / deadband. Only already-connected
        // sessions have a worker-side subscription to configure; a disconnected
        // UA session gets fresh config on its next Connected.
        let ua_session_ids: Vec<(SessionId, Protocol)> = self
            .sessions
            .iter()
            .filter(|s| s.protocol() == Protocol::OpcUa && s.state == SessionState::Connected)
            .map(|s| (s.id, s.protocol()))
            .collect();
        for (session, protocol) in ua_session_ids {
            self.send(
                session,
                protocol,
                CommandKind::ConfigureUaSubscription {
                    publishing_interval_ms: self.settings.options.update_interval_secs.max(1)
                        * 1000,
                    deadband: self.settings.options.ua_deadband,
                },
            );
        }
        self.journal.info_l(Tr::OptionsSaved, &[]);
    }

    pub fn save_script(&mut self) {
        self.settings.script = self.script_text.clone();
        self.settings.save();
        self.journal.info_l(Tr::ScriptSaved, &[]);
    }

    pub fn script_protocol_note(&self) -> String {
        tr(self.lang(), Tr::ScriptNotImplNote).to_string()
    }
}

/// Clamp a draft `Options` into its valid, UI-backed ranges before persisting.
fn clamp_options(draft: Options) -> Options {
    let mut draft = draft;
    draft.update_interval_secs = draft
        .update_interval_secs
        .clamp(UPDATE_INTERVAL_MIN_SECS, UPDATE_INTERVAL_MAX_SECS);
    draft.operation_timeout_secs = draft
        .operation_timeout_secs
        .clamp(OPERATION_TIMEOUT_MIN_SECS, OPERATION_TIMEOUT_MAX_SECS);
    draft.test_interval_secs = draft
        .test_interval_secs
        .clamp(TEST_INTERVAL_MIN_SECS, TEST_INTERVAL_MAX_SECS);
    draft.ua_deadband = draft.ua_deadband.clamp(0.0, 1_000_000.0);
    draft
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.update(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
        let sessions: Vec<(SessionId, Protocol)> = self
            .sessions
            .iter()
            .map(|session| (session.id, session.protocol()))
            .collect();
        for (session, protocol) in sessions {
            self.send(session, protocol, CommandKind::Disconnect);
        }
        self.gateway.shutdown();
    }
}
