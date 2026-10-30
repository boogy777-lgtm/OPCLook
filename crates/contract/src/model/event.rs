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
