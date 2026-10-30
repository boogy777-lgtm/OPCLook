use super::{
    BackendMsg, BrowseNode, DataValue, EndpointSpec, ItemInfo, ItemKey, Operation, Origin, Owner,
    ScanId, ServerEntry, UaEndpointInfo, UaNetworkInfo, UaServerInfo,
};
use serde::{Deserialize, Serialize};

/// App-side state of an OPC UA push subscription.
///
/// Only `Active` suppresses the periodic `Read`; `Inactive` (never created /
/// torn down) and `Failed` (creation or monitoring failed) both fall back to
/// polling so values keep flowing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubscriptionState {
    #[default]
    Inactive,
    Active,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Event {
    /// Correlation with a tracked command, or an unsolicited update.
    pub origin: Origin,
    pub owner: Owner,
    pub kind: EventKind,
}

/// Requested and server-revised parameters for one monitored UA item.
#[derive(Clone, Debug, PartialEq)]
pub struct UaMonitoredParameters {
    pub key: ItemKey,
    pub requested_sampling_ms: f64,
    pub requested_queue_size: u32,
    /// Server result retained verbatim by the SDK; -1 is a default sentinel.
    pub revised_sampling_ms: f64,
    pub revised_queue_size: usize,
}

/// Snapshot of the SDK's negotiated subscription state. Missing revised state
/// means that the SDK no longer has this subscription; it is never guessed.
#[derive(Clone, Debug, PartialEq)]
pub struct UaSubscriptionParameters {
    pub subscription_id: u32,
    pub requested_publishing_ms: u64,
    pub requested_lifetime: u32,
    pub requested_keepalive: u32,
    pub revised: Option<UaRevisedSubscription>,
    pub items: Vec<UaMonitoredParameters>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UaRevisedSubscription {
    pub publishing_ms: u64,
    pub lifetime: u32,
    pub keepalive: u32,
    /// False when the SDK value is outside OPCLook's bounded scheduling
    /// range or the lifetime ratio is invalid; this is not a protocol limit.
    pub valid: bool,
}

impl UaRevisedSubscription {
    /// Validate the SDK-normalized values without overflowing the UA ratio.
    pub fn from_sdk(publishing_ms: u64, lifetime: u32, keepalive: u32) -> Self {
        let valid = (1..=super::ACQUISITION_INTERVAL_MAX_MS).contains(&publishing_ms)
            && keepalive > 0
            && keepalive
                .checked_mul(3)
                .is_some_and(|minimum| lifetime >= minimum);
        Self {
            publishing_ms,
            lifetime,
            keepalive,
            valid,
        }
    }
}

/// Terminal outcome of a subnet scan. Partial discoveries are reported separately.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanEnd {
    Completed,
    Cancelled,
    Failed(BackendMsg),
}

#[derive(Clone, Debug)]
pub enum EventKind {
    Servers {
        host: String,
        servers: Vec<ServerEntry>,
    },
    UaServersFound {
        host: String,
        port: u16,
        servers: Vec<UaServerInfo>,
    },
    UaNetworkFound {
        servers: Vec<UaNetworkInfo>,
    },
    /// Live progress of a running N+1 subnet scan (streaming, `Origin::Unsolicited`).
    UaScanProgress {
        scan_id: ScanId,
        total: usize,
        checked: usize,
        found: usize,
    },
    /// Newly discovered servers from a running subnet scan (one or more batches).
    UaScanFound {
        scan_id: ScanId,
        servers: Vec<UaServerInfo>,
    },
    /// Terminal event of a subnet scan; `found` includes partial results.
    UaScanFinished {
        scan_id: ScanId,
        found: usize,
        end: ScanEnd,
    },
    UaEndpointsFound {
        url: String,
        endpoints: Vec<UaEndpointInfo>,
    },
    Connected {
        endpoint: EndpointSpec,
        info: BackendMsg,
    },
    AddItems {
        results: Vec<(ItemKey, Result<ItemInfo, BackendMsg>)>,
    },
    Removed {
        keys: Vec<ItemKey>,
    },
    Browse {
        node_id: String,
        children: Vec<BrowseNode>,
    },
    Views {
        children: Vec<BrowseNode>,
    },
    PathResolved {
        targets: Vec<String>,
    },
    Values {
        values: Vec<(ItemKey, DataValue)>,
    },
    /// P1 (UA): these items are registered but no subscription takes them
    /// (server limits), so the app must keep reading them by polling.
    PollOnly {
        keys: Vec<ItemKey>,
    },
    /// UA-only: current push-subscription state. `Active` stops the periodic
    /// Read for that UA session (values arrive via DataChange push); `Inactive`
    /// and `Failed` both keep polling.
    SubscriptionState {
        state: SubscriptionState,
    },
    /// Complete SDK snapshot for this connection epoch.
    UaSubscriptionParameters {
        subscriptions: Vec<UaSubscriptionParameters>,
    },
    Wrote {
        key: ItemKey,
        /// `Err` means validation or routing failed before the native Write was submitted.
        result: Result<super::WriteOutcome, BackendMsg>,
    },
    Disconnected {
        reason: BackendMsg,
    },
    Log {
        message: BackendMsg,
    },
    Error {
        operation: Operation,
        message: BackendMsg,
    },
}
