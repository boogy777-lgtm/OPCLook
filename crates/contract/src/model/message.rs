use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical operation identity carried by `EventKind::Error`.
///
/// The app matches on this enum (not on a string), so every operation that can
/// fail has one producer and one consumer arm checked by the compiler. The
/// `as_str()` spellings are the stable tokens used by logs/tests; display text
/// stays in the i18n dictionary (`Tr::Op*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    Connect,
    Disconnect,
    Browse,
    BrowseViews,
    ResolveBrowsePath,
    Read,
    Write,
    AddItems,
    RemoveItems,
    Test,
    Discover,
    SearchServers,
    SearchNetwork,
    Subscription,
    Add,
    Remove,
    UaRuntime,
}

impl Operation {
    /// Every variant, in declaration order (guard tests walk this list).
    pub const ALL: [Operation; 17] = [
        Operation::Connect,
        Operation::Disconnect,
        Operation::Browse,
        Operation::BrowseViews,
        Operation::ResolveBrowsePath,
        Operation::Read,
        Operation::Write,
        Operation::AddItems,
        Operation::RemoveItems,
        Operation::Test,
        Operation::Discover,
        Operation::SearchServers,
        Operation::SearchNetwork,
        Operation::Subscription,
        Operation::Add,
        Operation::Remove,
        Operation::UaRuntime,
    ];

    /// Stable, ASCII token for logs/journal and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::Connect => "connect",
            Operation::Disconnect => "disconnect",
            Operation::Browse => "browse",
            Operation::BrowseViews => "views",
            Operation::ResolveBrowsePath => "path",
            Operation::Read => "read",
            Operation::Write => "write",
            Operation::AddItems => "add_items",
            Operation::RemoveItems => "remove_items",
            Operation::Test => "test",
            Operation::Discover => "discover",
            Operation::SearchServers => "search_servers",
            Operation::SearchNetwork => "search_network",
            Operation::Subscription => "subscription",
            Operation::Add => "add",
            Operation::Remove => "remove",
            Operation::UaRuntime => "ua-runtime",
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Protection of an OPC UA channel as shown in the connection info line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UaProtection {
    None,
    Sign,
    SignAndEncrypt,
    Unknown,
}

/// Server data that `IOPCServer::GetStatus` adds to the DA connection info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaServerStatus {
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub group_count: u32,
    pub vendor: String,
}

/// Failure to deliver a command to a backend worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    #[error("gateway is shut down")]
    GatewayClosed,
    #[error("worker command channel is closed")]
    WorkerClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    Joined,
    Panicked,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatewayShutdown {
    pub da: ShutdownOutcome,
    pub ua: ShutdownOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UaService {
    Browse,
    BrowseNext,
    BrowseViews,
    TranslatePath,
    Read,
    Write,
}

/// A message of the backend (DA/UA workers, discovery, mDNS) for the user.
///
/// The backend never writes display text: it sends one of these variants and
/// the application renders it in the interface language. Technical details that come from a library, the OS or
/// the server (a `StatusCode`, an `HRESULT`, an `io::Error`) travel as `String`
/// substitutions and are shown as they are. [`BackendMsg::Raw`] is text that is
/// already final (for example, the application's own localized wording).
///
/// Rendering is the responsibility of the frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendMsg {
    InvalidItemHandle,
    // ---- pass-through / composition ----
    Raw(String),
    SendFailed(SendError),
    UaServiceError {
        service: UaService,
        detail: String,
    },
    DaRoute(crate::da_route::DaRouteError),
    RequestTimeout {
        seconds: u64,
    },
    Context {
        context: String,
        cause: Box<BackendMsg>,
    },
    Lines(Vec<BackendMsg>),
    // ---- session state ----
    NoConnection,
    SessionAlreadyConnected,
    ConnectInProgress,
    ConnectCancelled,
    ConnectionClosed,
    SessionClosedAfterTimeout,
    RequestTimedOutSessionClosed,
    SignalNotSubscribed,
    NodeNotSubscribed,
    ServerReturnedNoValue,
    ItemNoResult,
    // ---- unsupported / routing ----
    DaOnlyOnWindows,
    DaNoUaServerSearch,
    DaNoUaNetworkSearch,
    DaNoViews,
    DaNoBrowsePath,
    UaNeedsServerUrl,
    UaUrlScheme,
    InvalidDiscoveredUaUrl,
    UaRuntimeStartFailed {
        detail: String,
    },
    // ---- DA router ----
    DaSessionThreadCrashed,
    DaSessionThreadSpawnFailed {
        detail: String,
    },
    DaTooManyDiscoveries,
    DaDiscoveryCrashed,
    DaDiscoverySpawnFailed {
        detail: String,
    },
    DaDcomSecurityNotSet {
        detail: String,
    },
    // ---- DA connect / browse / test ----
    DaProgIdMissing,
    DaNoOpcServer {
        detail: String,
    },
    DaGroupCreateFailed {
        detail: String,
    },
    DaGroupEmpty,
    DaNoItemMgt {
        detail: String,
    },
    DaNoSyncIo {
        detail: String,
    },
    DaEmptyResult,
    DaNoBrowse,
    DaBrowseRootFailed {
        detail: String,
    },
    DaBrowseBranchFailed {
        segment: String,
        detail: String,
    },
    DaNamespaceFailed {
        detail: String,
    },
    DaLinkLost {
        detail: String,
    },
    DaNoState,
    DaServerListFailed {
        cause: Box<BackendMsg>,
    },
    DaServerListUnavailable {
        detail: String,
    },
    DaServersNotFound,
    DaEmptyInterface {
        host: String,
    },
    DaProgIdLookupFailed {
        prog_id: String,
        detail: String,
    },
    DaOpcEnumUnavailable {
        host: String,
        cause: Box<BackendMsg>,
    },
    DaServerListHostUnavailable {
        host: String,
        detail: String,
    },
    DaClsidResolveFailed {
        prog_id: String,
        host: String,
        remote: Box<BackendMsg>,
        local: Box<BackendMsg>,
    },
    DaStateOk,
    DaNotReady {
        name: String,
        state: i32,
    },
    DaUnknownState {
        state: i32,
    },
    DaConnected {
        group_handle: u32,
        update_rate_ms: u32,
        server: Option<DaServerStatus>,
    },
    // ---- DA value conversion ----
    DaDateRequired,
    DaBadDate,
    DaVtUnsupported {
        vt: u16,
    },
    DaArrayCreateFailed {
        vt: u16,
    },
    DaArrayVtUnsupported {
        vt: u16,
    },
    DaArrayLockFailed {
        vt: u16,
    },
    DaArrayAccessFailed,
    DaNotInteger {
        value: String,
    },
    DaNotUnsigned {
        value: String,
    },
    DaNegativeValue {
        value: String,
    },
    DaNotIntegerAny,
    DaNotUnsignedAny,
    DaNotNumberAny,
    // ---- UA value conversion ----
    WriteMultiDim,
    WriteNodeIdInvalid {
        value: String,
    },
    WriteOtherType {
        value: String,
    },
    WriteNotBoolean {
        value: String,
    },
    WriteOutOfRange {
        value: String,
        ty: String,
    },
    WriteNotInteger {
        value: String,
        ty: String,
    },
    WriteNotUnsigned {
        value: String,
    },
    WriteNotDateTime {
        value: String,
    },
    WriteNotByteString {
        value: String,
    },
    WriteNotText {
        value: String,
    },
    WriteNotNodeId {
        value: String,
    },
    WriteNotQualifiedName {
        value: String,
    },
    WriteBadNsPrefix {
        value: String,
    },
    WriteNotNumber {
        value: String,
        ty: String,
    },
    WriteStructUnsupported {
        ty: String,
    },
    WriteTypeUnsupported {
        ty: String,
    },
    WriteArrayBuildFailed {
        detail: String,
    },
    // ---- UA timeouts ----
    TimeoutBrowse,
    TimeoutBrowseNext,
    TimeoutViews,
    TimeoutResolvePath,
    TimeoutRead,
    TimeoutWrite,
    TimeoutCreateSubscription,
    TimeoutPublishingInterval,
    TimeoutDeadband,
    TimeoutMonitoredItems,
    TimeoutActivateSession,
    TimeoutSearch,
    TimeoutEndpoints {
        url: String,
    },
    // ---- UA browse / path / read / write / test ----
    NoViews,
    BrowsePathRequired,
    ResolvePathFailed {
        status: String,
    },
    PathNotFound,
    PathBadNsPrefix {
        part: String,
    },
    PathEmptyName {
        part: String,
    },
    PathEmpty,
    InvalidNodeId {
        value: String,
    },
    WriteRejected {
        status: String,
    },
    NoWriteResult,
    UaLinkOk,
    UaBadStatus {
        status: String,
    },
    NoStatus,
    // ---- UA subscription ----
    AttributesReadFailed {
        cause: Box<BackendMsg>,
    },
    TypesUnresolved {
        count: usize,
    },
    SubscriptionNotCreated {
        cause: Box<BackendMsg>,
    },
    /// V3: the server's structure types could not be loaded; structures stay
    /// shown as text.
    /// P1: count of items read by polling because the server's subscription
    /// limits are reached.
    PollOnlyItems {
        count: usize,
    },
    StructTypesUnavailable {
        cause: Box<BackendMsg>,
    },
    TimeoutStructTypes,
    CreateSubscriptionFailed {
        detail: String,
    },
    SubscriptionProblem {
        cause: Box<BackendMsg>,
    },
    ChangeIntervalFailed {
        detail: String,
    },
    ChangeDeadbandFailed {
        detail: String,
    },
    DeadbandRejected {
        count: usize,
    },
    DeleteRejected {
        count: usize,
    },
    DeleteFailed {
        detail: String,
    },
    MonitorFailed {
        detail: String,
    },
    // ---- UA connect ----
    UaCertUntrusted {
        url: String,
        detail: String,
        pki: String,
    },
    UaConnectFailed {
        url: String,
        detail: String,
    },
    UaSessionClosedOnConnect {
        url: String,
    },
    UaCertLoginUnsupported,
    UaClientCertMissing {
        cert: String,
        key: String,
    },
    UaClientCertLoadFailed {
        detail: String,
    },
    EndpointsFailed {
        url: String,
        detail: String,
    },
    SecurityResolveFailed {
        detail: String,
    },
    UaConnected {
        url: String,
        policy: String,
        mode: String,
        protection: UaProtection,
        secure_available: bool,
    },
    CertParseFailed {
        detail: String,
    },
    FsCreateFailed {
        path: String,
        detail: String,
    },
    FsSaveFailed {
        path: String,
        detail: String,
    },
    FsWriteFailed {
        path: String,
        detail: String,
    },
    // ---- UA / LDS / mDNS discovery ----
    HostRequired,
    PortInvalid,
    ResolveAddressFailed {
        host: String,
        detail: String,
    },
    AddressNoIp {
        host: String,
    },
    UaNoServers {
        failures: Vec<BackendMsg>,
    },
    AtUrl {
        url: String,
        cause: Box<BackendMsg>,
    },
    ServersNotFoundShort,
    SearchRejected {
        detail: String,
    },
    LdsTimeout {
        url: String,
    },
    LdsFailed {
        url: String,
        detail: String,
    },
    LdsSearchTimedOut,
    LdsUnavailable {
        cause: Box<BackendMsg>,
    },
    MdnsFailed {
        cause: Box<BackendMsg>,
    },
    MdnsCrashed {
        detail: String,
    },
    MdnsNotInTime,
    MdnsDiscarded,
    MdnsNotSupported,
    MdnsVerifyFailed {
        url: String,
        cause: Box<BackendMsg>,
    },
    MdnsVerifyTimeout {
        url: String,
    },
    MdnsApiUnavailable {
        function: String,
    },
    MdnsCallFailed {
        function: String,
        code: i32,
    },
    MdnsBrowseCancelFailed {
        code: i32,
    },
    MdnsResolveStartFailed {
        instance: String,
        code: i32,
    },
    MdnsResolveCompleted {
        code: u32,
    },
    MdnsResolveCancelFailed {
        code: i32,
    },
}

impl BackendMsg {
    /// Already-final text shown as it is.
    pub fn raw(text: impl Into<String>) -> Self {
        Self::Raw(text.into())
    }

    /// Technical detail of a library/OS error as a substitution.
    pub fn detail(error: impl fmt::Display) -> String {
        error.to_string()
    }

    /// `"{context}: {cause}"` (for example `read: <reason>`).
    pub fn context(context: impl Into<String>, cause: BackendMsg) -> Self {
        Self::Context {
            context: context.into(),
            cause: Box::new(cause),
        }
    }
}
