use super::{EndpointSpec, ItemKey, Operation, Owner, Protocol, RequestId, ScanId, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Branch,
    Item,
}

/// Classes returned by a browse command. Variables includes folders so that
/// variables remain reachable through the UA hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeClassFilter {
    All,
    Objects,
    Variables,
}

#[derive(Clone, Debug)]
pub struct BrowseNode {
    /// DA: branch path encoded with 0x1F separators or fully-qualified item id.
    /// UA: NodeId as text.
    pub node_id: String,
    pub name: String,
    pub kind: NodeKind,
    pub has_children: bool,
}

impl BrowseNode {
    /// Internal node id reserved for the synthetic UA Views root.
    pub const VIEWS_ROOT_ID: &'static str = "\u{1}views";

    pub fn is_views_root(&self) -> bool {
        self.node_id == Self::VIEWS_ROOT_ID
    }
}

#[derive(Clone, Debug)]
pub struct ItemSpec {
    pub key: ItemKey,
    pub item_id: String,
}

/// OPC DA VARIANT type, including any modifier bits returned by a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VarType(pub u16);

impl VarType {
    pub const EMPTY: Self = Self(0);
    pub const NULL: Self = Self(1);
    pub const I2: Self = Self(2);
    pub const I4: Self = Self(3);
    pub const R4: Self = Self(4);
    pub const R8: Self = Self(5);
    pub const CY: Self = Self(6);
    pub const DATE: Self = Self(7);
    pub const BSTR: Self = Self(8);
    pub const DISPATCH: Self = Self(9);
    pub const ERROR: Self = Self(10);
    pub const BOOL: Self = Self(11);
    pub const VARIANT: Self = Self(12);
    pub const UNKNOWN: Self = Self(13);
    pub const DECIMAL: Self = Self(14);
    pub const I1: Self = Self(16);
    pub const UI1: Self = Self(17);
    pub const UI2: Self = Self(18);
    pub const UI4: Self = Self(19);
    pub const I8: Self = Self(20);
    pub const UI8: Self = Self(21);
    pub const INT: Self = Self(22);
    pub const UINT: Self = Self(23);
    pub const LPSTR: Self = Self(30);
    pub const LPWSTR: Self = Self(31);
}

#[derive(Clone, Debug, PartialEq)]
pub struct WriteRequest {
    pub key: ItemKey,
    pub value: Value,
    /// Optional OPC DA VARIANT type; UA writes do not use this hint.
    pub type_hint: Option<VarType>,
}

#[derive(Clone, Debug)]
pub struct Command {
    pub request_id: Option<RequestId>,
    pub owner: Owner,
    pub protocol: Protocol,
    pub kind: CommandKind,
}

/// A discovery server the user named explicitly (the dialog's "LDS host").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LdsTarget {
    pub host: String,
    pub port: u16,
    /// Binding for the request to this host (loopback for a local host).
    pub bind: crate::net::NetworkBinding,
}

impl LdsTarget {
    /// `opc.tcp://host:port` with an IPv6 literal bracketed.
    pub fn url(&self) -> String {
        let host = self.host.trim();
        if host.contains(':') && !host.starts_with('[') {
            format!("opc.tcp://[{host}]:{}", self.port)
        } else {
            format!("opc.tcp://{host}:{}", self.port)
        }
    }
}

#[derive(Clone, Debug)]
pub enum CommandKind {
    Discover {
        host: String,
    },
    SearchUaServers {
        host: String,
        port: u16,
        /// Transport binding for this search. `None` keeps the unbound
        /// `DirectConnectionSource` behavior.
        bind: Option<crate::net::NetworkBinding>,
    },
    /// Discover OPC UA servers advertized via `FindServersOnNetwork` (mDNS/LDS).
    /// Without `lds` the machine's standard discovery endpoints are asked; with
    /// it, that host's LDS is asked instead. Without `bind` and with `lds`
    /// only the LDS is asked (mDNS needs an adapter).
    SearchUaNetwork {
        bind: Option<crate::net::NetworkBinding>,
        lds: Option<LdsTarget>,
    },
    /// Streaming N+1 subnet scan: loopback first, then the local address and the
    /// subnet hosts. Results arrive as `UaScan*` events with `Origin::Unsolicited`.
    ScanUaNetwork {
        scan_id: ScanId,
        bind: crate::net::NetworkBinding,
        subnet: Option<crate::net::SubnetSpec>,
        local_ip: std::net::IpAddr,
    },
    /// Cooperative cancellation of a running subnet scan. The scan task sends
    /// its terminal `UaScanFinished` event on its own.
    CancelUaScan {
        scan_id: ScanId,
    },
    Connect {
        endpoint: EndpointSpec,
    },
    Disconnect,
    /// Browse the children of `node_id`.
    ///
    /// `filter` (UA only) restricts the node-class mask. Variables includes
    /// the Objects/folders leading to them (a mask of Variables alone
    /// would hide the folders and leave nothing to expand).
    ///
    /// `view_id` (UA only, None for DA) selects a server View node to browse
    /// through; `None` browses the default (Objects/value) view.
    Browse {
        node_id: String,
        filter: NodeClassFilter,
        view_id: Option<String>,
    },
    /// Lists the server's Views (UA only; DA reports an unsupported error).
    BrowseViews,
    /// Translate a relative BrowsePath starting at `start_node` (None = root)
    /// into target NodeIds, producing an `EventKind::PathResolved`.
    ResolveBrowsePath {
        start_node: Option<String>,
        path: String,
    },
    AddItems {
        items: Vec<ItemSpec>,
    },
    RemoveItems {
        keys: Vec<ItemKey>,
    },
    Read {
        keys: Vec<ItemKey>,
    },
    Write {
        request: WriteRequest,
    },
    /// UA-only: configure the session's push subscription before items are
    /// (re-)added. `publishing_interval_ms` mirrors the app's update interval;
    /// `deadband` is the absolute deadband (0 = off). Applied when the
    /// subscription is next created. Sent by the app on every (re)connect.
    ConfigureUaSubscription {
        publishing_interval_ms: u64,
        deadband: f64,
    },
    Test,
}

impl CommandKind {
    /// Operation identity of this command. Exhaustive so a new command cannot
    /// reach `App::send` without an error label for a failed dispatch.
    pub fn operation(&self) -> Operation {
        match self {
            CommandKind::Discover { .. } => Operation::Discover,
            CommandKind::SearchUaServers { .. } => Operation::SearchServers,
            CommandKind::SearchUaNetwork { .. } => Operation::SearchNetwork,
            CommandKind::ScanUaNetwork { .. } => Operation::SearchNetwork,
            CommandKind::CancelUaScan { .. } => Operation::SearchNetwork,
            CommandKind::Connect { .. } => Operation::Connect,
            CommandKind::Disconnect => Operation::Disconnect,
            CommandKind::Browse { .. } => Operation::Browse,
            CommandKind::BrowseViews => Operation::BrowseViews,
            CommandKind::ResolveBrowsePath { .. } => Operation::ResolveBrowsePath,
            CommandKind::AddItems { .. } => Operation::AddItems,
            CommandKind::RemoveItems { .. } => Operation::RemoveItems,
            CommandKind::Read { .. } => Operation::Read,
            CommandKind::Write { .. } => Operation::Write,
            CommandKind::ConfigureUaSubscription { .. } => Operation::Subscription,
            CommandKind::Test => Operation::Test,
        }
    }
}
