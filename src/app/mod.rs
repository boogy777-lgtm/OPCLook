use crate::bff::model::{
    BrowseNode, CommandKind, ItemKey, Operation, Owner, Protocol, Quality, QualityKind, RequestId,
    ScanId, SessionId, SubscriptionState, Value,
};
use crate::bff::net::NetworkBinding;

#[cfg(feature = "test-support")]
use crate::bff::model::EndpointSpec;
use crate::da_route::{DaRouteError, LocalizedDaRouteError, LocalizedDaRouteStatus};
use crate::i18n::{Lang, Tr, tr};
use crate::journal::Journal;
use crate::net::{
    AdapterInfo, AdapterSelection, CachedAdapter, NetworkLease, NetworkState, SelectionState,
    build_adapter_cache, inventory_adapters, reconcile_selection,
};
use crate::settings::{CachedAdapters, Options, Settings};
use chrono::{DateTime, Utc};
#[cfg(test)]
use opclook_engine::PendingRequest as EnginePendingRequest;
use opclook_engine::{DispatchError, Engine};
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub mod actions;
mod dialogs;
mod events;
mod present;
mod row_index;
mod runtime;
mod search;
mod sessions;
mod state;
mod table;
mod tree;
mod value_time;
mod view;

#[cfg(test)]
mod tests;

pub use dialogs::{
    AboutDialog, AddSignalDialog, AutoPending, ColumnsDialog, ConfirmAction, ConfirmRequest,
    ConnectDialog, ConnectPane, DaHit, Dialogs, MessageDialog, OptionsDialog, PendingWrites,
    UaIdentity, UaSearch, WriteDialog,
};
pub use present::{CJK_FONT_ENV, compose_server_name, install_fonts};
pub use state::{
    AddChildrenJob, BottomTab, CsvImportJob, CsvImportRow, CsvImportState, ReconnectState, Row,
    RowQuality, SessionState, SessionView, TreeItem, WriteKind, WriteTypeChoice,
    write_type_choices,
};
pub use value_time::{TimeOrder, TimeSelection};
pub use view::{CHANGE_FLASH, MAX_FIELD_LINES, TableLine, is_numeric_value};

use self::present::{is_loopback_host, operation_label};

pub struct App {
    engine: Engine<PendingCtx>,
    read_observations:
        std::sync::Arc<std::sync::Mutex<opclook_contract::read_observer::ReadObservations>>,
    pub settings: Settings,
    /// Lazily populated adapter inventory + current network lease.
    pub network: NetworkState,
    pub sessions: Vec<SessionView>,
    rows: Vec<Row>,
    rows_revision: u64,
    table_cache:
        std::cell::RefCell<Option<(std::rc::Rc<Vec<usize>>, u64, std::rc::Rc<Vec<TableLine>>)>>,
    detail_revision: u64,
    visible_cache: std::cell::RefCell<Option<(u64, Option<SessionId>, std::rc::Rc<Vec<usize>>)>>,
    pub journal: Journal,
    pub dialogs: Dialogs,
    pub active_session: Option<SessionId>,
    pub selected_tree: Option<(SessionId, u64)>,
    pub selected_rows: HashSet<ItemKey>,
    pub script_text: String,
    pub status: String,
    pub table_focus: Option<usize>,
    /// Identity associated with the focused visible row; prevents a changed
    /// row order from silently retargeting a keyboard write.
    pub(crate) table_focus_key: Option<ItemKey>,
    pub table_anchor: Option<usize>,
    pub(crate) table_anchor_key: Option<ItemKey>,
    pub(crate) table_additive_base: Option<HashSet<ItemKey>>,
    pub tree_focus: Option<(SessionId, u64)>,
    /// Whether the tag tree owns keyboard focus for navigation. Cleared when a
    /// text field or a modal dialog captures the keyboard.
    pub tree_focused: bool,
    /// Session whose rows the table showed last frame (V1); a change drops
    /// the selection so it never spans tabs.
    table_view_session: Option<SessionId>,
    /// Structured / array values expanded in the table (V3): the row and the
    /// path of field indices from the row value down to the expanded node.
    expanded_values: HashSet<(ItemKey, Vec<u32>)>,
    pub last_update: Option<DateTime<Utc>>,
    /// Selected bottom tab (only relevant when both panels are enabled).
    pub bottom_tab: BottomTab,
    /// Session-info popup opened from the status bar indicator.
    pub status_popup: bool,
    /// Write progress and completion summary for the status indicator.
    pub write_batch: WriteBatch,
    /// Active (or finished) modal CSV import job.
    pub csv_import: Option<CsvImportJob>,
    /// Active recursive «Добавить дочерние» job (one per session).
    pub add_children: Option<AddChildrenJob>,
    next_session: Option<SessionId>,
    next_key: Option<ItemKey>,
    /// Monotonic id for streaming subnet scans (independent of request ids).
    next_scan: Option<ScanId>,
    /// Result of the CJK font probe done at startup (A1).
    pub cjk_font: CjkFont,
    /// Lookup index over `rows` (A4); see `row_index.rs`.
    row_index: row_index::RowIndex,
}

/// GUI projection of a write batch. Request correlation remains in Engine.
#[derive(Default)]
pub struct WriteBatch {
    pub pending: usize,
    pub total: usize,
    pub ok: usize,
    pub errors: usize,
    pub summary: Option<String>,
}

impl WriteBatch {
    fn accepted(&mut self, new_batch: bool) {
        if new_batch {
            *self = Self::default();
        }
        self.pending += 1;
        self.total += 1;
    }

    fn lost(&mut self, count: usize) {
        let count = count.min(self.pending);
        self.pending -= count;
        self.errors += count;
    }

    fn completed(&mut self, success: bool) {
        self.pending = self.pending.saturating_sub(1);
        if success {
            self.ok += 1;
        } else {
            self.errors += 1;
        }
    }
}

/// Application context that travels with an outstanding request and comes back
/// with its reply (the backend protocol is untouched: the app keeps this map
/// itself, keyed by `request_id`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PendingCtx {
    None,
    /// `Browse` of one specific tree item (an OPC UA NodeId may occur in several
    /// places of the tree, so the reply must land under the item that was
    /// expanded, not under the first node with that id).
    TreeItem(u64),
    /// A user-entered browse target not yet present in the tree.
    PaletteBrowse(String),
    /// `Discover` of one DA host: only that host's temporary route is released
    /// when the request fails.
    Host(String),
    /// «Добавить по имени»: the name being resolved and the requested display
    /// name, completed when `PathResolved` (or its error) arrives.
    ResolvePath {
        requested: String,
        name: String,
    },
    /// One `Write` of a batch: if no reply ever comes, the write is reported
    /// as failed for this row so the batch counters can complete (K1).
    WriteKey(ItemKey),
    /// `AddItems` of these rows: if the whole command fails or never answers
    /// the rows must leave the "waiting" state (K1).
    AddKeys(Vec<ItemKey>),
}

#[cfg(test)]
type PendingRequest = EnginePendingRequest<PendingCtx>;

/// Whether a font able to draw Chinese text was found at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CjkFont {
    /// Headless/test construction: fonts were not probed.
    NotProbed,
    /// Nothing usable on this machine.
    Missing,
    /// Path of the CJK fallback font that was installed.
    Loaded(String),
}

fn owner_label(lang: Lang, owner: Owner) -> String {
    match owner {
        Owner::Session(session) => session.to_string(),
        Owner::Discovery => operation_label(lang, Operation::Discover),
    }
}

enum SendOutcome {
    Sent(RequestId),
    Deferred,
    Failed,
}

impl App {
    pub(super) fn session(&self, id: SessionId) -> Option<&SessionView> {
        self.sessions.iter().find(|view| view.id == id)
    }

    pub(super) fn session_mut(&mut self, id: SessionId) -> Option<&mut SessionView> {
        self.sessions.iter_mut().find(|view| view.id == id)
    }

    pub(super) fn session_protocol(&self, id: SessionId) -> Option<Protocol> {
        self.session(id).map(SessionView::protocol)
    }

    pub fn expanded_values(&self) -> &HashSet<(ItemKey, Vec<u32>)> {
        &self.expanded_values
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    #[cfg(feature = "test-support")]
    pub fn rows_mut_for_test(&mut self) -> &mut Vec<Row> {
        self.rows_changed();
        for row in &mut self.rows {
            row.invalidate_display();
        }
        &mut self.rows
    }

    fn rows_changed(&mut self) {
        self.table_additive_base = None;
        self.rows_revision = self.rows_revision.wrapping_add(1);
        self.detail_revision = self.detail_revision.wrapping_add(1);
        self.row_index.invalidate();
        *self.visible_cache.borrow_mut() = None;
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let cjk = install_fonts(&cc.egui_ctx);
        let loaded = Settings::load();
        crate::ui::apply_style(&cc.egui_ctx, loaded.settings.view.options.theme);
        let mut app = Self::new_inner(loaded.settings);
        for notice in loaded.notices {
            app.report_settings_notice(notice);
        }
        app.journal.info_l(Tr::AppStarted, &[]);
        app.cjk_font = match cjk {
            Some(path) => {
                app.journal.info_l(Tr::CjkFontLoaded, &[&path]);
                CjkFont::Loaded(path)
            }
            None => {
                if app.lang() == Lang::Zh {
                    app.journal.warn_l(Tr::CjkFontMissing, &[]);
                }
                CjkFont::Missing
            }
        };
        app
    }

    /// Headless test constructor: default egui fonts (no font files) and
    /// default settings (no user config). Does not differ for the binary.
    #[cfg(feature = "test-support")]
    pub fn new_test(ctx: &egui::Context) -> Self {
        Self::new_test_with_settings(ctx, Settings::default())
    }

    /// Headless test constructor with explicit settings (no user config).
    #[cfg(feature = "test-support")]
    pub fn new_test_with_settings(ctx: &egui::Context, settings: Settings) -> Self {
        ctx.set_visuals(egui::Visuals::dark());
        // Headless tests run in one process, in parallel: each test thread gets
        // its own settings directory and the process its own PKI store, never
        // the real `%APPDATA%` and never a directory shared with other tests.
        crate::settings::thread_dir::ensure_private();
        opclook_engine::testing::use_private_pki_dir();
        let mut app = Self::new_inner(settings);
        app.journal.info_l(Tr::AppStarted, &[]);
        app
    }

    #[cfg(feature = "test-support")]
    pub fn new_test_with_loaded(ctx: &egui::Context, loaded: crate::settings::Loaded) -> Self {
        let mut app = Self::new_test_with_settings(ctx, loaded.settings);
        for notice in loaded.notices {
            app.report_settings_notice(notice);
        }
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
        self.persist_settings();
        let msg = crate::i18n::fmt(lang, Tr::OptionsSaved, &[]);
        self.status = msg;
        if lang == Lang::Zh && self.cjk_font == CjkFont::Missing {
            self.journal.warn_l(Tr::CjkFontMissing, &[]);
        }
    }

    fn new_inner(settings: Settings) -> Self {
        let script_text = settings.view.script.clone();
        let lang = settings.lang();
        let read_observations = std::sync::Arc::new(std::sync::Mutex::new(
            opclook_contract::read_observer::ReadObservations::default(),
        ));
        let sink = read_observations.clone();
        let mut engine = Engine::default();
        engine.set_read_terminal_sink(Some(Box::new(move |session, epoch, terminal| {
            sink.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(session, epoch, terminal);
        })));
        let mut app = Self {
            engine,
            read_observations,
            settings,
            network: NetworkState::default(),
            sessions: Vec::new(),
            rows: Vec::new(),
            rows_revision: 0,
            table_cache: std::cell::RefCell::new(None),
            detail_revision: 0,
            visible_cache: std::cell::RefCell::new(None),
            journal: Journal::default(),
            dialogs: Dialogs::default(),
            active_session: None,
            selected_tree: None,
            selected_rows: HashSet::new(),
            script_text,
            status: tr(lang, Tr::Ready).to_string(),
            table_focus: None,
            table_focus_key: None,
            table_anchor: None,
            table_anchor_key: None,
            table_additive_base: None,
            tree_focus: None,
            tree_focused: false,
            table_view_session: None,
            expanded_values: HashSet::new(),
            last_update: None,
            bottom_tab: BottomTab::Journal,
            status_popup: false,
            write_batch: WriteBatch::default(),
            csv_import: None,
            add_children: None,
            next_session: Some(SessionId::FIRST),
            next_key: Some(ItemKey::FIRST),
            next_scan: Some(ScanId::FIRST),
            cjk_font: CjkFont::NotProbed,
            row_index: row_index::RowIndex::default(),
        };
        app.dialogs.options.draft = app.settings.view.options.clone();
        app.init_network_from_cache();
        app
    }

    pub fn save_settings(&mut self) -> Option<String> {
        self.settings.view.script.clone_from(&self.script_text);
        self.persist_settings()
    }

    /// Save and report errors immediately, including errors from UI actions.
    pub fn persist_settings(&mut self) -> Option<String> {
        self.settings
            .save()
            .err()
            .map(|notice| self.report_settings_notice(notice))
    }

    pub(super) fn report_settings_notice(
        &mut self,
        notice: crate::settings::SettingsNotice,
    ) -> String {
        use crate::settings::SettingsNotice;
        let warning = matches!(notice, SettingsNotice::AcquisitionMigrated { .. });
        let migration = matches!(
            notice,
            SettingsNotice::AcquisitionMigrated { .. }
                | SettingsNotice::AcquisitionMigrationFailed { .. }
                | SettingsNotice::AcquisitionMigrationBlocked
        );
        let (key, args): (Tr, Vec<String>) = match notice {
            SettingsNotice::ReadFailed { path, error } => {
                (Tr::SettingsReadFailed, vec![path, error])
            }
            SettingsNotice::ParseFailed {
                path,
                error,
                backup: Ok(backup),
            } => (Tr::SettingsParseFailed, vec![path, error, backup]),
            SettingsNotice::ParseFailed {
                path,
                error,
                backup: Err(why),
            } => (Tr::SettingsParseNoBackup, vec![path, error, why]),
            SettingsNotice::SaveFailed { path, error } => {
                (Tr::SettingsSaveFailed, vec![path, error])
            }
            SettingsNotice::SaveBlocked { path } => (Tr::SettingsSaveBlocked, vec![path]),
            SettingsNotice::AcquisitionMigrationBlocked => {
                (Tr::SettingsAcquisitionMigrationBlocked, vec![])
            }
            SettingsNotice::AcquisitionMigrated {
                ua,
                da,
                interval_ms,
            } => (
                Tr::SettingsAcquisitionMigrated,
                vec![ua.to_string(), da.to_string(), interval_ms.to_string()],
            ),
            SettingsNotice::AcquisitionMigrationFailed { seconds } => (
                Tr::SettingsAcquisitionMigrationFailed,
                vec![seconds.to_string()],
            ),
        };
        let refs: Vec<&dyn std::fmt::Display> = args
            .iter()
            .map(|arg| arg as &dyn std::fmt::Display)
            .collect();
        if warning {
            self.journal.warn_l(key, &refs);
        } else {
            self.journal.error_l(key, &refs);
        }
        let message = crate::i18n::fmt(self.lang(), key, &refs);
        if migration {
            self.status.clone_from(&message);
        }
        message
    }

    /// Populate the network state from `settings.network.cached_adapters` without
    /// querying Windows. Missing/old configs simply start with an empty list.
    fn init_network_from_cache(&mut self) {
        let cached = self.settings.network.cached_adapters.adapters.clone();
        for warning in self.settings.network.cached_adapters.rejected.clone() {
            self.journal.warn_l(Tr::NetworkCacheWarning, &[&warning]);
        }
        self.journal
            .info_l(Tr::NetworkCacheLoaded, &[&cached.len()]);
        for adapter in &cached {
            self.journal_cached_adapter(adapter);
        }
        let report = self.network.load_cache(&cached);
        for rejection in &report.rejected {
            let details = format!(
                "id={} name=\"{}\" reason={}",
                rejection.id,
                rejection.name,
                rejection.reason.as_str()
            );
            self.journal.warn_l(Tr::NetworkCacheSkipped, &[&details]);
        }
        let changed = self.sync_network_from_settings();
        if changed {
            self.persist_settings();
        }
    }

    /// Explicit «Обновить»: fully replace the cache with the current Windows
    /// inventory. This is the only caller of `GetAdaptersAddresses`.
    pub fn refresh_network(&mut self) {
        let lang = self.lang();
        self.journal.info_l(Tr::NetworkRefreshStart, &[]);
        match inventory_adapters() {
            Ok(inventory) => self.apply_network_inventory(inventory),
            Err(err) => {
                // Keep the previous cache and lease; only surface the failure.
                self.network.error = Some(err.to_string());
                self.network.refresh_failed = true;
                self.status = tr(lang, Tr::NetworkRefreshFailed).to_string();
                self.journal.error_l(Tr::NetworkRefreshError, &[&err]);
            }
        }
    }

    /// Apply a freshly read inventory. Public so integration tests can exercise
    /// the same replacement path without touching the real Windows API.
    pub fn apply_network_inventory(&mut self, inventory: Vec<AdapterInfo>) {
        self.journal
            .info_l(Tr::NetworkRefreshInventory, &[&inventory.len()]);
        for adapter in &inventory {
            self.journal_network_adapter(adapter);
        }
        let build = build_adapter_cache(&inventory);
        for skip in &build.skipped {
            self.journal_network_adapter(&skip.adapter);
            let details = format!(
                "id={} name=\"{}\" reason={}",
                skip.adapter.id,
                skip.adapter.name,
                skip.reason.as_str()
            );
            self.journal.info_l(Tr::NetworkAdapterSkipped, &[&details]);
        }
        let cached = build.cached;
        self.network
            .set_adapters(cached.iter().map(CachedAdapter::to_info).collect());
        self.settings.network.cached_adapters = CachedAdapters::from_vec(cached);
        let _ = self.sync_network_from_settings();
        self.persist_settings();
        self.journal
            .info_l(Tr::NetworkRefreshDone, &[&self.network.adapters.len()]);
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
        let details = format!(
            "id={} name=\"{}\" description=\"{}\" state={} addresses=[{}] gateways=[{}] metric={} type={}",
            adapter.id,
            adapter.name,
            adapter.description,
            adapter.state.as_str(),
            addresses,
            gateways,
            adapter.metric,
            kind
        );
        self.journal.info_l(Tr::NetworkAdapterDetails, &[&details]);
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
                let id = resolution.selection.adapter_id();
                let ip = resolution.local_ip;
                if self.settings.network.preferred_adapter != id
                    || self.settings.network.selected_local_ip != ip
                {
                    self.settings
                        .set_adapter_selection(resolution.selection.clone());
                    self.settings.set_selected_local_ip(resolution.local_ip);
                    changed = true;
                }
                self.network
                    .resolve(resolution.selection.clone(), resolution.local_ip);
                let details = format!(
                    "adapter={} ip={} binding={} lease_generation={}",
                    id.map(|value| value.to_string()).unwrap_or_default(),
                    ip.map(|value| value.to_string()).unwrap_or_default(),
                    self.binding_summary(),
                    self.network.generation
                );
                self.journal
                    .info_l(Tr::NetworkSelectionDetails, &[&details]);
            }
            SelectionState::Stale => {
                self.network.resolve(AdapterSelection::None, None);
                let id = self
                    .settings
                    .network
                    .preferred_adapter
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                self.journal.warn_l(Tr::NetworkSelectionStale, &[&id]);
            }
            SelectionState::NotSelected => {
                if self.settings.network.preferred_adapter.is_some()
                    || self.settings.network.selected_local_ip.is_some()
                {
                    self.settings.set_adapter_selection(AdapterSelection::None);
                    self.settings.set_selected_local_ip(None);
                    changed = true;
                }
                self.network.resolve(AdapterSelection::None, None);
                self.journal.info_l(Tr::NetworkSelectionNone, &[]);
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
            let result = self
                .settings
                .save_network_selection(AdapterSelection::None, None);
            if let Err(notice) = result {
                self.report_settings_notice(notice);
            }
            self.network.resolve(AdapterSelection::None, None);
            self.journal.info_l(Tr::NetworkSelectionCleared, &[]);
            return;
        }
        // A saved IP survives only while it belongs to the target adapter;
        // otherwise the first suitable address wins.
        let preferred_ip = local_ip.or_else(|| self.settings.selected_local_ip());
        let resolution = reconcile_selection(&self.network.adapters, &selection, preferred_ip);
        match resolution.state {
            SelectionState::Selected => {
                let result = self
                    .settings
                    .save_network_selection(resolution.selection.clone(), resolution.local_ip);
                if let Err(notice) = result {
                    self.report_settings_notice(notice);
                }
                self.network
                    .resolve(resolution.selection.clone(), resolution.local_ip);
                let details = format!(
                    "adapter={} ip={} binding={} lease_generation={}",
                    self.settings
                        .network
                        .preferred_adapter
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    self.settings
                        .network
                        .selected_local_ip
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    self.binding_summary(),
                    self.network.generation
                );
                self.journal
                    .info_l(Tr::NetworkSelectionDetails, &[&details]);
            }
            _ => {
                self.network.resolve(AdapterSelection::None, None);
                let id = self
                    .settings
                    .network
                    .preferred_adapter
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                self.journal.warn_l(Tr::NetworkSelectionStale, &[&id]);
            }
        }
    }

    pub fn update(&mut self, ui: &mut egui::Ui) {
        self.engine
            .set_auto_reconnect(self.settings.view.options.auto_reconnect);
        self.poll_events();
        self.tick();
        self.drain_read_observations();
        self.sync_table_view();
        crate::ui::draw(self, ui);
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    pub(super) fn send(
        &mut self,
        owner: impl Into<Owner>,
        protocol: Protocol,
        kind: CommandKind,
    ) -> Option<RequestId> {
        self.send_with(owner, protocol, kind, PendingCtx::None)
    }

    /// Like [`App::send`], remembering `ctx` until the reply arrives.
    pub(super) fn send_with(
        &mut self,
        owner: impl Into<Owner>,
        protocol: Protocol,
        kind: CommandKind,
        ctx: PendingCtx,
    ) -> Option<RequestId> {
        self.send_with_permit(owner, protocol, kind, ctx, None)
    }

    pub(super) fn send_with_permit(
        &mut self,
        owner: impl Into<Owner>,
        protocol: Protocol,
        kind: CommandKind,
        ctx: PendingCtx,
        permit: Option<opclook_engine::ReadPermit>,
    ) -> Option<RequestId> {
        match self.send_with_permit_inner(owner, protocol, kind, ctx, permit, false) {
            SendOutcome::Sent(id) => Some(id),
            SendOutcome::Deferred | SendOutcome::Failed => None,
        }
    }

    fn send_with_permit_inner(
        &mut self,
        owner: impl Into<Owner>,
        protocol: Protocol,
        kind: CommandKind,
        ctx: PendingCtx,
        permit: Option<opclook_engine::ReadPermit>,
        defer_browse: bool,
    ) -> SendOutcome {
        let owner = owner.into();
        let operation = kind.operation();
        let dispatch = match (owner, kind) {
            (Owner::Session(session), CommandKind::Read { keys }) if permit.is_some() => {
                match permit {
                    Some(permit) => self
                        .engine
                        .send_due_read(session, protocol, keys, permit, ctx),
                    None => return SendOutcome::Failed,
                }
            }
            (
                Owner::Session(session),
                CommandKind::Browse {
                    node_id,
                    filter,
                    view_id,
                },
            ) => self.engine.execute_action(
                opclook_contract::actions::DomainAction::Browse {
                    session,
                    protocol,
                    node_id,
                    filter,
                    view_id,
                },
                ctx,
            ),
            (Owner::Session(session), CommandKind::Write { request }) => {
                self.engine.execute_action(
                    opclook_contract::actions::DomainAction::Write {
                        session,
                        protocol,
                        request,
                    },
                    ctx,
                )
            }
            (owner, kind) => self.engine.send(owner, protocol, kind, ctx),
        };
        let (request_id, failure) = match dispatch {
            Ok(id) => (id, None),
            Err(DispatchError::InvalidActionPayload(_)) => {
                self.journal.error_l(Tr::ActionPayloadInvalid, &[]);
                self.status = tr(self.lang(), Tr::ActionPayloadInvalid).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::InvalidAcquisition(_)) => {
                self.journal.error_l(Tr::AcquisitionInvalid, &[]);
                self.status = tr(self.lang(), Tr::AcquisitionInvalid).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::Exhausted) => {
                self.journal.error_l(Tr::RequestIdsExhausted, &[]);
                self.status = tr(self.lang(), Tr::RequestIdsExhausted).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::ReadBusy) => {
                self.journal.error_l(Tr::ReadBusy, &[]);
                self.status = tr(self.lang(), Tr::ReadBusy).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::ReadEmpty) => {
                self.journal.error_l(Tr::ReadEmpty, &[]);
                self.status = tr(self.lang(), Tr::ReadEmpty).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::ItemKeyReused) => {
                self.journal.error_l(Tr::ItemKeyReused, &[]);
                self.status = tr(self.lang(), Tr::ItemKeyReused).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::ItemKeyLimit) => {
                self.journal.error_l(Tr::ItemKeyLimit, &[]);
                self.status = tr(self.lang(), Tr::ItemKeyLimit).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::ActionUnavailable(
                opclook_engine::ActionUnavailable::ReadPending,
            )) if defer_browse => return SendOutcome::Deferred,
            Err(DispatchError::ActionUnavailable(reason)) => {
                let key = actions::action_unavailable_tr(reason);
                self.journal.error_l(key, &[]);
                self.status = tr(self.lang(), key).to_string();
                return SendOutcome::Failed;
            }
            Err(DispatchError::SessionLimit) => {
                self.journal.error_l(
                    Tr::UaSessionLimitReached,
                    &[&crate::constants::MAX_UA_SESSIONS.to_string()],
                );
                self.status = crate::i18n::fmt(
                    self.lang(),
                    Tr::UaSessionLimitReached,
                    &[&crate::constants::MAX_UA_SESSIONS.to_string()],
                );
                return SendOutcome::Failed;
            }
            Err(DispatchError::Closed { request_id, cause }) => (
                request_id,
                Some(opclook_locale::send_error_text(self.lang(), cause).to_string()),
            ),
        };
        if self.settings.view.options.diagnostic_trace {
            self.journal.info_l(
                Tr::TraceSent,
                &[
                    &request_id.to_string(),
                    &operation_label(self.lang(), operation),
                    &owner_label(self.lang(), owner),
                ],
            );
        }
        if let Some(ref message) = failure {
            // A closed command channel means the protocol worker is gone, so
            // every session routed to it is dead. Surface the failure instead of
            // silently dropping the command; do not arm reconnect (a reconnect
            // command could never reach the dead worker).
            self.status.clone_from(message);
            let lang = self.lang();
            self.journal
                .error_l(Tr::ErrorOp, &[&operation_label(lang, operation), &message]);
            let dead: Vec<SessionId> = self
                .sessions
                .iter()
                .filter(|view| view.protocol() == protocol)
                .map(|view| view.id)
                .collect();
            for id in dead {
                self.project_session(id);
                self.mark_link_lost(id, Some(message));
                self.release_da_routes_for_session(id);
            }
        }
        if failure.is_some() {
            SendOutcome::Failed
        } else {
            SendOutcome::Sent(request_id)
        }
    }

    /// Clear the UI state associated with a link that can no longer answer.
    /// Reconnect policy remains owned by Engine and is projected separately.
    pub(super) fn mark_link_lost(&mut self, session: SessionId, reason: Option<&str>) {
        self.cancel_add_children(session);
        self.clear_pending_requests(session);
        if let Some(view) = self.sessions.iter_mut().find(|view| view.id == session) {
            view.state = SessionState::Disconnected;
            if let Some(reason) = reason {
                view.info = reason.to_owned();
            }
            view.read_in_flight = false;
            view.root_loading = false;
            if let Some(ua) = view.ua_mut() {
                ua.subscription = SubscriptionState::Inactive;
            }
            for item in &mut view.tree {
                item.loading = false;
            }
        }
        self.mark_session_rows_stale(session);
    }

    /// Mark every row of `session` as stale ("нет данных от сервера"): Bad
    /// quality with the localized stale text. Used whenever the link is lost
    /// (`Disconnected`, a manual disconnect, a failed connectivity test).
    pub(super) fn mark_session_rows_stale(&mut self, session: SessionId) {
        let text = tr(self.lang(), Tr::StaleData).to_string();
        for row in self.rows.iter_mut().filter(|r| r.session == session) {
            row.quality = RowQuality::Error(text.clone());
            row.pending_push = None;
        }
    }

    /// Cancel a confirmation bound to a connection that is being replaced.
    fn cancel_write_confirmation_for_session(&mut self, session: SessionId) {
        let cancel = self.dialogs.confirm.as_ref().is_some_and(|request|
            matches!(&request.action, ConfirmAction::Write(pending) if pending.session == session));
        if cancel {
            self.dialogs.confirm = None;
            let message = tr(self.lang(), Tr::ActionWriteConnectionChanged).to_string();
            self.dialogs.write.error = Some(message.clone());
            self.status.clone_from(&message);
            self.journal.error(message);
        }
    }

    /// Forget requests from the preceding connect/disconnect lifecycle.
    pub(super) fn clear_pending_requests(&mut self, session: SessionId) {
        self.cancel_write_confirmation_for_session(session);
        let lost_writes = self
            .engine
            .invalidate_session_requests(session)
            .into_iter()
            .filter(|operation| *operation == Operation::Write)
            .count();
        // A11: writes whose reply can no longer arrive count as failed, or the
        // batch counters would never reach zero.
        if lost_writes > 0 {
            self.journal.warn_l(Tr::WriteUnknownStatus, &[]);
            self.write_batch.lost(lost_writes);
            self.finish_write_batch_if_done();
        }
        // A «Добавить по имени» waiting for this session can never be answered.
        if self
            .dialogs
            .add_signal
            .resolving
            .is_some_and(|(sid, _)| sid == session)
        {
            self.dialogs.add_signal.resolving = None;
            self.dialogs.add_signal.error = Some(tr(self.lang(), Tr::NoConnection).to_string());
        }
    }

    #[cfg(feature = "test-support")]
    pub fn record_dispatches_for_test(&self) {
        self.engine.limit_dispatch_for_test(usize::MAX);
    }
    #[cfg(feature = "test-support")]
    pub fn dispatched_for_test(&self) -> Vec<crate::bff::model::Command> {
        self.engine.dispatched_for_test()
    }
    #[cfg(feature = "test-support")]
    pub fn gateway_for_test(&self) -> &opclook_engine::testing::TestGateway {
        self.engine.gateway_for_test()
    }
    #[cfg(feature = "test-support")]
    pub fn age_pending_requests_for_test(&mut self, session: impl Into<Owner>, secs: u64) {
        self.engine
            .age_requests_for_test(session, Duration::from_secs(secs));
    }
    #[cfg(feature = "test-support")]
    pub fn pending_request_ids_for_test(&self, session: impl Into<Owner>) -> Vec<RequestId> {
        self.engine.pending_ids_for_test(session)
    }
}

impl App {
    fn project_session(&mut self, id: SessionId) {
        let Some(runtime) = self.engine.session(id) else {
            return;
        };
        if let Some(view) = self.sessions.iter_mut().find(|view| view.id == id) {
            view.state = runtime.state;
            view.reconnect = runtime.reconnect;
            if let Some(ua) = view.ua_mut() {
                ua.subscription = runtime.subscription;
            }
            view.read_in_flight = runtime.read_in_flight;
            view.last_poll = runtime.last_poll;
            view.last_test = runtime.last_test;
        }
    }
    #[cfg(feature = "test-support")]
    fn sync_fixture_sessions(&mut self) {
        for view in &self.sessions {
            let mut runtime =
                opclook_engine::SessionRuntime::new(view.endpoint().clone(), Instant::now());
            runtime.state = view.state;
            runtime.reconnect = view.reconnect;
            if let Some(ua) = view.ua() {
                runtime.subscription = ua.subscription;
            }
            runtime.read_in_flight = view.read_in_flight;
            runtime.last_poll = view.last_poll;
            runtime.last_test = view.last_test;
            runtime.items = self
                .rows
                .iter()
                .filter(|row| row.session == view.id && view.state == SessionState::Connected)
                .map(|row| (row.key, row.poll_only))
                .collect();
            self.engine.seed_session_for_test(view.id, runtime);
        }
    }

    /// Test-only seeding: append a connected session view.
    #[cfg(feature = "test-support")]
    pub fn seed_session(&mut self, id: SessionId, endpoint: EndpointSpec) {
        let mut view = SessionView::new(id, endpoint, Lang::Ru);
        view.state = SessionState::Connected;
        view.info = tr(Lang::Ru, Tr::Ready).to_string();
        view.root_loaded = true;
        view.last_poll = Instant::now();
        view.last_test = Instant::now();
        self.sessions.push(view);
        self.active_session = Some(id);
        self.selected_tree = None;
        self.sync_fixture_sessions();
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
        view.invalidate_tree();
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
            .map(|s| s.endpoint().display_name())
            .unwrap_or_default();
        let key = self.next_key.expect("test row IDs exhausted");
        self.next_key = key.successor();
        self.push_row(Row {
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
            quality: RowQuality::Server(Quality {
                code: 0xC0,
                kind: QualityKind::Good,
                text: "GOOD".into(),
            }),
            timestamp: None,
            times: Default::default(),
            time_selection: Default::default(),
            last_read: None,
            description: String::new(),
            pending: false,
            pending_push: None,
            poll_only: false,
            changed_at: None,
            value_text_cache: Default::default(),
            timestamp_text_cache: Default::default(),
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
        if let Some(row) = self.row_position(key).and_then(|p| self.rows.get_mut(p)) {
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
                    format!(
                        "{} ({})",
                        s.endpoint().display_name(),
                        s.endpoint().protocol
                    ),
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
        let (status, error) = self.engine.da_route_status();
        let text = status.localized(self.lang()).to_string();
        match error {
            Some(error) => format!("{text} — {}", error.localized(self.lang())),
            None => text,
        }
    }

    fn route_options(&self) -> opclook_engine::RouteOptions {
        let adapter = self
            .network
            .lease
            .as_ref()
            .map(|lease| lease.adapter.clone());
        let local_addresses = self
            .network
            .adapters
            .iter()
            .flat_map(|adapter| adapter.addresses.iter().map(|address| address.ip))
            .collect();
        let timeout = std::time::Duration::from_secs(
            self.settings.view.options.operation_timeout_secs.max(30),
        );
        opclook_engine::RouteOptions {
            adapter,
            local_addresses,
            deadline: Instant::now()
                .checked_add(timeout)
                .unwrap_or_else(Instant::now),
        }
    }
    pub(super) fn send_da_prepared(
        &mut self,
        owner: impl Into<Owner>,
        kind: CommandKind,
        ctx: PendingCtx,
    ) -> Result<RequestId, DaRouteError> {
        let options = self.route_options();
        self.engine.prepare_da(owner, kind, ctx, options)
    }
    pub(super) fn release_da_routes_for_session(&mut self, session: SessionId) {
        self.engine.cancel_da_routes(session);
    }
    pub(super) fn release_da_discovery_route(&mut self, _host: &str) {
        // Correlated discovery completion releases exactly its own ticket in Engine::accept.
    }
    pub(super) fn release_da_discovery_routes(&mut self) {
        self.engine.cancel_da_routes(Owner::Discovery);
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
        self.engine.invalidate_session_requests(Owner::Discovery);
        let d = &mut self.dialogs.connect;
        d.open = true;
        d.pane = ConnectPane::for_protocol(d.pane.protocol());
        d.error = None;
        // Empty by design: no persistence restores host/port/URL into the
        // fields. The servers list is what the user picks from.
        d.ua_acquisition = Some(crate::bff::model::AcquisitionConfig::ua_default());
        d.da_acquisition = Some(opclook_engine::recommended_acquisition(Protocol::OpcDa));
        d.da_host.clear();
        d.da_manual.clear();
        // Keep previously discovered DA results and their source hosts
        // across close/reopen. A manual or explicit new search replaces them.
        d.da_pending = false;
        d.ua_search_host.clear();
        d.ua_search_port.clear();
        d.ua_lds_host = self.settings.ua.ua_lds_host.clone();
        d.ua_url.clear();
        d.ua_search = UaSearch::Idle;
        // Reuse discovered UA results across popup visits. A new search
        // explicitly replaces them; closing the popup does not.
        d.da_selected = None;
        d.da_auto_pending = 0;
        d.focus_pending = true;
        d.ua_user.clear();
        d.ua_password.clear();
        d.ua_security_policy = self.settings.ua.ua_security_policy;
        d.ua_security_mode = self.settings.ua.ua_security_mode;
        // U13: trust is opt-in per connection; the legacy global flag is never
        // re-applied (recent endpoints carry their own).
        d.ua_trust_server_cert = crate::settings::default_trust_server_cert();
        d.ua_identity = UaIdentity::Anonymous;
        d.ua_session_timeout = self.settings.ua.ua_session_timeout_ms;
        d.ua_scan_checked = 0;
        d.ua_scan_total = 0;
        d.ua_scan_found = 0;
        d.ua_scan_error = None;
        // Reopening a populated list must not restart network discovery.
        if d.da_servers.is_empty() {
            self.discover_da_all();
        }
    }

    /// Close the connect popup and invalidate all search results still in
    /// flight. Network settings themselves are applied immediately and are not
    /// rolled back by closing the dialog.
    pub fn close_connect_dialog(&mut self) {
        self.cancel_ua_subnet_scan();
        self.engine.invalidate_session_requests(Owner::Discovery);
        // D5: replies of an abandoned DA search are dropped now, so nothing
        // would ever release their temporary discovery routes later.
        self.release_da_discovery_routes();
        // «Отмена» during a connect attempt aborts the wait: the half-open
        // session is disconnected (reconnect stopped) and stays in the list,
        // where it can be reconnected or removed.
        if let Some(session) = self.dialogs.connect.connecting.take() {
            let still_connecting = self
                .sessions
                .iter()
                .any(|s| s.id == session && s.state == SessionState::Connecting);
            if still_connecting {
                self.disconnect_session(session);
            }
        }
        let d = &mut self.dialogs.connect;
        d.open = false;
        d.ua_search = UaSearch::Idle;
        d.da_auto_pending = 0;
        d.da_pending = false;
    }

    pub fn open_options_dialog(&mut self) {
        self.dialogs.options.draft = self.settings.view.options.clone();
        self.dialogs.options.open = true;
        self.dialogs.options.focus_pending = true;
    }

    pub fn apply_options(&mut self) {
        let draft = clamp_options(self.dialogs.options.draft.clone());
        self.settings.view.options = draft;
        self.persist_settings();
        self.dialogs.options.open = false;
        // Reconfigure UA push subscriptions so a newly created subscription
        // honors the updated interval / deadband. Only already-connected
        // sessions have a worker-side subscription to configure; a disconnected
        // UA session gets fresh config on its next Connected.
        let ua_session_ids: Vec<(SessionId, Protocol)> = self
            .sessions
            .iter()
            .filter(|s| {
                s.protocol() == Protocol::OpcUa
                    && s.state == SessionState::Connected
                    && s.endpoint().acquisition.is_none()
            })
            .map(|s| (s.id, s.protocol()))
            .collect();
        for (session, protocol) in ua_session_ids {
            self.send(
                session,
                protocol,
                CommandKind::ConfigureUaSubscription {
                    publishing_interval_ms: self
                        .settings
                        .view
                        .options
                        .update_interval_secs
                        .max(1)
                        .saturating_mul(1000),
                    deadband: self.settings.view.options.ua_deadband,
                },
            );
        }
        self.journal.info_l(Tr::OptionsSaved, &[]);
    }

    pub fn save_script(&mut self) {
        self.settings.view.script = self.script_text.clone();
        if self.persist_settings().is_none() {
            self.journal.info_l(Tr::ScriptSaved, &[]);
        }
    }

    pub fn script_protocol_note(&self) -> String {
        tr(self.lang(), Tr::ScriptNotImplNote).to_string()
    }
}

/// Clamp a draft `Options` into its valid, UI-backed ranges before persisting.
fn clamp_options(draft: Options) -> Options {
    draft.clamped()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.update(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let save_error = self.save_settings();
        let sessions: Vec<(SessionId, Protocol)> = self
            .sessions
            .iter()
            .map(|session| (session.id, session.protocol()))
            .collect();
        for (session, protocol) in sessions {
            let _ = self.engine.disconnect_untracked(session, protocol);
        }
        let outcome = self.engine.shutdown();
        self.drain_read_observations();
        let errors = exit_messages(save_error, self.lang(), outcome);
        if !errors.is_empty() {
            rfd::MessageDialog::new()
                .set_title("OPCLook")
                .set_description(errors.join("\n"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }
}

fn exit_messages(
    save_error: Option<String>,
    lang: opclook_locale::Lang,
    outcome: opclook_contract::model::GatewayShutdown,
) -> Vec<String> {
    save_error
        .into_iter()
        .chain(shutdown_messages(lang, outcome))
        .collect()
}

fn shutdown_messages(
    lang: opclook_locale::Lang,
    outcome: opclook_contract::model::GatewayShutdown,
) -> Vec<String> {
    [("DA", outcome.da), ("UA", outcome.ua)]
        .into_iter()
        .filter_map(|(protocol, result)| {
            opclook_locale::shutdown_error_text(lang, protocol, result)
        })
        .collect()
}
