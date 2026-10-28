use crate::bff::model::{
    ItemKey, Protocol, RequestId, ScanId, ServerEntry, SessionId, UaSecurityMode, UaSecurityPolicy,
    WriteRequest,
};
use crate::settings::Options;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectPane {
    Da,
    Ua,
    Network { previous: Protocol },
}

impl ConnectPane {
    pub fn protocol(self) -> Protocol {
        match self {
            Self::Da => Protocol::OpcDa,
            Self::Ua => Protocol::OpcUa,
            Self::Network { previous } => previous,
        }
    }

    pub fn for_protocol(protocol: Protocol) -> Self {
        match protocol {
            Protocol::OpcDa => Self::Da,
            Protocol::OpcUa => Self::Ua,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UaIdentity {
    Anonymous,
    UserName,
    Certificate,
}

/// The active UA discovery operation. A combined search always has at least
/// one outstanding request; each request is correlated independently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UaSearch {
    Idle,
    SingleHost { request: RequestId },
    Network { request: RequestId },
    Auto { pending: AutoPending },
    ConfirmSubnet { count: u64 },
    Subnet { scan_id: ScanId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoPending {
    Host(RequestId),
    Network(RequestId),
    Both { host: RequestId, network: RequestId },
}

impl UaSearch {
    pub fn busy(&self) -> bool {
        !matches!(self, Self::Idle)
    }
    pub fn host_searching(&self) -> bool {
        matches!(self, Self::SingleHost { .. } | Self::Auto { .. })
    }
    pub fn network_searching(&self) -> bool {
        matches!(
            self,
            Self::Network { .. }
                | Self::Auto {
                    pending: AutoPending::Network(_) | AutoPending::Both { .. }
                }
        )
    }
    pub fn scan_id(&self) -> Option<ScanId> {
        match self {
            Self::Subnet { scan_id } => Some(*scan_id),
            _ => None,
        }
    }
    pub fn auto_pending_count(&self) -> usize {
        match self {
            Self::Auto {
                pending: AutoPending::Both { .. },
            } => 2,
            Self::Auto { .. } => 1,
            _ => 0,
        }
    }
    pub fn subnet_active(&self) -> bool {
        matches!(self, Self::Subnet { .. } | Self::ConfirmSubnet { .. })
    }
    pub fn confirmation_count(&self) -> Option<u64> {
        match self {
            Self::ConfirmSubnet { count } => Some(*count),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DaHit {
    pub server: ServerEntry,
    pub host: String,
}

pub struct ConnectDialog {
    pub open: bool,
    pub pane: ConnectPane,
    pub da_host: String,
    pub da_servers: Vec<DaHit>,
    pub da_selected: Option<DaHit>,
    pub da_manual: String,
    pub da_pending: bool,
    pub ua_url: String,
    pub ua_user: String,
    pub ua_password: String,
    pub error: Option<String>,
    /// OPC UA discovery search state.
    pub ua_search_host: String,
    pub ua_search_port: String,
    /// Optional LDS host (`host[:port]`) for the LDS/mDNS search.
    pub ua_lds_host: String,
    pub ua_search: UaSearch,
    pub(super) ua_search_results: Vec<crate::bff::model::UaServerInfo>,
    pub(crate) ua_results_revision: u64,
    pub(crate) ua_results_cache_id: u64,
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
    pub ua_identity: UaIdentity,
    /// UA: requested session timeout in milliseconds (range 5000..=3600000).
    pub ua_session_timeout: u32,
    /// OPC UA network discovery state (mDNS/LDS `FindServersOnNetwork`).
    pub ua_network_results: Vec<crate::bff::model::UaNetworkInfo>,
    /// Last subnet scan report, retained after completion or cancellation.
    pub ua_scan_checked: usize,
    pub ua_scan_total: usize,
    pub ua_scan_found: usize,
    pub ua_scan_error: Option<String>,
    /// Servers accumulated by the running scan, merged into
    /// `ua_search_results` on completion or cancellation.
    pub ua_scan_servers: Vec<crate::bff::model::UaServerInfo>,
    /// Outstanding sub-searches of the DA auto-discovery (local + known remote
    /// hosts). When it drops to zero the merged list is complete.
    pub da_auto_pending: usize,
    /// Session whose `Connect` attempt the dialog is waiting for. While `Some`
    /// the dialog stays open with a spinner; `Connected` closes it, `Error`
    /// shows the message inside it and «Отмена» aborts the wait (U9).
    pub connecting: Option<SessionId>,
}

static NEXT_UA_CACHE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl ConnectDialog {
    pub fn ua_search_results(&self) -> &[crate::bff::model::UaServerInfo] {
        &self.ua_search_results
    }
    pub(super) fn invalidate_ua_results(&mut self) {
        self.ua_results_revision = self.ua_results_revision.wrapping_add(1);
    }
    #[cfg(feature = "test-support")]
    pub fn ua_search_results_mut_for_test(&mut self) -> &mut Vec<crate::bff::model::UaServerInfo> {
        self.invalidate_ua_results();
        &mut self.ua_search_results
    }
}

impl Default for ConnectDialog {
    fn default() -> Self {
        Self {
            open: false,
            pane: ConnectPane::Da,
            da_host: String::new(),
            da_servers: Vec::new(),
            da_selected: None,
            da_manual: String::new(),
            da_pending: false,
            ua_url: String::new(),
            ua_user: String::new(),
            ua_password: String::new(),
            error: None,
            ua_search_host: String::new(),
            ua_search_port: String::new(),
            ua_lds_host: String::new(),
            ua_search: UaSearch::Idle,
            ua_search_results: Vec::new(),
            ua_results_revision: 0,
            ua_results_cache_id: NEXT_UA_CACHE_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ua_result_epoch: 0,
            focus_pending: false,
            ua_security_policy: UaSecurityPolicy::Auto,
            ua_security_mode: UaSecurityMode::Auto,
            ua_discovered_endpoints: Vec::new(),
            ua_trust_server_cert: crate::settings::default_trust_server_cert(),
            ua_identity: UaIdentity::Anonymous,
            ua_session_timeout: crate::settings::default_session_timeout(),
            ua_network_results: Vec::new(),
            ua_scan_checked: 0,
            ua_scan_total: 0,
            ua_scan_found: 0,
            ua_scan_error: None,
            ua_scan_servers: Vec::new(),
            da_auto_pending: 0,
            connecting: None,
        }
    }
}

#[derive(Default)]
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

#[derive(Default)]
pub struct WriteDialog {
    pub open: bool,
    pub keys: Vec<ItemKey>,
    pub text: String,
    pub type_index: usize,
    pub error: Option<String>,
    pub focus_pending: bool,
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
    pub(crate) certificate_subject: Option<Option<String>>,
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

pub struct ConfirmRequest {
    pub title: String,
    pub text: String,
    pub action: ConfirmAction,
}

pub struct Dialogs {
    pub connect: ConnectDialog,
    pub add_signal: AddSignalDialog,
    pub write: WriteDialog,
    pub columns: ColumnsDialog,
    pub options: OptionsDialog,
    pub about: AboutDialog,
    pub message: Option<MessageDialog>,
    pub confirm: Option<ConfirmRequest>,
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
            about: AboutDialog {
                open: false,
                certificate_subject: None,
            },
            message: None,
            confirm: None,
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
            || self.confirm.is_some()
    }
}
