pub use crate::ua_url::{UaUrl, UaUrlIdentity};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub type SessionId = u64;
pub type ItemKey = u32;
pub type RequestId = u64;

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
    // ---- pass-through / composition ----
    Raw(String),
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    OpcDa,
    OpcUa,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpcDa => f.write_str("OPC DA"),
            Self::OpcUa => f.write_str("OPC UA"),
        }
    }
}

/// OPC UA security-policy selection carried at every first-party boundary
/// (dialog, settings, `EndpointSpec`, recent endpoints). `Auto` is the only
/// non-URI member: it means "resolve against the advertised endpoints".
///
/// Legacy settings used free strings ("Авто", "Auto", "None", short names or
/// full `http://opcfoundation.org/UA/SecurityPolicy#...` URIs). Deserialization
/// accepts those spellings; an unknown value is a serde error, never a silent
/// fallback to `Auto`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UaSecurityPolicy {
    #[default]
    Auto,
    None,
    Basic128Rsa15,
    Basic256,
    Basic256Sha256,
    Aes128Sha256RsaOaep,
    Aes256Sha256RsaPss,
}

impl UaSecurityPolicy {
    /// Every variant, in UI order (guard tests walk this list).
    pub const ALL: [UaSecurityPolicy; 7] = [
        UaSecurityPolicy::Auto,
        UaSecurityPolicy::None,
        UaSecurityPolicy::Basic128Rsa15,
        UaSecurityPolicy::Basic256,
        UaSecurityPolicy::Basic256Sha256,
        UaSecurityPolicy::Aes128Sha256RsaOaep,
        UaSecurityPolicy::Aes256Sha256RsaPss,
    ];

    /// Stable, ASCII token for logs/persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            UaSecurityPolicy::Auto => "Auto",
            UaSecurityPolicy::None => "None",
            UaSecurityPolicy::Basic128Rsa15 => "Basic128Rsa15",
            UaSecurityPolicy::Basic256 => "Basic256",
            UaSecurityPolicy::Basic256Sha256 => "Basic256Sha256",
            UaSecurityPolicy::Aes128Sha256RsaOaep => "Aes128_Sha256_RsaOaep",
            UaSecurityPolicy::Aes256Sha256RsaPss => "Aes256_Sha256_RsaPss",
        }
    }

    /// Full policy URI for a concrete policy. `Auto` has no URI (it must be
    /// resolved against the server's advertised endpoints first).
    pub fn as_uri(self) -> Option<&'static str> {
        match self {
            UaSecurityPolicy::Auto => None,
            UaSecurityPolicy::None => Some("http://opcfoundation.org/UA/SecurityPolicy#None"),
            UaSecurityPolicy::Basic128Rsa15 => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Basic128Rsa15")
            }
            UaSecurityPolicy::Basic256 => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256")
            }
            UaSecurityPolicy::Basic256Sha256 => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256")
            }
            UaSecurityPolicy::Aes128Sha256RsaOaep => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Aes128_Sha256_RsaOaep")
            }
            UaSecurityPolicy::Aes256Sha256RsaPss => {
                Some("http://opcfoundation.org/UA/SecurityPolicy#Aes256_Sha256_RsaPss")
            }
        }
    }

    /// Parse a server-advertised policy (short name or full URI). An empty
    /// string is the legacy "no policy advertised" placeholder and maps to
    /// `None`. Unknown policies return `None` so the caller can report them
    /// explicitly instead of forcing a fallback.
    pub fn from_advertised(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityPolicy::None);
        }
        Self::from_token(text.rsplit('#').next().unwrap_or(text))
    }

    /// Parse a stored/legacy selection token. An empty string means "unset"
    /// and maps to `Auto` (the old dialogs always wrote an explicit token).
    pub fn from_stored(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityPolicy::Auto);
        }
        Self::from_token(text.rsplit('#').next().unwrap_or(text))
    }

    fn from_token(token: &str) -> Option<Self> {
        // `to_ascii_lowercase` leaves Cyrillic untouched; accept both legacy
        // capitalizations explicitly.
        if token == "Авто" || token == "авто" {
            return Some(UaSecurityPolicy::Auto);
        }
        // The OPC UA URI fragment spells the Aes policies with underscores
        // (`Aes256_Sha256_RsaPss`) while the library calls them with hyphens;
        // separators carry no meaning, so ignore them.
        let compact: String = token
            .chars()
            .filter(|c| *c != '_' && *c != '-')
            .collect::<String>()
            .to_ascii_lowercase();
        match compact.as_str() {
            "auto" => Some(UaSecurityPolicy::Auto),
            "none" => Some(UaSecurityPolicy::None),
            "basic128rsa15" => Some(UaSecurityPolicy::Basic128Rsa15),
            "basic256" => Some(UaSecurityPolicy::Basic256),
            "basic256sha256" => Some(UaSecurityPolicy::Basic256Sha256),
            "aes128sha256rsaoaep" => Some(UaSecurityPolicy::Aes128Sha256RsaOaep),
            "aes256sha256rsapss" => Some(UaSecurityPolicy::Aes256Sha256RsaPss),
            _ => None,
        }
    }
}

impl fmt::Display for UaSecurityPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UaSecurityPolicy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UaSecurityPolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_stored(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown OPC UA security policy {raw:?} (expected Auto, None, Basic128Rsa15, Basic256, Basic256Sha256, Aes128_Sha256_RsaOaep, Aes256_Sha256_RsaPss or a full SecurityPolicy URI)"
            ))
        })
    }
}

/// OPC UA message-security-mode selection. `Auto` is the only non-wire member;
/// the rest map 1:1 to `MessageSecurityMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UaSecurityMode {
    #[default]
    Auto,
    None,
    Sign,
    SignAndEncrypt,
}

impl UaSecurityMode {
    pub const ALL: [UaSecurityMode; 4] = [
        UaSecurityMode::Auto,
        UaSecurityMode::None,
        UaSecurityMode::Sign,
        UaSecurityMode::SignAndEncrypt,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UaSecurityMode::Auto => "Auto",
            UaSecurityMode::None => "None",
            UaSecurityMode::Sign => "Sign",
            UaSecurityMode::SignAndEncrypt => "SignAndEncrypt",
        }
    }

    /// Parse a server-advertised mode (`{:?}` form). An empty string is the
    /// legacy "not advertised" placeholder and maps to `None`.
    pub fn from_advertised(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityMode::None);
        }
        Self::from_token(text)
    }

    /// Parse a stored/legacy selection token. An empty string means "unset"
    /// and maps to `Auto`.
    pub fn from_stored(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(UaSecurityMode::Auto);
        }
        Self::from_token(text)
    }

    fn from_token(token: &str) -> Option<Self> {
        if token == "Авто" || token == "авто" {
            return Some(UaSecurityMode::Auto);
        }
        match token.to_ascii_lowercase().as_str() {
            "auto" => Some(UaSecurityMode::Auto),
            "none" => Some(UaSecurityMode::None),
            "sign" => Some(UaSecurityMode::Sign),
            "signandencrypt" => Some(UaSecurityMode::SignAndEncrypt),
            _ => None,
        }
    }
}

impl fmt::Display for UaSecurityMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UaSecurityMode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UaSecurityMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_stored(&raw).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown OPC UA security mode {raw:?} (expected Auto, None, Sign, SignAndEncrypt)"
            ))
        })
    }
}

/// Why a policy/mode pair cannot be requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityPairError {
    /// `None` policy means "no security"; a signing/encrypting mode contradicts it.
    NonePolicyRequiresNoneMode { mode: UaSecurityMode },
}

impl fmt::Display for SecurityPairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityPairError::NonePolicyRequiresNoneMode { mode } => write!(
                f,
                "security policy None cannot be combined with mode {mode} (None policy requires mode None)"
            ),
        }
    }
}

/// Validate the policy/mode combination independently of any server.
///
/// Sign/SignAndEncrypt require a non-`None` policy; a `None` policy requires
/// the `None` mode. `Auto` on either axis is always allowed.
pub fn validate_security_pair(
    policy: UaSecurityPolicy,
    mode: UaSecurityMode,
) -> Result<(), SecurityPairError> {
    match (policy, mode) {
        (UaSecurityPolicy::None, UaSecurityMode::Sign | UaSecurityMode::SignAndEncrypt) => {
            Err(SecurityPairError::NonePolicyRequiresNoneMode { mode })
        }
        (UaSecurityPolicy::None, UaSecurityMode::None | UaSecurityMode::Auto) => Ok(()),
        (
            UaSecurityPolicy::Auto
            | UaSecurityPolicy::Basic128Rsa15
            | UaSecurityPolicy::Basic256
            | UaSecurityPolicy::Basic256Sha256
            | UaSecurityPolicy::Aes128Sha256RsaOaep
            | UaSecurityPolicy::Aes256Sha256RsaPss,
            _,
        ) => Ok(()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EndpointSpec {
    pub protocol: Protocol,
    /// DA: computer name (empty = local). UA: unused (kept for display).
    pub host: String,
    /// DA: ProgID. UA: opc.tcp:// URL.
    pub address: String,
    /// UA user name (empty = anonymous).
    pub user: String,
    #[serde(skip)]
    pub password: String,
    /// UA: chosen security policy. `None` means "not set" and resolves as
    /// `UaSecurityPolicy::Auto`; legacy string values are accepted by serde.
    /// DA: unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ua_security_policy: Option<UaSecurityPolicy>,
    /// UA: chosen message security mode. `None` means "not set" and resolves
    /// as `UaSecurityMode::Auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ua_security_mode: Option<UaSecurityMode>,
    /// UA: trust the server certificate without importing it into the PKI trust
    /// store (reuses `settings` trust flag on the connect dialog).
    #[serde(default)]
    pub ua_trust_server_cert: bool,
    /// UA: whether the identity was requested as X509 (client certificate).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ua_use_certificate: bool,
    /// UA: requested session timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ua_session_timeout: Option<u32>,
    /// UA: transport binding captured when the session was created. Reconnects
    /// reuse this snapshot instead of the current adapter map. Never persisted
    /// and always `None` for DA.
    #[serde(skip)]
    pub ua_bind: Option<crate::net::NetworkBinding>,
    /// The name advertised by the server itself, captured when the user selects
    /// a discovered server. UA: the application name (e.g. "Prosys OPC UA
    /// Simulation Server"); DA: the description + ProgID. `None` when a server
    /// was selected or typed manually and the advertised name was never seen,
    /// in which case the endpoint falls back to the address (`display_name()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl EndpointSpec {
    pub fn da(host: impl Into<String>, prog_id: impl Into<String>) -> Self {
        Self {
            protocol: Protocol::OpcDa,
            host: host.into(),
            address: prog_id.into(),
            user: String::new(),
            password: String::new(),
            ua_security_policy: None,
            ua_security_mode: None,
            ua_trust_server_cert: false,
            ua_use_certificate: false,
            ua_session_timeout: None,
            ua_bind: None,
            display_name: None,
        }
    }

    pub fn ua(url: impl Into<String>) -> Self {
        Self {
            protocol: Protocol::OpcUa,
            host: String::new(),
            address: url.into(),
            user: String::new(),
            password: String::new(),
            ua_security_policy: None,
            ua_security_mode: None,
            ua_trust_server_cert: false,
            ua_use_certificate: false,
            ua_session_timeout: None,
            ua_bind: None,
            display_name: None,
        }
    }

    pub fn display_name(&self) -> String {
        match self.protocol {
            Protocol::OpcDa => {
                if self.host.is_empty() {
                    self.address.clone()
                } else {
                    format!("{} @ {}", self.address, self.host)
                }
            }
            Protocol::OpcUa => self.address.clone(),
        }
    }

    /// Record the name the server itself advertises (UA application name or the
    /// DA description + ProgID). Stored in `self.display_name` and written back
    /// into recent-endpoints persistence so the servers list/tab show it.
    pub fn with_advertised_name(mut self, name: Option<String>) -> Self {
        self.display_name = name.filter(|s| !s.trim().is_empty());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEntry {
    pub id: String,
    pub description: String,
}

/// An advertised value can be absent, supported, or unknown to this client.
/// Unknown wire values remain visible and can never become an Auto/None choice.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Advertised<T> {
    Unspecified,
    Known(T),
    Unknown(String),
}

pub type AdvertisedPolicy = Advertised<UaSecurityPolicy>;
pub type AdvertisedMode = Advertised<UaSecurityMode>;

impl<T: Copy> Advertised<T> {
    pub fn known(&self) -> Option<T> {
        match self {
            Self::Known(value) => Some(*value),
            _ => None,
        }
    }
    pub fn is_unspecified(&self) -> bool {
        matches!(self, Self::Unspecified)
    }
}
impl AdvertisedPolicy {
    pub fn from_wire(text: &str) -> Self {
        let raw = text;
        let text = text.trim();
        if text.is_empty() {
            return Self::Unspecified;
        }
        // A policy URI is an identity, not just its fragment. Never turn an
        // unrecognized namespace ending in `#None` into an unprotected choice.
        if text.contains('#') && !text.starts_with("http://opcfoundation.org/UA/SecurityPolicy#") {
            return Self::Unknown(raw.to_owned());
        }
        match UaSecurityPolicy::from_advertised(text) {
            Some(policy)
                if policy != UaSecurityPolicy::Auto
                    && (!text.contains('#') || policy.as_uri() == Some(text)) =>
            {
                Self::Known(policy)
            }
            _ => Self::Unknown(raw.to_owned()),
        }
    }
}
impl AdvertisedMode {
    pub fn from_wire(text: &str) -> Self {
        let raw = text;
        let text = text.trim();
        if text.is_empty() {
            return Self::Unspecified;
        }
        match UaSecurityMode::from_advertised(text) {
            Some(mode) if mode != UaSecurityMode::Auto => Self::Known(mode),
            _ => Self::Unknown(raw.to_owned()),
        }
    }
}
impl<T: fmt::Display> fmt::Display for Advertised<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unspecified => Ok(()),
            Self::Known(value) => value.fmt(f),
            Self::Unknown(value) => f.write_str(value),
        }
    }
}

/// A single OPC UA endpoint as reported by a discovery search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UaEndpointInfo {
    pub url: UaUrl,
    pub security_policy: AdvertisedPolicy,
    pub security_mode: AdvertisedMode,
}

/// A single OPC UA server found by the discovery search.
#[derive(Clone, Debug)]
pub struct UaServerInfo {
    pub application_name: String,
    pub application_uri: String,
    pub endpoints: Vec<UaEndpointInfo>,
}

/// A server reported by `FindServersOnNetwork` (mDNS/LDS network discovery),
/// carrying its discovery URLs instead of full endpoint descriptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UaNetworkInfo {
    pub application_name: String,
    pub application_uri: String,
    pub discovery_urls: Vec<UaUrl>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityKind {
    Good,
    Uncertain,
    Bad,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quality {
    pub code: u32,
    pub kind: QualityKind,
    pub text: String,
}

impl Quality {
    pub fn from_da(code: u16) -> Self {
        let code32 = u32::from(code);
        let kind = match code & 0xC0 {
            0xC0 => QualityKind::Good,
            0x40 => QualityKind::Uncertain,
            _ => QualityKind::Bad,
        };
        Self {
            code: code32,
            kind,
            text: da_quality_text(code).to_string(),
        }
    }

    pub fn from_ua(code: u32, text: impl Into<String>) -> Self {
        // OPC UA severity: 00 = Good, 01 = Uncertain, 10/11 = Bad.
        let kind = match code >> 30 {
            0b00 => QualityKind::Good,
            0b01 => QualityKind::Uncertain,
            _ => QualityKind::Bad,
        };
        Self {
            code,
            kind,
            text: text.into(),
        }
    }
}

fn da_quality_text(code: u16) -> &'static str {
    match code & 0xFFFC {
        0x0000 => "BAD",
        0x0004 => "BAD - CONFIG_ERROR",
        0x0008 => "BAD - NOT_CONNECTED",
        0x000C => "BAD - DEVICE_FAILURE",
        0x0010 => "BAD - SENSOR_FAILURE",
        0x0014 => "BAD - LAST_KNOWN",
        0x0018 => "BAD - COMM_FAILURE",
        0x001C => "BAD - OUT_OF_SERVICE",
        0x0020 => "BAD - WAITING_FOR_INITIAL_DATA",
        0x0040 => "UNCERTAIN",
        0x0044 => "UNCERTAIN - LAST_USABLE",
        0x0048 => "UNCERTAIN - SENSOR_NOT_ACCURATE",
        0x004C => "UNCERTAIN - EGU_EXCEEDED",
        0x0050 => "UNCERTAIN - SUB_NORMAL",
        0x0054 => "UNCERTAIN - CONVERT_ERROR",
        0x00C0 => "GOOD",
        0x00C8 => "GOOD - CALCULATED",
        0x00D8 => "GOOD - LOCAL_OVERRIDE",
        _ => "UNKNOWN",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Empty,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Bytes(Vec<u8>),
    DateTime(chrono::DateTime<chrono::Utc>),
    Array(Vec<Value>),
    NodeId(String),
    QualifiedName {
        namespace_index: u16,
        name: String,
    },
    LocalizedText {
        locale: String,
        text: String,
    },
    /// V3: a decoded structure (OPC UA ExtensionObject whose type the server
    /// described): its type name and the fields in declaration order.
    Struct {
        type_name: String,
        fields: Vec<(String, Value)>,
    },
    Other(String),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::I64(v) => Some(*v as f64),
            Self::U64(v) => Some(*v as f64),
            Self::F64(v) => Some(*v),
            Self::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            Self::Str(v) => v.trim().parse().ok(),
            Self::LocalizedText { text, .. } => text.trim().parse().ok(),
            _ => None,
        }
    }
}

/// How many leading bytes are shown for a `Bytes` value before truncation.
const BYTES_SHOWN: usize = 16;
/// How many leading elements are shown for an `Array` value before truncation.
const ARRAY_SHOWN: usize = 16;

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => Ok(()),
            Self::Bool(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::F64(v) => {
                if v.is_finite() {
                    write!(f, "{v}")
                } else {
                    write!(f, "{v:?}")
                }
            }
            Self::Str(v) => f.write_str(v),
            Self::Bytes(v) => {
                let shown = v
                    .iter()
                    .take(BYTES_SHOWN)
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>();
                write!(f, "{}", shown.join(" "))?;
                if v.len() > BYTES_SHOWN {
                    write!(f, "…")?;
                }
                Ok(())
            }
            Self::DateTime(v) => write!(
                f,
                "{}",
                v.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S%.3f")
            ),
            Self::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().take(ARRAY_SHOWN).enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                if items.len() > ARRAY_SHOWN {
                    write!(f, "…({})", items.len())?;
                }
                f.write_str("]")
            }
            Self::NodeId(v) => write!(f, "NodeId({v})"),
            Self::QualifiedName {
                namespace_index,
                name,
            } => {
                if *namespace_index == 0 {
                    write!(f, "{name}")
                } else {
                    write!(f, "{namespace_index}:{name}")
                }
            }
            Self::LocalizedText { locale, text } => {
                if locale.trim().is_empty() {
                    f.write_str(text)
                } else {
                    write!(f, "[{locale}] {text}")
                }
            }
            Self::Struct { fields, .. } => {
                f.write_str("{")?;
                for (i, (name, value)) in fields.iter().take(ARRAY_SHOWN).enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                if fields.len() > ARRAY_SHOWN {
                    write!(f, ", …({})", fields.len())?;
                }
                f.write_str("}")
            }
            Self::Other(v) => f.write_str(v),
        }
    }
}

impl Value {
    /// V3: children of a structure (field name, value) or an array (`[i]`,
    /// value), for the expandable table row. Empty for scalars.
    pub fn children(&self) -> Vec<(String, &Value)> {
        match self {
            Self::Struct { fields, .. } => fields
                .iter()
                .map(|(name, value)| (name.clone(), value))
                .collect(),
            Self::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, value)| (format!("[{i}]"), value))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether the value has children to expand (a non-empty structure or array).
    pub fn is_expandable(&self) -> bool {
        match self {
            Self::Struct { fields, .. } => !fields.is_empty(),
            Self::Array(items) => !items.is_empty(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataValue {
    pub value: Value,
    pub quality: Quality,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Why the backend produced no value (set by [`DataValue::bad`]); `None`
    /// for a value the server delivered.
    pub reason: Option<BackendMsg>,
}

impl DataValue {
    /// A Bad value carrying a typed reason. Frontends render it in their chosen
    /// language; raw messages are also retained as quality text.
    pub fn bad(reason: BackendMsg) -> Self {
        Self {
            value: Value::Empty,
            quality: Quality {
                code: 0,
                kind: QualityKind::Bad,
                text: match &reason {
                    BackendMsg::Raw(text) => text.clone(),
                    _ => String::new(),
                },
            },
            timestamp: None,
            reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItemInfo {
    pub data_type: Option<String>,
    /// OPC DA canonical VT_* code (None for OPC UA).
    pub type_id: Option<u16>,
    pub writable: bool,
}

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
    pub request_id: RequestId,
    pub session: SessionId,
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
    /// subnet hosts. Results arrive as `UaScan*` events with `request_id: 0`.
    ScanUaNetwork {
        scan_id: u64,
        bind: crate::net::NetworkBinding,
        subnet: Option<crate::net::SubnetSpec>,
        local_ip: std::net::IpAddr,
    },
    /// Cooperative cancellation of a running subnet scan. The scan task sends
    /// its terminal `UaScanFinished` event on its own.
    CancelUaScan {
        scan_id: u64,
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
    /// Identity of the command that produced this event. `0` means an
    /// unsolicited push/runtime event (DataChange, worker startup error),
    /// which is always accepted by the app; non-zero events are only accepted
    /// while their `(session, request_id)` pair is outstanding.
    pub request_id: RequestId,
    pub session: SessionId,
    pub kind: EventKind,
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
    /// Live progress of a running N+1 subnet scan (streaming, `request_id: 0`).
    UaScanProgress {
        scan_id: u64,
        total: usize,
        checked: usize,
        found: usize,
    },
    /// Newly discovered servers from a running subnet scan (one or more batches).
    UaScanFound {
        scan_id: u64,
        servers: Vec<UaServerInfo>,
    },
    /// Terminal event of a subnet scan. `cancelled` distinguishes a user stop
    /// from a completed run; `error` carries a technical detail when the scan
    /// could not run at all (for example a subnet above the confirmation cap).
    UaScanFinished {
        scan_id: u64,
        cancelled: bool,
        found: usize,
        error: Option<BackendMsg>,
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
        result: Result<(), BackendMsg>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_views_root_is_identified_by_reserved_id() {
        let mut node = BrowseNode {
            node_id: BrowseNode::VIEWS_ROOT_ID.to_owned(),
            name: String::new(),
            kind: NodeKind::Branch,
            has_children: true,
        };
        assert!(node.is_views_root());
        node.node_id = "ns=1;s=views".to_owned();
        assert!(!node.is_views_root());
    }

    #[test]
    fn operation_codes_are_ascii_and_cover_the_emitters() {
        // Typed operations are compared by enum (not localized text), so their
        // log tokens must never contain non-ASCII and must be unique.
        let mut seen = std::collections::HashSet::new();
        for op in Operation::ALL {
            let code = op.as_str();
            assert!(code.is_ascii(), "operation code must be ASCII: {op:?}");
            assert!(!code.is_empty());
            assert!(seen.insert(code), "duplicate operation code: {code}");
        }
        // The tokens are the exact legacy values the journal/logs used.
        assert_eq!(Operation::SearchServers.as_str(), "search_servers");
        assert_eq!(Operation::SearchNetwork.as_str(), "search_network");
        assert_eq!(Operation::Connect.as_str(), "connect");
        assert_eq!(Operation::BrowseViews.as_str(), "views");
        assert_eq!(Operation::Subscription.as_str(), "subscription");
        assert_eq!(Operation::Add.as_str(), "add");
        assert_eq!(Operation::Remove.as_str(), "remove");
    }

    #[test]
    fn command_kind_maps_to_its_operation() {
        // Pins the command -> operation mapping used to label dispatch failures.
        assert_eq!(
            CommandKind::Connect {
                endpoint: EndpointSpec::ua("opc.tcp://x:4840")
            }
            .operation(),
            Operation::Connect
        );
        assert_eq!(
            CommandKind::ConfigureUaSubscription {
                publishing_interval_ms: 1000,
                deadband: 0.0
            }
            .operation(),
            Operation::Subscription
        );
        assert_eq!(
            CommandKind::AddItems { items: Vec::new() }.operation(),
            Operation::AddItems
        );
        assert_eq!(
            CommandKind::RemoveItems { keys: Vec::new() }.operation(),
            Operation::RemoveItems
        );
        assert_eq!(CommandKind::Test.operation(), Operation::Test);
        assert_eq!(
            CommandKind::ScanUaNetwork {
                scan_id: 1,
                bind: crate::net::NetworkBinding::loopback(),
                subnet: None,
                local_ip: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            }
            .operation(),
            Operation::SearchNetwork
        );
        assert_eq!(
            CommandKind::CancelUaScan { scan_id: 1 }.operation(),
            Operation::SearchNetwork
        );
    }

    #[test]
    fn endpoint_display_name_serde_backward_compatible() {
        // Old JSON (no display_name) must deserialize to `None` and load fine.
        let old = r#"{"protocol":"OpcDa","host":"box1","address":"Alpha.Server","user":"","password":"","ua_trust_server_cert":false}"#;
        let ep: EndpointSpec = serde_json::from_str(old).unwrap();
        assert!(ep.display_name.is_none());
        // And it round-trips without the field (skipped when None).
        let json = serde_json::to_string(&ep).unwrap();
        assert!(!json.contains("display_name"), "got: {json}");
        // New JSON carries the advertised name.
        let ep2 = EndpointSpec::ua("opc.tcp://x:4840")
            .with_advertised_name(Some("My Server".to_string()));
        let json = serde_json::to_string(&ep2).unwrap();
        assert!(json.contains("display_name") && json.contains("My Server"));
        let back: EndpointSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.display_name.as_deref(), Some("My Server"));
    }

    #[test]
    fn endpoint_ua_binding_is_runtime_only_and_not_persisted() {
        let mut ep = EndpointSpec::ua("opc.tcp://192.168.1.50:4840");
        ep.ua_bind = Some(crate::net::NetworkBinding::new(
            "192.168.1.20".parse().unwrap(),
            std::num::NonZeroU32::new(7),
            std::num::NonZeroU32::new(9),
            None,
        ));
        let json = serde_json::to_string(&ep).unwrap();
        assert!(!json.contains("ua_bind"), "got: {json}");
        let back: EndpointSpec = serde_json::from_str(&json).unwrap();
        assert!(back.ua_bind.is_none());

        let da = EndpointSpec::da("", "A.Server");
        assert!(da.ua_bind.is_none());
    }

    #[test]
    fn endpoint_display_name_composition() {
        let ep = EndpointSpec::ua("opc.tcp://127.0.0.1:4840")
            .with_advertised_name(Some("Prosys OPC UA".to_string()));
        assert_eq!(ep.display_name.as_deref(), Some("Prosys OPC UA"));
        // Blank advertised names are dropped (None -> fallback to address).
        let ep2 =
            EndpointSpec::da("", "Alpha.Server").with_advertised_name(Some("   ".to_string()));
        assert!(ep2.display_name.is_none());
    }

    #[test]
    fn endpoint_constructors_keep_no_advertised_name() {
        assert!(EndpointSpec::da("", "A.Server").display_name.is_none());
        assert!(EndpointSpec::ua("opc.tcp://x:4840").display_name.is_none());
    }

    #[test]
    fn quality_from_ua_severity_mapping() {
        assert_eq!(Quality::from_ua(0x0000, "Good").kind, QualityKind::Good);
        assert_eq!(
            Quality::from_ua(0x4000_0000, "Uncertain").kind,
            QualityKind::Uncertain
        );
        assert_eq!(Quality::from_ua(0x8000_0000, "Bad").kind, QualityKind::Bad);
        assert_eq!(Quality::from_ua(0xC000_0001, "Bad").kind, QualityKind::Bad);
        assert_eq!(Quality::from_ua(0x8000_0000, "x").code >> 30, 0b10);
    }

    #[test]
    fn value_as_f64() {
        assert_eq!(Value::I64(42).as_f64(), Some(42.0));
        assert_eq!(Value::U64(7).as_f64(), Some(7.0));
        assert_eq!(Value::F64(3.5).as_f64(), Some(3.5));
        assert_eq!(Value::Bool(true).as_f64(), Some(1.0));
        assert_eq!(Value::Bool(false).as_f64(), Some(0.0));
        assert_eq!(Value::Str("12.5".to_string()).as_f64(), Some(12.5));
        assert_eq!(Value::Str(" 4 ".to_string()).as_f64(), Some(4.0));
        assert_eq!(Value::Str("abc".to_string()).as_f64(), None);
        assert_eq!(Value::Str("".to_string()).as_f64(), None);
        assert_eq!(Value::Empty.as_f64(), None);
    }

    #[test]
    fn value_format() {
        assert_eq!(format!("{}", Value::Bool(false)), "false");
        assert_eq!(format!("{}", Value::I64(-5)), "-5");
        assert_eq!(format!("{}", Value::U64(9)), "9");
        assert_eq!(format!("{}", Value::F64(1.5)), "1.5");
        assert_eq!(format!("{}", Value::F64(f64::NAN)), "NaN");
        assert_eq!(format!("{}", Value::F64(f64::INFINITY)), "inf");
        assert_eq!(format!("{}", Value::Str("hi".to_string())), "hi");
        assert_eq!(format!("{}", Value::Empty), "");
    }

    #[test]
    fn datetime_format() {
        let dt = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 7, 2, 3, 4, 5).unwrap();
        let s = format!("{}", Value::DateTime(dt));
        assert!(s.contains("2024"), "unexpected datetime format: {s}");
    }

    #[test]
    fn node_id_format() {
        assert_eq!(
            format!("{}", Value::NodeId("i=85".to_string())),
            "NodeId(i=85)"
        );
        assert_eq!(
            format!("{}", Value::NodeId("ns=2;s=MyVar".to_string())),
            "NodeId(ns=2;s=MyVar)"
        );
    }

    #[test]
    fn qualified_name_format() {
        assert_eq!(
            format!(
                "{}",
                Value::QualifiedName {
                    namespace_index: 0,
                    name: "MyVar".to_string()
                }
            ),
            "MyVar"
        );
        assert_eq!(
            format!(
                "{}",
                Value::QualifiedName {
                    namespace_index: 2,
                    name: "MyVar".to_string()
                }
            ),
            "2:MyVar"
        );
    }

    #[test]
    fn localized_text_format() {
        assert_eq!(
            format!(
                "{}",
                Value::LocalizedText {
                    locale: String::new(),
                    text: "hi".to_string()
                }
            ),
            "hi"
        );
        assert_eq!(
            format!(
                "{}",
                Value::LocalizedText {
                    locale: "en".to_string(),
                    text: "hi".to_string()
                }
            ),
            "[en] hi"
        );
    }

    #[test]
    fn bytes_format_is_hex_with_truncation() {
        assert_eq!(
            format!("{}", Value::Bytes(vec![0xAA, 0x01, 0xFF])),
            "AA 01 FF"
        );
        let long: Vec<u8> = (0..40).collect();
        let s = format!("{}", Value::Bytes(long));
        assert!(
            s.starts_with("00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F"),
            "got: {s}"
        );
        assert!(s.ends_with('…'), "truncation marker expected: {s}");
        assert!(!s.contains("40"), "bytes beyond 16 must be hidden: {s}");
    }

    #[test]
    fn array_format_truncates_after_16() {
        let short = Value::Array(vec![Value::I64(1), Value::I64(2)]);
        assert_eq!(format!("{short}"), "[1, 2]");
        let long = Value::Array((0..20).map(Value::I64).collect());
        let s = format!("{long}");
        assert!(
            s.starts_with("[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15"),
            "got: {s}"
        );
        assert!(s.ends_with("…(20)]"), "got: {s}");
    }

    #[test]
    fn new_value_variants_as_f64() {
        assert_eq!(
            Value::LocalizedText {
                locale: String::new(),
                text: "12.5".to_string()
            }
            .as_f64(),
            Some(12.5)
        );
        assert_eq!(
            Value::LocalizedText {
                locale: "ru".to_string(),
                text: "abc".to_string()
            }
            .as_f64(),
            None
        );
        assert_eq!(Value::NodeId("i=5".to_string()).as_f64(), None);
        assert_eq!(
            Value::QualifiedName {
                namespace_index: 0,
                name: "x".to_string()
            }
            .as_f64(),
            None
        );
    }

    #[test]
    fn security_enums_serde_roundtrip_and_stable_tokens() {
        for policy in UaSecurityPolicy::ALL {
            let json = serde_json::to_string(&policy).unwrap();
            assert_eq!(json, format!("\"{}\"", policy.as_str()));
            let back: UaSecurityPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(back, policy);
            assert!(policy.as_str().is_ascii());
        }
        for mode in UaSecurityMode::ALL {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", mode.as_str()));
            let back: UaSecurityMode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mode);
            assert!(mode.as_str().is_ascii());
        }
        // Tokens are unique (guard against two variants serializing alike).
        let mut seen = std::collections::HashSet::new();
        for policy in UaSecurityPolicy::ALL {
            assert!(seen.insert(policy.as_str()), "duplicate policy token");
        }
        let mut seen = std::collections::HashSet::new();
        for mode in UaSecurityMode::ALL {
            assert!(seen.insert(mode.as_str()), "duplicate mode token");
        }
    }

    #[test]
    fn security_legacy_strings_deserialize() {
        // The spellings the old UI could persist.
        for (raw, expected) in [
            ("\"Авто\"", UaSecurityPolicy::Auto),
            ("\"Auto\"", UaSecurityPolicy::Auto),
            ("\"auto\"", UaSecurityPolicy::Auto),
            ("\"None\"", UaSecurityPolicy::None),
            ("\"Basic128Rsa15\"", UaSecurityPolicy::Basic128Rsa15),
            ("\"Basic256\"", UaSecurityPolicy::Basic256),
            ("\"Basic256Sha256\"", UaSecurityPolicy::Basic256Sha256),
            (
                "\"http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256\"",
                UaSecurityPolicy::Basic256Sha256,
            ),
            (
                "\"http://opcfoundation.org/UA/SecurityPolicy#None\"",
                UaSecurityPolicy::None,
            ),
            ("\"\"", UaSecurityPolicy::Auto),
        ] {
            let got: UaSecurityPolicy = serde_json::from_str(raw).unwrap();
            assert_eq!(got, expected, "legacy policy {raw}");
        }
        for (raw, expected) in [
            ("\"Авто\"", UaSecurityMode::Auto),
            ("\"Auto\"", UaSecurityMode::Auto),
            ("\"None\"", UaSecurityMode::None),
            ("\"Sign\"", UaSecurityMode::Sign),
            ("\"SignAndEncrypt\"", UaSecurityMode::SignAndEncrypt),
            ("\"\"", UaSecurityMode::Auto),
        ] {
            let got: UaSecurityMode = serde_json::from_str(raw).unwrap();
            assert_eq!(got, expected, "legacy mode {raw}");
        }
    }

    #[test]
    fn security_unknown_strings_are_errors_not_auto() {
        for raw in [
            "\"Aes512Sha999Hypothetical\"",
            "\"http://example.org/SecurityPolicy#Custom\"",
            "\"garbage\"",
        ] {
            let err = serde_json::from_str::<UaSecurityPolicy>(raw).unwrap_err();
            assert!(
                err.to_string().contains("unknown OPC UA security policy"),
                "unexpected error for {raw}: {err}"
            );
        }
        for raw in ["\"Invalid\"", "\"SignAndEncryption\"", "\"garbage\""] {
            let err = serde_json::from_str::<UaSecurityMode>(raw).unwrap_err();
            assert!(
                err.to_string().contains("unknown OPC UA security mode"),
                "unexpected error for {raw}: {err}"
            );
        }
    }

    #[test]
    fn aes_policies_round_trip_through_every_spelling() {
        let aes128 = "http://opcfoundation.org/UA/SecurityPolicy#Aes128_Sha256_RsaOaep";
        let aes256 = "http://opcfoundation.org/UA/SecurityPolicy#Aes256_Sha256_RsaPss";
        assert_eq!(UaSecurityPolicy::Aes128Sha256RsaOaep.as_uri(), Some(aes128));
        assert_eq!(UaSecurityPolicy::Aes256Sha256RsaPss.as_uri(), Some(aes256));
        for (policy, uri) in [
            (UaSecurityPolicy::Aes128Sha256RsaOaep, aes128),
            (UaSecurityPolicy::Aes256Sha256RsaPss, aes256),
        ] {
            // What the server advertises (URI) and what the settings stored
            // (token) both come back as the same typed value.
            assert_eq!(UaSecurityPolicy::from_advertised(uri), Some(policy));
            assert_eq!(UaSecurityPolicy::from_stored(policy.as_str()), Some(policy));
            let json = serde_json::to_string(&policy).unwrap();
            assert_eq!(
                serde_json::from_str::<UaSecurityPolicy>(&json).unwrap(),
                policy
            );
            assert!(UaSecurityPolicy::ALL.contains(&policy));
            assert!(validate_security_pair(policy, UaSecurityMode::SignAndEncrypt).is_ok());
            assert!(validate_security_pair(policy, UaSecurityMode::Sign).is_ok());
        }
        // The spelling without separators is accepted too.
        assert_eq!(
            UaSecurityPolicy::from_stored("Aes256Sha256RsaPss"),
            Some(UaSecurityPolicy::Aes256Sha256RsaPss)
        );
    }

    #[test]
    fn security_policy_uri_mapping() {
        assert_eq!(UaSecurityPolicy::Auto.as_uri(), None);
        assert_eq!(
            UaSecurityPolicy::None.as_uri(),
            Some("http://opcfoundation.org/UA/SecurityPolicy#None")
        );
        assert_eq!(
            UaSecurityPolicy::Basic128Rsa15.as_uri(),
            Some("http://opcfoundation.org/UA/SecurityPolicy#Basic128Rsa15")
        );
        assert_eq!(
            UaSecurityPolicy::Basic256.as_uri(),
            Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256")
        );
        assert_eq!(
            UaSecurityPolicy::Basic256Sha256.as_uri(),
            Some("http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256")
        );
        // Advertised full URIs and short names both normalize to the enum.
        for policy in UaSecurityPolicy::ALL
            .into_iter()
            .filter(|p| *p != UaSecurityPolicy::Auto)
        {
            assert_eq!(
                UaSecurityPolicy::from_advertised(policy.as_uri().unwrap()),
                Some(policy)
            );
            assert_eq!(
                UaSecurityPolicy::from_advertised(policy.as_str()),
                Some(policy)
            );
        }
    }

    #[test]
    fn security_pair_compatibility() {
        use UaSecurityMode::{None as ModeNone, Sign, SignAndEncrypt};
        use UaSecurityPolicy::{Auto, Basic256Sha256, None as PolicyNone};
        // None policy requires None mode; Auto is on the policy axis.
        assert!(validate_security_pair(PolicyNone, ModeNone).is_ok());
        assert!(validate_security_pair(PolicyNone, Sign).is_err());
        assert!(validate_security_pair(PolicyNone, SignAndEncrypt).is_err());
        // Auto policy is decided against the server.
        assert!(validate_security_pair(Auto, ModeNone).is_ok());
        assert!(validate_security_pair(Auto, Sign).is_ok());
        assert!(validate_security_pair(Auto, SignAndEncrypt).is_ok());
        // Concrete policy with a protecting mode.
        assert!(validate_security_pair(Basic256Sha256, Sign).is_ok());
        assert!(validate_security_pair(Basic256Sha256, SignAndEncrypt).is_ok());
        assert!(validate_security_pair(Basic256Sha256, ModeNone).is_ok());
    }

    #[test]
    fn endpoint_security_legacy_json_migrates_to_enums() {
        let legacy = r#"{
            "protocol":"OpcUa",
            "host":"",
            "address":"opc.tcp://127.0.0.1:4840",
            "user":"",
            "ua_security_policy":"Авто",
            "ua_security_mode":"Sign"
        }"#;
        let ep: EndpointSpec = serde_json::from_str(legacy).unwrap();
        assert_eq!(ep.ua_security_policy, Some(UaSecurityPolicy::Auto));
        assert_eq!(ep.ua_security_mode, Some(UaSecurityMode::Sign));
        // New JSON carries stable ASCII tokens.
        let json = serde_json::to_string(&ep).unwrap();
        assert!(
            json.contains("\"ua_security_policy\":\"Auto\""),
            "got: {json}"
        );
        assert!(
            json.contains("\"ua_security_mode\":\"Sign\""),
            "got: {json}"
        );
        // Unknown legacy value is a hard error, not a silent Auto.
        let bad = legacy.replace("Авто", "NoSuchPolicy");
        assert!(serde_json::from_str::<EndpointSpec>(&bad).is_err());
    }
}
