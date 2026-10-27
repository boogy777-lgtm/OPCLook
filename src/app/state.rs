use super::present::compose_server_name;
use crate::bff::model::{
    BrowseNode, EndpointSpec, ItemKey, Protocol, Quality, RequestId, SessionId, SubscriptionState,
    Value,
};
use crate::i18n::{Lang, Tr, tr};
use chrono::{DateTime, Local, Utc};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Connecting,
    Connected,
    Disconnected,
}

/// Automatic-reconnect state machine (one enum instead of five booleans).
///
/// `Waiting` carries both the scheduled instant and the 1-based attempt
/// number. It stays `Waiting` while the resulting `Connect` is in flight (the
/// session state is `Connecting` then) so the counter keeps showing the attempt
/// that is being made; a failure schedules the next `Waiting`, a success moves
/// to `Idle`. The attempt cap is the shared `RECONNECT_MAX_ATTEMPTS` constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconnectState {
    Idle,
    Waiting { at: Instant, attempt: u32 },
    Stopped,
    Exhausted,
}

impl ReconnectState {
    pub fn attempt(self) -> u32 {
        match self {
            ReconnectState::Waiting { attempt, .. } => attempt,
            _ => 0,
        }
    }
}

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
    pub server: String,
    pub item_id: String,
    pub name: String,
}

/// Modal CSV import progress job (TFormLoad-style).
#[derive(Clone, Debug)]
pub struct CsvImportJob {
    pub open: bool,
    pub cancelled: bool,
    pub done: bool,
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

pub struct SessionView {
    pub id: SessionId,
    pub endpoint: EndpointSpec,
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
    pub tree: Vec<TreeItem>,
    pub next_tree_id: u64,
    pub root_loaded: bool,
    pub root_loading: bool,
    pub read_in_flight: bool,
    pub last_poll: Instant,
    pub last_test: Instant,
    /// Automatic-reconnect state (Idle / Waiting / Stopped / Exhausted).
    pub reconnect: ReconnectState,
    /// UA-only: the selected server View (NodeId text) to browse through.
    /// `None` browses the default view.
    pub ua_view_id: Option<String>,
    /// UA-only: restrict future browses to Variable nodes.
    pub ua_browse_vars_only: bool,
    /// UA-only: requested session timeout in ms (displayed in the status popup;
    /// the revised server value is not exposed by async-opcua, so we show the
    /// requested one).
    pub ua_session_timeout_ms: u32,
    /// UA-only: push-subscription state. Only `Active` skips the periodic Read;
    /// `Inactive`/`Failed` keep polling. Meaningless for DA sessions.
    pub subscription: SubscriptionState,
}

impl SessionView {
    pub(super) fn new(id: SessionId, endpoint: EndpointSpec, lang: Lang) -> Self {
        let now = Instant::now();
        let display_name = compose_server_name(&endpoint);
        Self {
            id,
            endpoint,
            display_name,
            state: SessionState::Connecting,
            visible: true,
            info: tr(lang, Tr::Connecting).to_string(),
            tree: Vec::new(),
            next_tree_id: 1,
            root_loaded: false,
            root_loading: false,
            read_in_flight: false,
            last_poll: now.checked_sub(Duration::from_secs(10)).unwrap_or(now),
            last_test: now,
            reconnect: ReconnectState::Idle,
            ua_view_id: None,
            ua_browse_vars_only: false,
            ua_session_timeout_ms: crate::settings::default_session_timeout(),
            subscription: SubscriptionState::Inactive,
        }
    }

    pub fn protocol(&self) -> Protocol {
        self.endpoint.protocol
    }
}

pub struct Row {
    pub key: ItemKey,
    pub session: SessionId,
    pub item_id: String,
    pub display_name: String,
    pub server_label: String,
    pub data_type: Option<String>,
    pub canonical_vt: Option<u16>,
    pub write_type: Option<u16>,
    pub writable: bool,
    pub value: Value,
    pub quality: Quality,
    pub timestamp: Option<DateTime<Utc>>,
    pub description: String,
    pub pending: bool,
    /// UI4: when the shown value last CHANGED (not merely arrived again); the
    /// table lights the value cell up for a moment after it.
    pub changed_at: Option<Instant>,
}

impl Row {
    pub fn write_type_text(&self) -> String {
        if let Some(vt) = self.write_type.or(self.canonical_vt) {
            crate::bff::da_variant::vt_name(vt).to_string()
        } else if let Some(data_type) = &self.data_type {
            data_type.clone()
        } else {
            "Auto".to_string()
        }
    }

    pub fn timestamp_text(&self) -> String {
        self.timestamp
            .map(|t| {
                t.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default()
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
    pub vt: Option<u16>,
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
                vt: Some(11),
                kind: WriteKind::Bool,
            },
            WriteTypeChoice {
                label: "VT_I2",
                vt: Some(2),
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "VT_I4",
                vt: Some(3),
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "VT_R4",
                vt: Some(4),
                kind: WriteKind::F64,
            },
            WriteTypeChoice {
                label: "VT_R8",
                vt: Some(5),
                kind: WriteKind::F64,
            },
            WriteTypeChoice {
                label: "VT_BSTR",
                vt: Some(8),
                kind: WriteKind::Str,
            },
            WriteTypeChoice {
                label: "VT_DATE",
                vt: Some(7),
                kind: WriteKind::DateTime,
            },
            WriteTypeChoice {
                label: "VT_I8",
                vt: Some(20),
                kind: WriteKind::I64,
            },
            WriteTypeChoice {
                label: "VT_UI8",
                vt: Some(21),
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
