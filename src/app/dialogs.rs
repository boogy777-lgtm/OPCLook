use crate::bff::model::{
    ItemKey, Protocol, RequestId, ServerEntry, SessionId, UaSecurityMode, UaSecurityPolicy,
    WriteRequest,
};
use crate::settings::Options;

pub struct ConnectDialog {
    pub open: bool,
    pub protocol: Protocol,
    /// Third popup tab: «Сетевые настройки». DA/UA rendering is unaffected when
    /// this is false.
    pub network_tab: bool,
    pub da_host: String,
    pub da_servers: Vec<ServerEntry>,
    /// Computer for each DA result, indexed in parallel with da_servers.
    pub da_server_hosts: Vec<String>,
    pub da_selected: Option<String>,
    pub da_selected_host: Option<String>,
    pub da_manual: String,
    pub da_pending: bool,
    pub ua_url: String,
    pub ua_user: String,
    pub ua_password: String,
    pub error: Option<String>,
    /// OPC UA discovery search state.
    pub ua_search_host: String,
    pub ua_search_port: String,
    pub ua_searching: bool,
    pub ua_search_results: Vec<crate::bff::model::UaServerInfo>,
    /// Changes the collapse-state identity when a new search begins.
    pub ua_result_epoch: u64,
    /// Request keyboard focus for the primary field on the next populated frame.
    pub focus_pending: bool,
    /// UA: chosen security policy (typed; `Auto` default).
    pub ua_security_policy: UaSecurityPolicy,
    /// UA: chosen message security mode (typed; `Auto` default).
    pub ua_security_mode: UaSecurityMode,
    /// UA: the endpoints discovered for the current host/port (populated after
    /// «Поиск» or when the host/port combo changes). Drives the two combos.
    pub ua_discovered_endpoints: Vec<crate::bff::model::UaEndpointInfo>,
    /// UA: trust the server certificate without importing it.
    pub ua_trust_server_cert: bool,
    /// UA: identity selector (0 = Anonymous, 1 = UserName, 2 = X509 «Сертификат»).
    pub ua_identity_index: usize,
    /// UA: requested session timeout in milliseconds (range 5000..=3600000).
    pub ua_session_timeout: u32,
    /// OPC UA network discovery state (mDNS/LDS `FindServersOnNetwork`).
    pub ua_network_searching: bool,
    pub ua_network_results: Vec<crate::bff::model::UaNetworkInfo>,
    /// OPC UA subnet scan (N+1) state. `ua_scan_id` is the id of the only scan
    /// whose streaming events are accepted; events from older scans are ignored.
    pub ua_subnet_searching: bool,
    pub ua_scan_id: Option<u64>,
    pub ua_scan_checked: usize,
    pub ua_scan_total: usize,
    pub ua_scan_found: usize,
    pub ua_scan_error: Option<String>,
    /// Pending explicit confirmation for an over-large subnet (address count).
    pub ua_scan_confirm: Option<u64>,
    /// Servers accumulated by the running scan, merged into
    /// `ua_search_results` on completion or cancellation.
    pub ua_scan_servers: Vec<crate::bff::model::UaServerInfo>,
    /// Outstanding sub-searches of a combined "empty host" UA auto-search
    /// (network + local hosts). When it drops to zero the partial server lists
    /// are merged into `ua_search_results`. Zero means no combined search runs.
    pub ua_auto_pending: usize,
    /// Outstanding sub-searches of the DA auto-discovery (local + known remote
    /// hosts). When it drops to zero the merged list is complete.
    pub da_auto_pending: usize,
    /// Session whose `Connect` attempt the dialog is waiting for. While `Some`
    /// the dialog stays open with a spinner; `Connected` closes it, `Error`
    /// shows the message inside it and «Отмена» aborts the wait (U9).
    pub connecting: Option<SessionId>,
}

impl Default for ConnectDialog {
    fn default() -> Self {
        Self {
            open: false,
            protocol: Protocol::OpcDa,
            network_tab: false,
            da_host: String::new(),
            da_servers: Vec::new(),
            da_server_hosts: Vec::new(),
            da_selected: None,
            da_selected_host: None,
            da_manual: String::new(),
            da_pending: false,
            ua_url: String::new(),
            ua_user: String::new(),
            ua_password: String::new(),
            error: None,
            ua_search_host: String::new(),
            ua_search_port: String::new(),
            ua_searching: false,
            ua_search_results: Vec::new(),
            ua_result_epoch: 0,
            focus_pending: false,
            ua_security_policy: UaSecurityPolicy::Auto,
            ua_security_mode: UaSecurityMode::Auto,
            ua_discovered_endpoints: Vec::new(),
            ua_trust_server_cert: crate::settings::default_trust_server_cert(),
            ua_identity_index: 0,
            ua_session_timeout: crate::settings::default_session_timeout(),
            ua_network_searching: false,
            ua_network_results: Vec::new(),
            ua_subnet_searching: false,
            ua_scan_id: None,
            ua_scan_checked: 0,
            ua_scan_total: 0,
            ua_scan_found: 0,
            ua_scan_error: None,
            ua_scan_confirm: None,
            ua_scan_servers: Vec::new(),
            ua_auto_pending: 0,
            da_auto_pending: 0,
            connecting: None,
        }
    }
}

pub struct AddSignalDialog {
    pub open: bool,
    pub session: Option<SessionId>,
    pub item_id: String,
    pub display_name: String,
    pub error: Option<String>,
    pub focus_pending: bool,
    /// UA «Добавить по имени»: the `(session, request_id)` of the
    /// `ResolveBrowsePath` the dialog is waiting for (U14). While `Some` the
    /// dialog stays open with a spinner; the answer closes it or shows its
    /// error inline.
    pub resolving: Option<(SessionId, RequestId)>,
}

impl Default for AddSignalDialog {
    fn default() -> Self {
        Self {
            open: false,
            session: None,
            item_id: String::new(),
            display_name: String::new(),
            error: None,
            focus_pending: false,
            resolving: None,
        }
    }
}

pub struct WriteDialog {
    pub open: bool,
    pub keys: Vec<ItemKey>,
    pub text: String,
    pub type_index: usize,
    pub error: Option<String>,
    pub focus_pending: bool,
}

impl Default for WriteDialog {
    fn default() -> Self {
        Self {
            open: false,
            keys: Vec::new(),
            text: String::new(),
            type_index: 0,
            error: None,
            focus_pending: false,
        }
    }
}

pub struct ColumnsDialog {
    pub open: bool,
    /// Last column moved by the dialog's «↑»/«↓» buttons and the wall-clock
    /// time (seconds) it was moved, for a brief highlight.
    pub last_moved: Option<(crate::columns::ColumnId, f64)>,
    /// The column that owns the DnD keyboard fallback (Alt/Ctrl+↑/↓). Updated
    /// when the user focuses/clicks a row; `None` falls back to the first row.
    pub focused: Option<crate::columns::ColumnId>,
}

pub struct OptionsDialog {
    pub open: bool,
    pub draft: Options,
    pub focus_pending: bool,
}

pub struct AboutDialog {
    pub open: bool,
}

pub struct MessageDialog {
    pub title: String,
    pub text: String,
    pub details: String,
}

/// A set of write requests awaiting explicit user confirmation (confirm_write).
#[derive(Clone, Debug)]
pub struct PendingWrites {
    pub session: SessionId,
    pub protocol: Protocol,
    pub requests: Vec<WriteRequest>,
    pub type_label: String,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Disconnect(SessionId),
    RemoveSession(SessionId),
    Write(PendingWrites),
}

pub struct ConfirmDialog {
    pub open: bool,
    pub title: String,
    pub text: String,
    pub action: Option<ConfirmAction>,
}

impl Default for ConfirmDialog {
    fn default() -> Self {
        Self {
            open: false,
            // Empty: the renderer fills in the localized `Tr::ConfirmTitle` when
            // no explicit title was requested.
            title: String::new(),
            text: String::new(),
            action: None,
        }
    }
}

pub struct Dialogs {
    pub connect: ConnectDialog,
    pub add_signal: AddSignalDialog,
    pub write: WriteDialog,
    pub columns: ColumnsDialog,
    pub options: OptionsDialog,
    pub about: AboutDialog,
    pub message: Option<MessageDialog>,
    pub confirm: ConfirmDialog,
}

impl Default for Dialogs {
    fn default() -> Self {
        Self {
            connect: ConnectDialog::default(),
            add_signal: AddSignalDialog::default(),
            write: WriteDialog::default(),
            columns: ColumnsDialog {
                open: false,
                last_moved: None,
                focused: None,
            },
            options: OptionsDialog {
                open: false,
                draft: Options::default(),
                focus_pending: false,
            },
            about: AboutDialog { open: false },
            message: None,
            confirm: ConfirmDialog::default(),
        }
    }
}

impl Dialogs {
    /// Whether any modal window is currently open. While a dialog is shown it
    /// owns focus (and hence the keyboard) for the application.
    pub fn any_open(&self) -> bool {
        self.connect.open
            || self.add_signal.open
            || self.write.open
            || self.columns.open
            || self.options.open
            || self.about.open
            || self.message.is_some()
            || self.confirm.open
    }
}
