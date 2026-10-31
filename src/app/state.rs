use super::present::compose_server_name;
use crate::bff::model::{
    BrowseNode, EndpointSpec, ItemKey, NodeClassFilter, Protocol, Quality, QualityKind, RequestId,
    SessionId, SubscriptionState, Value, VarType,
};
use crate::i18n::{Lang, Tr, tr};
use chrono::{DateTime, Local, Utc};
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub use opclook_engine::{ReconnectState, SessionState};

/// Which of the bottom panels is currently displayed as a tab. Only meaningful
/// when both `show_script` and `show_log` are enabled; otherwise the single
/// enabled panel is shown directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomTab {
    Script,
    Journal,
}

/// A parsed CSV row awaiting import (server + signal identity).
#[derive(Clone, Debug)]
pub struct CsvImportRow {
    pub line: usize,
    pub server: String,
    pub item_id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsvImportState {
    Running,
    Cancelled,
    Completed,
}

/// Modal CSV import progress job (TFormLoad-style).
#[derive(Clone, Debug)]
pub struct CsvImportJob {
    pub state: CsvImportState,
    pub total: usize,
    pub processed: usize,
    pub loaded: usize,
    /// Rows not yet dispatched to the gateway.
    pub remaining: Vec<CsvImportRow>,
}

/// Recursive «Добавить дочерние» job: browsed branch -> every descendant Item,
/// walking Branch children depth-first. One job per session (a new one cancels
/// the previous). Driven by the existing `Browse` events.
#[derive(Clone, Debug)]
pub struct AddChildrenJob {
    /// Session whose tree is being walked.
    pub session: SessionId,
    pub epoch: Option<RequestId>,
    pub initial_item_id: Option<u64>,
    /// Branch `node_id`s still to browse (BFS/DFS queue).
    pub queue: std::collections::VecDeque<String>,
    /// Branch `node_id`s already queued/browsed (guards cycles and duplicates).
    pub visited: HashSet<String>,
    /// Browses the job itself issued and is still awaiting: `request_id ->
    /// node_id`. Only a `Browse` event whose request id is in this map may
    /// advance the job, so a manual user browse (or a stale reply from an
    /// earlier job) is never consumed as a job page.
    pub in_flight: std::collections::HashMap<RequestId, String>,
    /// Number of signals actually added to the table so far.
    pub added: usize,
    /// Node cap: stop enqueueing below this many visited branches.
    pub cap: usize,
    /// Whether the cap was already hit (so we journal the warning only once).
    pub capped: bool,
}

pub struct TreeItem {
    pub id: u64,
    pub parent: Option<u64>,
    pub node: BrowseNode,
    pub depth: usize,
    pub expanded: bool,
    pub loaded: bool,
    pub loading: bool,
}

/// State that exists only for UA sessions.
pub struct UaSessionView {
    /// Selected server View; None browses the default view.
    pub ua_view_id: Option<String>,
    /// Restriction applied to subsequent browse requests.
    pub ua_browse_filter: NodeClassFilter,
    /// Requested timeout; the server-revised value is not exposed by async-opcua.
    pub ua_session_timeout_ms: u32,
    /// Projected from Engine; only Active suppresses normal polling.
    pub subscription: SubscriptionState,
}

impl Default for UaSessionView {
    fn default() -> Self {
        Self {
            ua_view_id: None,
            ua_browse_filter: NodeClassFilter::All,
            ua_session_timeout_ms: crate::settings::default_session_timeout(),
            subscription: SubscriptionState::Inactive,
        }
    }
}

pub enum SessionKind {
    Da,
    Ua(UaSessionView),
}

impl SessionKind {
    fn for_protocol(protocol: Protocol) -> Self {
        match protocol {
            Protocol::OpcDa => Self::Da,
            Protocol::OpcUa => Self::Ua(UaSessionView::default()),
        }
    }
}

pub struct SessionView {
    pub id: SessionId,
    endpoint: EndpointSpec,
    /// None means no staged change; Some(None) stages the legacy profile.
    pub pending_acquisition: Option<Option<crate::bff::model::AcquisitionConfig>>,
    kind: SessionKind,
    /// Human-readable server name shown in the servers list and tab strip
    /// (OPC UA host:port or the advertised application name when known; for
    /// DA the ProgID/description). Refreshed on connect when available.
    pub display_name: String,
    pub state: SessionState,
    /// Whether this session container is shown as a tab / view. A hidden
    /// container survives in memory (rows + tree + address) but has no tab;
    /// it is restored via double-click in the connect dialog. Disconnecting
    /// never changes this flag — closing a tab is not disconnecting.
    pub visible: bool,
    pub info: String,
    pub(super) tree: Vec<TreeItem>,
    tree_visible_cache: std::cell::RefCell<Option<std::rc::Rc<Vec<usize>>>>,
    pub next_tree_id: u64,
    pub root_loaded: bool,
    pub root_loading: bool,
    pub read_in_flight: bool,
    pub last_poll: Instant,
    pub last_test: Instant,
    /// Automatic-reconnect state (Idle / Waiting / Stopped / Exhausted).
    pub reconnect: ReconnectState,
}

impl SessionView {
    pub(super) fn new(id: SessionId, endpoint: EndpointSpec, lang: Lang) -> Self {
        let now = Instant::now();
        let display_name = compose_server_name(&endpoint);
        Self {
            id,
            kind: SessionKind::for_protocol(endpoint.protocol),
            endpoint,
            pending_acquisition: None,
            display_name,
            state: SessionState::Connecting,
            visible: true,
            info: tr(lang, Tr::Connecting).to_string(),
            tree: Vec::new(),
            tree_visible_cache: std::cell::RefCell::new(None),
            next_tree_id: 1,
            root_loaded: false,
            root_loading: false,
            read_in_flight: false,
            last_poll: now.checked_sub(Duration::from_secs(10)).unwrap_or(now),
            last_test: now,
            reconnect: ReconnectState::Idle,
        }
    }

    pub fn tree(&self) -> &[TreeItem] {
        &self.tree
    }

    #[cfg(feature = "test-support")]
    pub fn tree_mut_for_test(&mut self) -> &mut Vec<TreeItem> {
        self.invalidate_tree();
        &mut self.tree
    }

    pub(crate) fn invalidate_tree(&mut self) {
        *self.tree_visible_cache.get_mut() = None;
    }

    pub(crate) fn clear_tree(&mut self) {
        self.tree.clear();
        self.invalidate_tree();
    }

    pub(crate) fn toggle_tree(&mut self, id: u64) -> bool {
        let Some(item) = self.tree.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.expanded = !item.expanded;
        let browse = item.expanded && !item.loaded && item.node.has_children;
        if browse {
            item.loading = true;
        }
        self.invalidate_tree();
        browse
    }

    pub(crate) fn visible_tree_indices(&self) -> std::rc::Rc<Vec<usize>> {
        if let Some(indices) = &*self.tree_visible_cache.borrow() {
            return indices.clone();
        }
        let mut indices = Vec::new();
        let mut hidden = None;
        for (index, item) in self.tree.iter().enumerate() {
            if let Some(depth) = hidden {
                if item.depth > depth {
                    continue;
                }
                hidden = None;
            }
            indices.push(index);
            if !item.expanded {
                hidden = Some(item.depth);
            }
        }
        let indices = std::rc::Rc::new(indices);
        *self.tree_visible_cache.borrow_mut() = Some(indices.clone());
        indices
    }

    pub fn protocol(&self) -> Protocol {
        match self.kind {
            SessionKind::Da => Protocol::OpcDa,
            SessionKind::Ua(_) => Protocol::OpcUa,
        }
    }

    pub fn endpoint(&self) -> &EndpointSpec {
        &self.endpoint
    }

    /// Keep UA selection on metadata changes; reset it when protocol changes.
    pub fn update_endpoint(&mut self, endpoint: EndpointSpec) {
        if self.protocol() != endpoint.protocol {
            self.kind = SessionKind::for_protocol(endpoint.protocol);
        }
        self.endpoint = endpoint;
    }

    pub fn ua(&self) -> Option<&UaSessionView> {
        match &self.kind {
            SessionKind::Ua(ua) => Some(ua),
            SessionKind::Da => None,
        }
    }

    pub fn ua_mut(&mut self) -> Option<&mut UaSessionView> {
        match &mut self.kind {
            SessionKind::Ua(ua) => Some(ua),
            SessionKind::Da => None,
        }
    }
}

/// Display state is separate from server quality codes, including vendor codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowQuality {
    Server(Quality),
    NoData(String),
    Waiting(String),
    Error(String),
}

impl RowQuality {
    pub fn kind(&self) -> QualityKind {
        match self {
            Self::Server(quality) => quality.kind,
            Self::Waiting(_) => QualityKind::Good,
            Self::NoData(_) | Self::Error(_) => QualityKind::Bad,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Self::Server(quality) => &quality.text,
            Self::NoData(text) | Self::Waiting(text) | Self::Error(text) => text,
        }
    }
}

pub struct Row {
    pub key: ItemKey,
    pub session: SessionId,
    pub item_id: String,
    pub display_name: String,
    pub server_label: String,
    pub data_type: Option<String>,
    pub canonical_vt: Option<VarType>,
    pub write_type: Option<VarType>,
    pub writable: bool,
    pub value: Value,
    pub quality: RowQuality,
    pub timestamp: Option<DateTime<Utc>>,
    pub times: crate::bff::model::ValueTimes,
    pub time_selection: super::value_time::TimeSelection,
    /// Latest per-item read lifecycle observation for the current connection.
    pub last_read: Option<opclook_contract::read_observer::ReadObservation>,
    pub description: String,
    pub pending: bool,
    /// Latest subscription value awaiting successful registration; never shown
    /// as a registered value until the corresponding AddItems succeeds.
    pub(super) pending_push: Option<crate::bff::model::DataValue>,
    /// P1 (UA): no subscription takes this item (server limits), so it is
    /// polled even while the session's push subscription is active.
    pub poll_only: bool,
    /// UI4: when the shown value last CHANGED (not merely arrived again); the
    /// table lights the value cell up for a moment after it.
    pub changed_at: Option<Instant>,
    pub(super) value_text_cache: std::cell::RefCell<Option<String>>,
    pub(super) timestamp_text_cache: std::cell::RefCell<Option<String>>,
}

impl Row {
    pub fn write_type_text(&self) -> std::borrow::Cow<'_, str> {
        if let Some(vt) = self.write_type.or(self.canonical_vt) {
            std::borrow::Cow::Borrowed(crate::bff::da_variant::vt_name(vt))
        } else if let Some(data_type) = &self.data_type {
            std::borrow::Cow::Borrowed(data_type)
        } else {
            std::borrow::Cow::Borrowed("Auto")
        }
    }

    pub(super) fn invalidate_display(&mut self) {
        *self.value_text_cache.get_mut() = None;
        *self.timestamp_text_cache.get_mut() = None;
    }

    pub fn value_text(&self) -> String {
        let mut cached = self.value_text_cache.borrow_mut();
        cached.get_or_insert_with(|| self.value.to_string()).clone()
    }

    /// Tooltip exposes raw clock facts and avoids implying a global order.
    pub fn time_details(&self, lang: Lang) -> String {
        let show = |time: Option<DateTime<Utc>>| time.map(|v| v.to_rfc3339()).unwrap_or_default();
        let origin = match self.times.origin {
            crate::bff::model::ValueOrigin::Unknown => Tr::TimeOriginUnknown,
            crate::bff::model::ValueOrigin::Read => Tr::TimeOriginRead,
            crate::bff::model::ValueOrigin::Subscription => Tr::TimeOriginSubscription,
        };
        let order = match self.time_selection.order {
            super::value_time::TimeOrder::Unknown => Tr::TimeOrderUnknown,
            super::value_time::TimeOrder::Comparable => Tr::TimeOrderComparable,
            super::value_time::TimeOrder::OlderHeld => Tr::TimeOrderOlderHeld,
            super::value_time::TimeOrder::EqualConflict => Tr::TimeOrderEqualConflict,
            super::value_time::TimeOrder::Discontinuity => Tr::TimeOrderDiscontinuity,
            super::value_time::TimeOrder::Diagnostic => Tr::TimeOrderDiagnostic,
        };
        format!(
            "{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}",
            tr(lang, Tr::TimeSource),
            show(self.times.ua_source),
            tr(lang, Tr::TimeServer),
            show(self.times.ua_server),
            tr(lang, Tr::TimeDa),
            show(self.times.da_timestamp),
            tr(lang, Tr::TimeReceived),
            show(self.times.received_at),
            tr(lang, Tr::TimeOrigin),
            tr(lang, origin),
            tr(lang, order)
        )
    }

    pub fn timestamp_text(&self) -> String {
        let mut cached = self.timestamp_text_cache.borrow_mut();
        cached
            .get_or_insert_with(|| {
                self.timestamp
                    .map(|t| {
                        t.with_timezone(&Local)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                    })
                    .unwrap_or_default()
            })
            .clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteKind {
    Auto,
    Bool,
    I64,
    U64,
    F64,
    Str,
    DateTime,
}

#[derive(Clone, Copy)]
pub struct WriteTypeChoice {
    pub label: &'static str,
    pub vt: Option<VarType>,
    pub kind: WriteKind,
}

pub fn write_type_choices(protocol: Protocol) -> Vec<WriteTypeChoice> {
    match protocol {
        Protocol::OpcDa => vec![
            WriteTypeChoice {
                label: "Auto",
                vt: None,
                kind: WriteKind::Auto,
            },
            WriteTypeChoice {
                label: "VT_BOOL",
                vt: Some(VarType::BOOL),
                kind: WriteKind::Bool,
            },
            WriteTypeChoice {
                label: "VT_I2",
                vt: Some(VarType::I2),
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "VT_I4",
                vt: Some(VarType::I4),
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "VT_R4",
                vt: Some(VarType::R4),
                kind: WriteKind::F64,
            },
            WriteTypeChoice {
                label: "VT_R8",
                vt: Some(VarType::R8),
                kind: WriteKind::F64,
            },
            WriteTypeChoice {
                label: "VT_BSTR",
                vt: Some(VarType::BSTR),
                kind: WriteKind::Str,
            },
            WriteTypeChoice {
                label: "VT_DATE",
                vt: Some(VarType::DATE),
                kind: WriteKind::DateTime,
            },
            WriteTypeChoice {
                label: "VT_I8",
                vt: Some(VarType::I8),
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "VT_UI8",
                vt: Some(VarType::UI8),
                kind: WriteKind::U64,
            },
        ],
        Protocol::OpcUa => vec![
            WriteTypeChoice {
                label: "Auto",
                vt: None,
                kind: WriteKind::Auto,
            },
            WriteTypeChoice {
                label: "Boolean",
                vt: None,
                kind: WriteKind::Bool,
            },
            WriteTypeChoice {
                label: "Int64",
                vt: None,
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "UInt64",
                vt: None,
                kind: WriteKind::U64,
            },
            WriteTypeChoice {
                label: "Double",
                vt: None,
                kind: WriteKind::F64,
            },
            WriteTypeChoice {
                label: "String",
                vt: None,
                kind: WriteKind::Str,
            },
            WriteTypeChoice {
                label: "DateTime",
                vt: None,
                kind: WriteKind::DateTime,
            },
        ],
    }
}
