//! Runtime language switching with a compile-checked string dictionary.
//!
//! All user-visible strings live in the `Tr` enum. `tr()` maps a key to text
//! for the current `Lang`. Each language is a separate exhaustive `match`, so
//! the compiler guarantees no string can be missing a translation: every `Tr`
//! variant must appear in each dictionary (RU/EN/ZH). A unit test walks
//! `Tr::ALL` to assert every variant yields non-empty text in all languages.

use opclook_contract::model::{BackendMsg, UaProtection};
use serde::{Deserialize, Serialize};

/// The three supported UI languages. Russian is the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[serde(rename = "ru")]
    Ru,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh")]
    Zh,
}

impl Default for Lang {
    fn default() -> Self {
        Lang::Ru
    }
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ru, Lang::En, Lang::Zh];

    /// Stable, ASCII serialization stored in `settings.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
            Lang::Zh => "zh",
        }
    }

    /// Endonym (native self-name) shown in the language menu. Intentional
    /// multilingual labels are not translated: each option is displayed in its
    /// own language ("Русский", "English", "中文"). Kept in one place so the UI
    /// layer never hardcodes them.
    pub fn endonym(self) -> &'static str {
        match self {
            Lang::Ru => "Русский",
            Lang::En => "English",
            Lang::Zh => "中文",
        }
    }

    pub fn from_str(s: &str) -> Option<Lang> {
        match s {
            "ru" | "RU" | "рус" | "Рус" | "Russian" => Some(Lang::Ru),
            "en" | "EN" | "eng" | "English" => Some(Lang::En),
            "zh" | "ZH" | "zh-Hans" | "中文" | "简体中文" | "Chinese" => Some(Lang::Zh),
            _ => None,
        }
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

macro_rules! translation_keys {
    ($( $(#[$meta:meta])* $key:ident, )*) => {
        /// Every translatable string in the application.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u16)]
        pub enum Tr { $( $(#[$meta])* $key, )* }

        impl Tr {
            /// Every key in declaration order, generated from the enum definition.
            pub const ALL: &'static [Tr] = &[$(Tr::$key,)*];
            pub const COUNT: usize = Self::ALL.len();

            pub fn all() -> Vec<Tr> {
                Self::ALL.to_vec()
            }
        }
    };
}

translation_keys! {
    // ---- Top menu ----
    MenuClient,
    TraceDropped,
    TraceReply,
    TraceSent,
    DiagnosticTraceHint,
    DiagnosticTraceCheck,
    NoReplyFromServer,
    WorkerPanicked,
    WorkerTimedOut,
    GatewayClosed,
    WorkerClosed,
    RequestIdsExhausted,
    // P2 bounded action payload integration.
    ActionPayloadInvalid,
    ReadBusy,
    AcquisitionTitle,
    AcquisitionExplicit,
    AcquisitionMode,
    AcquisitionSubscription,
    AcquisitionPoll,
    AcquisitionInterval,
    AcquisitionRequestedInterval,
    AcquisitionReadback,
    AcquisitionFreshness,
    AcquisitionCache,
    AcquisitionSource,
    AcquisitionUnsupported,
    AcquisitionInvalid,
    AcquisitionLegacy,
    AcquisitionCacheGap,
    AcquisitionEdit,
    AcquisitionApplyNext,
    AcquisitionApplyReconnect,
    AcquisitionPending,
    AcquisitionEpochChanged,
    AcquisitionCollision,
    AcquisitionActive,
    AcquisitionDraft,

    ReadEmpty,
    ItemKeyReused,
    ItemKeyLimit,
    ScanIdsExhausted,
    SessionIdsExhausted,
    ItemIdsExhausted,
    InvalidItemHandle,
    CsvIssueLine,
    CsvIssuesOmitted,
    CsvMalformedQuote,
    CsvMissingFields,
    CsvEmptyItem,
    CsvServerAmbiguous,
    CsvServerEmpty,
    CjkFontMissing,
    CjkFontLoaded,
    ResolvingName,
    AddDuplicateResult,
    TimeSource,
    TimeServer,
    TimeDa,
    TimeReceived,
    TimeOrigin,
    TimeOriginRead,
    TimeOriginSubscription,
    TimeOriginUnknown,
    TimeOrderUnknown,
    TimeOrderComparable,
    TimeOrderOlderHeld,
    TimeOrderEqualConflict,
    TimeOrderDiscontinuity,
    TimeOrderDiagnostic,
    ReadNeedInitial,
    ReadNeedPoll,
    ReadNeedRefresh,
    ReadNeedReadback,
    ReadNeedDirect,
    ReadTerminalLine,
    ReadTerminalReceived,
    ReadTerminalInitial,
    ReadTerminalMissing,
    ReadTerminalServiceError,
    ReadTerminalTimeout,
    ReadTerminalCancelled,
    ReadCancelRemoved,
    ReadCancelDisconnected,
    ReadCancelEpoch,
    ReadCancelExplicit,
    ReadTerminalDispatchFailed,
    ReadTerminalUnavailable,
    ReadTerminalUnexpected,
    ReadTerminalSummaryLoss,
    SettingsReadFailed,
    SettingsParseFailed,
    SettingsParseNoBackup,
    SettingsSaveFailed,
    SettingsSaveBlocked,
    SettingsAcquisitionMigrated,
    SettingsAcquisitionMigrationFailed,
    SettingsAcquisitionMigrationBlocked,
    NetworkCacheWarning,
    NetworkCacheLoaded,
    NetworkCacheSkipped,
    NetworkRefreshStart,
    NetworkRefreshError,
    NetworkRefreshInventory,
    NetworkRefreshDone,
    NetworkAdapterSkipped,
    NetworkAdapterDetails,
    NetworkSelectionDetails,
    NetworkSelectionStale,
    NetworkSelectionNone,
    NetworkSelectionCleared,
    ConnectInProgress,
    MenuServers,
    MenuSignals,
    MenuScript,
    MenuJournal,
    MenuHelp,
    MenuLanguage,
    LangRu,
    LangEn,
    LangZh,
    ConnectEllipsis,
    ActionWriteConnectionChanged,
    DiagnosticRedacted,
    PaletteTitle,
    PaletteExecute,
    PaletteSearch,
    PaletteNoResults,
    PaletteNeedsContext,
    PaletteBrowseNode,
    PaletteBrowseUnknown,
    PaletteBrowseMismatch,
    PaletteBrowseLimit,
    PaletteTarget,
    PaletteSelectOne,
    PaletteConnectionChanged,
    PaletteWriteStale,
    PaletteBrowse,
    PaletteWrite,
    ActionModalOpen,
    ActionSessionMissing,
    ActionSessionNotConnected,
    ActionProtocolMismatch,
    ActionImportInProgress,
    Disconnect,
    SettingsEllipsis,
    Exit,
    Reconnect,
    RefreshBranch,
    LoadTreeCsv,
    SaveTreeCsv,
    AddSelected,
    AddChildrenSignals,
    AddByName,
    RemoveSelected,
    ChangeValue,
    ChangeSelectedValues,
    SelectAll,
    ClearSelection,
    ColumnsEllipsis,
    Save,
    Load,
    CheckSyntax,
    ShowScript,
    ShowJournal,
    ShowWorkJournal,
    Clear,
    SaveJournal,
    LoadJournal,
    AboutProgram,

    // ---- Tooltips (menus) ----
    HintConnectUa,
    HintRefreshF5,
    HintLoadCsv,
    HintSaveCsv,
    HintEnterTree,
    HintAddChildren,
    HintDelSelected,
    HintEnterF2,
    HintCtrlA,
    HintEditColumns,

    // ---- Status bar / session state ----
    Connected,
    Connecting,
    NoLink,
    NoActiveServer,
    SignalsCount,
    Writing,
    WrittenSummary,
    UpdatedAt,
    RowsHint,
    TreeHint,
    KeyboardIdleHint,

    // ---- Bottom panels ----
    ScriptTab,
    JournalTab,
    HintScriptTab,
    HintJournalTab,
    SaveScriptLabel,
    LoadScriptLabel,
    ClearJournalLabel,
    SaveJournalLabel,
    ScriptsNotImpl,
    ThemeTitle,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    ThemeHint,
    HintScriptTabNotImpl,
    EntriesCount,

    // ---- Navigation / tree ----
    Servers,
    Connect,
    ConnectPrompt,
    Add,
    AddChildren,
    AddByNameShort,
    BrowseProto,
    UpdatingTree,
    LoadingTree,
    TreeNotLoaded,
    NoConnection,
    ResumeAttempts,
    StopAttempts,
    Refresh,
    CopyNodeName,
    NodeNameCopied,
    ExpandCollapse,
    /// Synthetic tree root listing server Views (UA).
    ViewsRoot,
    /// Checkbox restricting UA browse to Variable nodes.
    OnlyVars,
    /// Error when a «Добавить по имени» name cannot be resolved as a BrowsePath.
    NameNotResolved,

    // ---- Table ----
    TableEmpty,
    TableEmptyHelp,
    TableHint,
    /// Hint rendered INSIDE the signal table body when there are no rows yet.
    /// Tells the user how to populate it; rendered on the same column grid so
    /// the table headers stay visible (it never replaces the table).
    TableEmptyInner,
    /// UI1: accessible name / tooltip of the expander of a structured value.
    TableValueExpand,
    TableValueCollapse,
    /// UI10: second line of the empty table without a session: the keys.
    TableEmptyKeys,
    /// UI10: second line of the empty table with an active session.
    TableEmptyDragHint,
    /// UI10: the button of the empty table without a session.
    TableEmptyConnect,
    EscHint,
    PkmMoveColumn,
    MoveLeft,
    MoveRight,
    AddThisNode,
    AddChildrenCtx,
    CopyFullNameCtx,
    CopyValueCtx,
    SelectAllCtx,
    ClearSelectionCtx,
    SaveCsvCtx,
    /// Tags-area (table panel) context menu — opens the columns settings dialog.
    CtxColumns,
    /// Tags-area context menu — «Добавить по имени...».
    CtxAddByName,
    /// Tags-area context menu — «Обновить ветку».
    CtxRefreshBranch,
    /// Tags-area context menu — «Сохранить CSV...».
    CtxSaveCsv,
    /// Tags-area context menu — «Выделить все».
    CtxSelectAll,
    /// Tags-area context menu — «Снять выделение».
    CtxClearSelection,
    /// Tooltip on the columns-dialog drag handle.
    ColumnsDragHandle,
    /// Columns-dialog keyboard-reorder hint (Alt/Ctrl+Up/Down).
    ColumnsReorderHint,
    ValueCopied,
    FullNameCopied,
    Copied,
    ColumnMovedLeft,
    ColumnMovedRight,
    ColStatus,
    ColFullName,
    ColWriteType,
    ColType,
    ColValue,
    ColQuality,
    ColTime,
    ColServer,
    ColDescription,
    HintStatus,
    HintFullName,
    HintWriteType,
    HintType,
    HintValue,
    HintQuality,
    HintTime,
    HintServer,
    HintDescription,

    // ---- Status popup ----
    Server,
    ProtocolLabel,
    EndpointLabel,
    StateLabel,
    DetailsLabel,
    SubscribedLabel,
    /// UA push-subscription state in the server info popup.
    SubscriptionLabel,
    SubscriptionActive,
    SubscriptionInactive,
    SubscriptionFailed,
    SelectedLabel,
    LastUpdateLabel,
    ReconnectLabel,
    RetriesLabel,
    Close,
    AutoReconnectStopped,
    ReconnectingLine,
    ReconnectingAttempt,
    RetriesLine,
    /// «Запрос списка OPC DA серверов» host suffix (« на {host}»).
    ServerHostSuffix,
    /// «Поиск OPC UA серверов в сети (mDNS/LDS)...».
    SearchingUaNetwork,

    // ---- Connect dialog ----
    ConnectToServer,
    DaTab,
    UaTab,
    DaServersList,
    FoundServers,
    PollingServers,
    ListEmptyManual,
    SearchingUa,
    ListEmptyUrl,
    NetworkAddr,
    AddressLabel,
    HintLocalAddr,
    PortLabel,
    SearchLan,
    Search,
    /// Under the dialog fields: an empty address means this computer + local
    /// network.
    EmptySearchHint,
    /// UA-only placeholder for the host field (empty = this computer + local network).
    UaHostHint,
    /// UA-only placeholder for the port field (empty defaults to 4840).
    UaPortHint,
    /// Label of the optional "LDS host" field of the UA search.
    LdsHostLabel,
    /// Placeholder of the LDS host field.
    LdsHostHint,
    /// Tooltip of the LDS host field.
    LdsHostTip,
    /// The LDS host text is not a host name / address with an optional port.
    LdsHostInvalid,
    /// Journal line: the LDS/mDNS search also asks this LDS (`{0}` = host:port).
    SearchingUaLds,
    UserLabel,
    PasswordLabel,
    UrlLabel,
    HintProgId,
    HintUaUrl,
    Recent,
    Cancel,
    /// UA connect-dialog security controls (Task: security).
    SecPolicyLabel,
    SecModeLabel,
    AutoLabel,
    /// Security-policy combo values.
    SecPolicyNone,
    SecPolicyBasic128Rsa15,
    SecPolicyBasic256,
    SecPolicyBasic256Sha256,
    /// Security-mode combo values.
    SecModeNone,
    SecModeSign,
    SecModeSignAndEncrypt,
    /// Invalid policy/mode combination (placeholder: technical detail).
    SecurityInvalidCombination,
    /// Security resolution failed against the server (placeholder: detail).
    SecurityResolveFailed,
    TrustServerCertCheck,
    /// Tooltip of the trust checkbox: what "trust" means (this session only,
    /// validity period / host name / ApplicationUri are not checked).
    TrustServerCertTip,
    IdentityLabel,
    IdentityAnonymous,
    IdentityUserName,
    IdentityCertificate,
    SessionTimeoutLabel,
    SearchLdsMdns,
    NetworkServersNotFound,
    CertIdentityUnsupported,
    PkiPathLabel,
    ClientCertSubjectLabel,
    NoClientCert,
    SessionTimeoutInfo,
    /// UserName identity selected but the user name field is empty.
    UaUserNameRequired,

    // ---- Add signal dialog ----
    AddSignal,
    ServerLabel,
    SignalName,
    HintItemId,
    DisplayName,
    Optional,
    NotSelected,

    // ---- Write dialog ----
    ChangeSignalValue,
    SignalLabel,
    WriteTargetCount,
    CurrentValue,
    ServerTypeLabel,
    ReadOnly,
    CanonicalDaType,
    ServerDataType,
    WriteTypeLabel,
    ValueLabel,
    HintWriteValue,
    ConfirmAfterWrite,
    Write,

    // ---- Columns dialog ----
    ColumnsSetup,
    ColumnsHint,
    IndicatorNote,
    MovedFormat,
    Reset,
    Apply,

    // ---- Options dialog ----
    OptionsTitle,
    UpdateInterval,
    Seconds,
    /// Short unit suffix for the advanced timeout/interval inputs (« с»).
    SecondsShortSuffix,
    RecommendedOneSec,
    AutoReconnectTitle,
    AutoReconnectCheck,
    AutoReconnectWarn,
    DisplayOptions,
    ShowColorsCheck,
    ConfirmWriteCheck,
    AdvancedTitle,
    OpTimeoutLabel,
    TestIntervalLabel,
    DeadbandLabel,
    DeadbandHint,

    // ---- About dialog ----
    AboutTitle,
    TestOpcClient,
    RustEgui2026,
    OriginalProto,
    SupportedLabel,
    AboutDaLine,
    AboutUaLine,
    AboutTreeLine,
    AboutSettingsLine,
    ConnectedServers,
    NoConnectedServers,
    VendorNotAvail,
    LimitationsLabel,
    LimitScriptLine,
    LimitQualityLine,
    LimitDcomLine,

    // ---- Message / confirm dialogs ----
    ConfirmTitle,
    Yes,
    No,

    // ---- App-provided dialog content ----
    WriteError,
    WriteFailedName,
    WriteUnknownName,
    WriteNotSubmitted,
    WriteNotSubmittedName,
    AddSignalTitle,
    AddSignalBadNode,
    ChangeValueTitle,
    SameServerMsg,
    ConfirmWriteTitle,
    ConfirmWriteOne,
    ConfirmWriteMany,
    DisconnectConfirmTitle,
    DisconnectConfirmText,

    // ---- Script dialog ----
    ScriptTitle,
    ScriptNotImpl,

    // ---- CSV import dialog ----
    LoadTreeDialog,
    ImportStopped,
    ImportDone,
    ImportProgress,

    // ---- App / journal messages ----
    AppStarted,
    Ready,
    ConnectingTo,
    DisconnectingFrom,
    Reconnecting,
    ConnectedInfo,
    AttemptsExhausted20,
    AutoReconnectAtt,
    ReconnectRestoring,
    AutoReconnectStoppedByUser,
    AttemptsExhausted,
    ReconnectBackoff,
    ReconnectStopped,
    AttemptsExhaustedFor,
    ConnLostReconnect,
    ReadTimeout,
    ServersFoundUa,
    ServersFoundDa,
    ReqServerList,
    SearchingUaHost,
    AddFailedName,
    NoChildSignals,
    WrittenValue,
    WriteAcceptedStatus,
    WriteAsyncStatus,
    WriteUncertainStatus,
    WriteRejectedStatus,
    WriteUnknownStatus,
    WriteOutcomeForItem,
    WriteFailed,
    WriteDoneSummary,
    StaleData,
    NoData,
    WaitingValue,
    SignalAdded,
    AddedSignals,
    AddFailedEmpty,
    RemovedSignals,
    OptionsSaved,
    ScriptSaved,
    ErrorOp,
    OpConnect,
    OpDisconnect,
    OpBrowse,
    OpAdd,
    OpRemove,
    OpRead,
    OpWrite,
    OpTest,
    OpDiscover,
    OpSearchServers,
    OpBrowseViews,
    OpResolvePath,
    OpSearchNetwork,
    OpSubscription,
    OpUaRuntime,
    NotWritable,
    WriteRequested,
    ScriptNotImplNote,
    ScriptSavedPath,
    ScriptSaveErr,
    ScriptLoadedPath,
    ScriptLoadErr,
    JournalSaved,
    JournalSaveErr,
    JournalLoaded,
    JournalLoadErr,
    TableSaved,
    CsvSaveErr,
    CsvReadErr,
    CsvNoSignals,
    CsvServerNotConnected,
    NotInteger,
    NotUnsigned,
    NotNumber,
    NotBool,
    WriteInputTooLarge,
    NotDatetime,
    ValueNotServer,
    PortInvalid,
    HostInvalid,
    NoServerSelected,
    NameRequired,

    // ---- Drag & drop ----
    DropToAdd,
    DropCrossSession,
    DropEmptyBranch,
    /// Live progress of a recursive add-children job, shown in the tree panel
    /// header / journal: «adding children: {0}…».
    AddChildrenProgress,
    /// Warning journaled when a recursive add-children job hits the node cap.
    AddChildrenCapHit,
    /// Journaled when a dropped Branch (recursive job) produced nothing to add.
    FolChildrenNone,

    // ---- Session tabs (tab bar / containers) ----
    /// Tooltip on the «+» button (open the connect dialog).
    AddTabTooltip,
    /// Tooltip on a tab's close «×» button (hide the view, keep the container).
    CloseTabTooltip,
    /// Hint shown on a connect-dialog server row (double-click opens/restores a tab).
    DoubleClickServerHint,
    /// Context-menu action that fully drops the session container.
    RemoveFromList,
    RemoveFromListConfirmTitle,
    /// Substitution: server display name.
    RemoveFromListConfirmText,
    /// Empty-state hint under the tab bar when no tab is open.
    EmptyTabsHint,
    /// Empty-tree action button: re-read the root of the active session (the same as F5).
    RefreshF5Button,
    /// Empty-state action button when there is no session: open the connect dialog (the same as Ctrl+N).
    /// Tooltip of the left panel's «add» icon (adds the selected tree node).
    AddNodeTooltip,
    /// Tooltip of the left panel's «add by name» icon.
    AddByNameTooltip,
    /// Accessible name of a tree node's expander when the node is collapsed.
    TreeExpandLabel,
    /// Accessible name of a tree node's expander when the node is expanded.
    TreeCollapseLabel,

    // ---- Servers area (left panel) ----
    /// Full-width button that opens the connect dialog.
    AddServer,
    /// Tooltip on the servers-row «×» (fully removes the container + tab).
    RemoveServerTooltip,

    // ---- Toolbar reconnect ----
    /// Toolbar «Переподключиться» icon tooltip.
    ReconnectTooltip,
    /// Reconnect icon with an in-icon attempt counter «{n}/{cap}».
    ReconnectCount,
    /// Tab-strip «+»: open the connect dialog.
    NewConnection,

    // ---- Network settings tab ----
    NetworkTab,
    NetworkAuto,
    NetworkRefresh,
    /// Network settings apply immediately; the footer button only closes.
    NetworkApplyHint,
    NetworkAdaptersTitle,
    NetworkPreferredLine,
    NetworkUsedLine,
    NetworkFallbackLine,
    NetworkAddressesLine,
    NetworkGatewayLine,
    NetworkMetricLine,
    NetworkStateLabel,
    NetworkTypePhysical,
    NetworkTypeVirtual,
    NetworkTypeLoopback,
    NetworkNoAdapters,
    NetworkErrorLine,
    NetworkFallbackPreferredMissing,
    NetworkFallbackPreferredDown,
    NetworkFallbackPreferredNoAddress,
    NetworkFallbackNoPhysical,
    NetworkFallbackAutoPhysical,
    NetworkFallbackAutoLoopback,
    NetworkLoopbackName,
    NetworkAddressChoice,
    NetworkStateUp,
    NetworkStateDown,
    NetworkStateTesting,
    NetworkStateDormant,
    NetworkStateNotPresent,
    NetworkStateLowerLayerDown,
    NetworkStateUnknown,

    // ---- UA subnet scan (N+1) ----
    /// Button next to «Поиск через LDS/mDNS».
    SearchUaSubnet,
    /// Journal line when the scan starts.
    SearchingUaSubnet,
    /// Progress line: checked / total / found.
    UaScanProgress,
    /// Stop button while the scan runs.
    UaScanStop,
    /// Confirmation prompt for a subnet above `SCAN_CONFIRM_THRESHOLD`.
    UaScanConfirmText,
    /// Backend refusal for an over-large subnet.
    UaScanTooLarge,
    /// Localized wrapper for an arbitrary scanner error.
    UaScanFailed,
    /// Scan completed with no servers.
    UaScanNoResults,
    /// No lease/subnet is available for the scan.
    UaScanNoLease,

    // ---- Adapter selection (no automatic mode) ----
    NetworkAdapterCombo,
    NetworkNotSelected,
    NetworkSelectAdapterHint,
    NetworkAdapterDescription,
    NetworkDaRouteLine,
    /// Compact status: a saved GUID is not present in the cached list.
    NetworkStaleAdapter,
    /// Empty cache before the first explicit «Обновить».
    NetworkEmptyList,
    /// Short localized status when the Windows inventory query fails.
    NetworkRefreshFailed,
    /// No suitable local address for the selected adapter.
    NetworkNoIp,
    /// Remove a stale saved adapter/IP while keeping the cached list.
    NetworkForget,

    // ---- UA discovery results tree (server → URL → security) ----
    /// Server row summary. Substitutions: URL count, security option count.
    UaTreeSummary,
    /// Hover text of a URL whose security options were not reported
    /// (LDS/mDNS discovery URL): they are resolved on connect (Auto).
    UaTreeSecurityOnConnect,
    /// Suffix of an advertised security option this client cannot use.
    UaTreeUnsupported,
    /// Server row body when the server reported no connection URL.
    UaTreeNoEndpoints,
    SecPolicyAes128Sha256RsaOaep,
    SecPolicyAes256Sha256RsaPss,

    // ---- UA session cap ----
    UaSessionCounter,
    UaSessionLimitReached,

    // ---- DA route helper / steering ----
    SelectAdapterRequired,
    DaRouteError,
    DaRouteNoAdapter,
    DaRouteHelperStarting,
    DaRouteHelperUnavailable,
    DaRouteConflict,
    DaRouteNoGateway,
    DaRouteResolve,
    DaRouteAdd,
    DaRouteVerify,
    DaRouteHelperRunning,
    DaRouteHelperIdle,
}

/// Look up the translation for `key` in `lang`.
pub fn tr(lang: Lang, key: Tr) -> &'static str {
    match lang {
        Lang::Ru => ru(key),
        Lang::En => en(key),
        Lang::Zh => zh(key),
    }
}

/// Substitute `{}`/`{0}` placeholders in a template with the given arguments.
///
/// `{}` consumes arguments in order; `{n}` addresses a specific argument. An
/// out-of-range index renders a `?` instead of panicking.
pub fn fmt(lang: Lang, key: Tr, args: &[&dyn std::fmt::Display]) -> String {
    let template = tr(lang, key);
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars().peekable();
    let mut positional = 0usize;
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut body = String::new();
            let mut closed = false;
            for _ in 0..4 {
                match chars.next() {
                    Some('}') => {
                        closed = true;
                        break;
                    }
                    Some(ch) => body.push(ch),
                    None => break,
                }
            }
            if !closed {
                out.push('{');
                out.push_str(&body);
                continue;
            }
            let index = if !body.is_empty() && body.chars().all(|b| b.is_ascii_digit()) {
                body.parse::<usize>().unwrap_or(positional)
            } else {
                positional
            };
            match args.get(index) {
                Some(a) => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "{a}");
                }
                None => out.push('?'),
            }
            positional += 1;
        } else {
            out.push(c);
        }
    }
    out
}

fn ru(key: Tr) -> &'static str {
    use Tr::*;
    match key {
        // ---- Top menu ----
        MenuClient => "OPC клиент",
        TraceDropped => "← #{0} (сессия {1}): нет ожидающего запроса, ответ отброшен",
        TraceReply => "← #{0} {1} (сессия {2}): {3} мс",
        TraceSent => "→ #{0} {1} (сессия {2})",
        DiagnosticTraceHint => {
            "Номер запроса, операция, сессия и время ответа в мс; для разбора зависаний. Журнал растёт быстро."
        }
        DiagnosticTraceCheck => "Диагностика: писать запросы и ответы в журнал",
        NoReplyFromServer => "Нет ответа от сервера за {0} с",
        WorkerPanicked => "Рабочий поток {0} завершился с паникой",
        WorkerTimedOut => "Истекло время завершения рабочего потока {0}",
        GatewayClosed => "Шлюз остановлен",
        WorkerClosed => "Канал рабочего потока закрыт",
        RequestIdsExhausted => "Исчерпаны идентификаторы запросов",
        ActionPayloadInvalid => {
            "Параметры действия превышают ограничения размера, глубины или числа элементов. Уменьшите объём данных"
        }
        ReadBusy => "Чтение этой сессии уже выполняется",
        AcquisitionTitle => "Получение данных",
        AcquisitionEdit => "Настроить профиль соединения",
        AcquisitionApplyNext => "Сохранить для следующего ручного подключения",
        AcquisitionApplyReconnect => "Сохранить и переподключить",
        AcquisitionPending => "Сохранён профиль для следующего ручного подключения",
        AcquisitionEpochChanged => "Соединение изменилось или закрыто; откройте настройки снова",
        AcquisitionCollision => "Соединение с таким профилем уже существует",
        AcquisitionActive => "Активный профиль",
        AcquisitionDraft => "Черновик",
        AcquisitionExplicit => "Использовать профиль соединения",
        AcquisitionLegacy => "Прежние общие настройки и резервный опрос",
        AcquisitionCacheGap => "Режим кэша UA ещё не реализован в OPCLook",
        AcquisitionMode => "Режим обновления",
        AcquisitionSubscription => "Подписка",
        AcquisitionPoll => "Опрос",
        AcquisitionInterval => "Интервал, мс",
        AcquisitionRequestedInterval => "Запрошенный интервал, мс",
        AcquisitionReadback => "Читать после записи",
        AcquisitionFreshness => "Источник чтения",
        AcquisitionCache => "Допускается кэш",
        AcquisitionSource => "Запросить источник",
        AcquisitionUnsupported => "Недоступно в текущем адаптере",
        AcquisitionInvalid => "Неверные или неподдерживаемые настройки получения данных",
        ReadEmpty => "Нет элементов для чтения",
        ItemKeyReused => "Идентификатор элемента уже использован в этом соединении",
        ItemKeyLimit => "Достигнут предел идентификаторов элементов для соединения",
        ScanIdsExhausted => "Исчерпаны идентификаторы сканирования",
        SessionIdsExhausted => "Исчерпаны идентификаторы сессий",
        ItemIdsExhausted => "Исчерпаны идентификаторы сигналов",
        InvalidItemHandle => "Получен неверный идентификатор сигнала",
        CsvServerAmbiguous => {
            "Строка CSV пропущена: имени сервера «{0}» соответствует несколько подключений, укажите точное имя"
        }
        CsvIssueLine => "Строка CSV {0}: {1}",
        CsvIssuesOmitted => "Ещё ошибок CSV: {0}",
        CsvMalformedQuote => "неверное кавычение",
        CsvMissingFields => "не хватает полей",
        CsvEmptyItem => "пустой идентификатор сигнала",
        CsvServerEmpty => "Строка CSV пропущена: для сигнала «{0}» не указан сервер",
        CjkFontMissing => {
            "Не найден шрифт для китайского текста (msyh.ttc, simsun.ttc): интерфейс на 中文 будет показан квадратами"
        }
        CjkFontLoaded => "Шрифт для китайского текста: {0}",
        ResolvingName => "Поиск узла…",
        AddDuplicateResult => "Сервер вернул несколько результатов регистрации для одного сигнала",
        TimeSource => "Время источника",
        TimeServer => "Время сервера",
        TimeDa => "Время DA",
        TimeReceived => "Получено локально",
        TimeOrigin => "Происхождение",
        TimeOriginRead => "Чтение",
        TimeOriginSubscription => "Подписка",
        TimeOriginUnknown => "Неизвестно",
        TimeOrderUnknown => "Порядок по времени источника не подтверждён",
        TimeOrderComparable => "Сравнимое время источника",
        TimeOrderOlderHeld => "Более старый образец временно удержан",
        TimeOrderEqualConflict => "Одинаковое время, разные значения: порядок неоднозначен",
        TimeOrderDiscontinuity => "Возможный скачок часов: порядок неоднозначен",
        TimeOrderDiagnostic => "Диагностический результат",
        ReadNeedInitial => "начальное значение",
        ReadNeedPoll => "опрос",
        ReadNeedRefresh => "обновление",
        ReadNeedReadback => "чтение после записи",
        ReadNeedDirect => "прямое чтение",
        ReadTerminalLine => {
            "Чтение: соединение {0}, эпоха {1}, сигнал {2}, запрос {3}, цель {4}: {5}"
        }
        ReadTerminalReceived => "Получен результат: качество {0}; причина: {1}",
        ReadTerminalInitial => "Начальное уведомление: качество {0}; причина: {1}",
        ReadTerminalMissing => "Сервер не вернул результат для сигнала",
        ReadTerminalServiceError => "Ошибка чтения: {0}",
        ReadTerminalTimeout => "Тайм-аут чтения, с: {0}",
        ReadTerminalCancelled => "Чтение отменено: {0}",
        ReadCancelRemoved => "сигнал удалён",
        ReadCancelDisconnected => "соединение закрыто",
        ReadCancelEpoch => "соединение изменилось",
        ReadCancelExplicit => "операция отменена",
        ReadTerminalDispatchFailed => "Не удалось отправить чтение: {0}",
        ReadTerminalUnavailable => "Чтение недоступно: исчерпаны идентификаторы",
        ReadTerminalUnexpected => "Некорректный ответ чтения",
        ReadTerminalSummaryLoss => "Результаты чтения объединены: {0}; не помещены в буфер: {1}",
        SettingsReadFailed => {
            "Не удалось прочитать файл настроек {0}: {1}. Файл не будет перезаписан"
        }
        SettingsParseFailed => {
            "Файл настроек {0} повреждён ({1}). Копия сохранена: {2}. Используются значения по умолчанию"
        }
        SettingsParseNoBackup => {
            "Файл настроек {0} повреждён ({1}), копию сделать не удалось ({2}). Файл не будет перезаписан, используются значения по умолчанию"
        }
        SettingsSaveFailed => "Не удалось сохранить настройки {0}: {1}",
        SettingsAcquisitionMigrated => {
            "Перенесены профили: UA {0}, DA {1}; интервал {2} мс. UA: подписка, разовые чтения из источника, без чтения после записи; прежний резервный опрос poll_only отключён. DA: опрос Sync, кэш, без чтения после записи."
        }
        SettingsAcquisitionMigrationFailed => {
            "Не удалось перенести профили: старый интервал {0} с вне допустимого диапазона или переполнен. Исходный файл защищён от записи; исправьте интервал и перезапустите приложение."
        }
        SettingsAcquisitionMigrationBlocked => {
            "Старый профиль недоступен: исправьте интервал в файле настроек и перезапустите приложение."
        }
        SettingsSaveBlocked => {
            "Не удалось сохранить настройки {0}: существующий файл повреждён или недоступен, а его резервная копия не создана. Файл не будет перезаписан"
        }
        NetworkCacheWarning => "Кэш сетевых адаптеров: {0}",
        NetworkCacheLoaded => "Загружен кэш сетевых адаптеров: {0}",
        NetworkCacheSkipped => "Пропущена запись кэша: {0}",
        NetworkRefreshStart => "Обновление сетевых адаптеров начато",
        NetworkRefreshError => "Не удалось обновить сетевые адаптеры: {0}",
        NetworkRefreshInventory => "Найдено сетевых адаптеров: {0}",
        NetworkRefreshDone => "Обновление завершено, доступных адаптеров: {0}",
        NetworkAdapterSkipped => "Адаптер пропущен: {0}",
        NetworkAdapterDetails => "Сетевой адаптер: {0}",
        NetworkSelectionDetails => "Выбран сетевой адаптер: {0}",
        NetworkSelectionStale => "Выбранный адаптер недоступен: {0}",
        NetworkSelectionNone => "Сетевой адаптер не выбран",
        NetworkSelectionCleared => "Выбор сетевого адаптера сброшен",
        ConnectInProgress => "Подключение…",
        MenuServers => "Серверы",
        MenuSignals => "Сигналы",
        MenuScript => "Скрипт",
        MenuJournal => "Журнал",
        MenuHelp => "Справка",
        MenuLanguage => "Язык",
        LangRu => "Русский",
        LangEn => "Английский",
        LangZh => "Китайский (简体中文)",
        ConnectEllipsis => "Подключиться...",
        ActionWriteConnectionChanged => {
            "Запись отменена: соединение изменилось. Проверьте значение и подтвердите запись заново."
        }
        DiagnosticRedacted => "[скрыто]",
        PaletteTitle => "Палитра команд",
        PaletteExecute => "Выполнить",
        PaletteSearch => "Поиск команды",
        PaletteNoResults => "Команды не найдены",
        PaletteNeedsContext => "Требуются параметры и контекст",
        PaletteBrowseNode => "NodeId ветки (пусто — корень)",
        PaletteBrowseUnknown => "Недопустимый или слишком длинный идентификатор",
        PaletteBrowseMismatch => "Сервер вернул ответ для другой ветки",
        PaletteBrowseLimit => "Исчерпаны идентификаторы дерева",
        PaletteTarget => "Подключение: {0}",
        PaletteSelectOne => "Выберите один сигнал текущего подключения",
        PaletteConnectionChanged => "Подключение изменилось; откройте палитру снова",
        PaletteWriteStale => "Подключение изменилось; откройте запись снова",
        PaletteBrowse => "Обзор ветки",
        PaletteWrite => "Запись значения",
        ActionModalOpen => "Сначала закройте открытый диалог",
        ActionSessionMissing => "Сессия не найдена",
        ActionSessionNotConnected => "Сессия не подключена",
        ActionProtocolMismatch => "Протокол не соответствует сессии",
        ActionImportInProgress => "Дождитесь завершения импорта",
        Disconnect => "Отключиться",
        SettingsEllipsis => "Настройка...",
        Exit => "Выход",
        Reconnect => "Восстановить подключение",
        RefreshBranch => "Обновить ветку",
        LoadTreeCsv => "Загрузить дерево сигналов (CSV)...",
        SaveTreeCsv => "Сохранить дерево сигналов (CSV)...",
        AddSelected => "Добавить выбранный",
        AddChildrenSignals => "Добавить дочерние сигналы",
        AddByName => "Добавить по имени...",
        RemoveSelected => "Удалить выбранные",
        ChangeValue => "Изменить значение...",
        ChangeSelectedValues => "Изменить выбранные значения...",
        SelectAll => "Выделить все",
        ClearSelection => "Снять выделение",
        ColumnsEllipsis => "Столбцы...",
        Save => "Сохранить",
        Load => "Загрузить...",
        CheckSyntax => "Проверить синтаксис",
        ShowScript => "Показать скрипт",
        ShowJournal => "Показать журнал",
        ShowWorkJournal => "Показать журнал работы",
        Clear => "Очистить",
        SaveJournal => "Сохранить...",
        LoadJournal => "Загрузить...",
        AboutProgram => "О программе...",

        // ---- Tooltips ----
        HintConnectUa => "Подключение к OPC DA/UA серверу",
        HintRefreshF5 => "{0} — обновить ветку",
        HintLoadCsv => "Загрузить сигналы из CSV-файла",
        HintSaveCsv => "Сохранить сигналы в CSV-файл",
        HintEnterTree => "Enter в дереве",
        HintAddChildren => "Добавить все сигналы ветки",
        HintDelSelected => "{0} — удалить выбранные",
        HintEnterF2 => "{0} — изменить значение",
        HintCtrlA => "{0} — выделить все",
        HintEditColumns => "Настроить состав и порядок столбцов",

        // ---- Status bar ----
        Connected => "Подключено",
        Connecting => "Подключение",
        NoLink => "Нет связи",
        NoActiveServer => "нет активного сервера",
        SignalsCount => "Сигналов: {0} | Выбрано: {1}",
        Writing => "Запись...",
        WrittenSummary => "Принято сервером: {0} из {1}",
        UpdatedAt => "Обновлено {0}",
        RowsHint => {
            "Таблица: ↑↓ Shift+PgUp/PgDn Home/End {0} {1} {3} {4} {5} · {6} — дерево · {2} — сервер"
        }
        TreeHint => "Дерево: ↑↓ ← → Space {1} {2} · {3} — таблица · {0} — сервер",
        KeyboardIdleHint => "{1} — таблица / дерево · {0} — подключиться",

        // ---- Bottom panels ----
        ScriptTab => "Скрипт",
        JournalTab => "Журнал",
        HintScriptTab => "Скрипт — редактор и исполнение",
        HintJournalTab => "Журнал — протокол работы",
        SaveScriptLabel => "Сохранить скрипт",
        LoadScriptLabel => "Загрузить скрипт",
        ClearJournalLabel => "Очистить журнал",
        SaveJournalLabel => "Сохранить журнал...",
        ScriptsNotImpl => "Скрипты: интерпретатор не реализован",
        ThemeTitle => "Тема оформления",
        ThemeSystem => "Как в системе",
        ThemeLight => "Светлая",
        ThemeDark => "Тёмная",
        ThemeHint => "Применяется после «Сохранить»",
        HintScriptTabNotImpl => {
            "Скрипт — не реализовано: интерпретатор пока отсутствует, можно только редактировать и сохранять текст"
        }
        EntriesCount => "записей: {0}",

        // ---- Navigation ----
        Servers => "Серверы",
        Connect => "Подключиться",
        ConnectPrompt => "Подключитесь к OPC DA или OPC UA серверу.",
        Add => "Добавить",
        AddChildren => "Дочерние",
        AddByNameShort => "По имени...",
        BrowseProto => "Обзор: {0}",
        UpdatingTree => "Обновление дерева...",
        LoadingTree => "Загрузка дерева...",
        TreeNotLoaded => "Дерево не загружено.",
        NoConnection => "Нет соединения.",
        ResumeAttempts => "Возобновить попытки",
        StopAttempts => "Остановить попытки",
        Refresh => "Обновить",
        CopyNodeName => "Копировать имя узла",
        NodeNameCopied => "Имя узла скопировано",
        ExpandCollapse => "Развернуть / свернуть",
        ViewsRoot => "Представления",
        OnlyVars => "Только переменные",
        NameNotResolved => "Имя \"{0}\" не разрешено как путь OPC UA (BrowsePath)",

        // ---- Table ----
        TableEmpty => "Таблица сигналов пуста.",
        TableEmptyHelp => "Подключитесь к серверу, раскройте ветку и добавьте сигналы.",
        TableHint => "Подсказка: двойной щелчок в дереве по сигналу или ПКМ → Добавить.",
        TableEmptyInner => "Нет сигналов — добавьте из дерева, через ПКМ или перетащите сюда",
        TableValueExpand => "Развернуть значение",
        TableValueCollapse => "Свернуть значение",
        TableEmptyKeys => "{0} — подключиться к серверу · {1} — дерево и таблица",
        TableEmptyDragHint => "Перетащите теги из дерева или нажмите Enter на узле",
        TableEmptyConnect => "Подключиться ({0})",
        EscHint => "Нажмите Esc, чтобы снять выделение. ПКМ по строке — меню.",
        PkmMoveColumn => "ПКМ — переместить столбец",
        MoveLeft => "Влево",
        MoveRight => "Вправо",
        AddThisNode => "Добавить (по этому узлу)",
        AddChildrenCtx => "Добавить дочерние",
        CopyFullNameCtx => "Копировать полное имя",
        CopyValueCtx => "Копировать значение",
        SelectAllCtx => "Выделить всё",
        ClearSelectionCtx => "Снять выделение",
        SaveCsvCtx => "Сохранить CSV",
        CtxColumns => "Столбцы...",
        CtxAddByName => "Добавить по имени...",
        CtxRefreshBranch => "Обновить ветку",
        CtxSaveCsv => "Сохранить CSV...",
        CtxSelectAll => "Выделить все",
        CtxClearSelection => "Снять выделение",
        ColumnsDragHandle => "Перетащите, чтобы переместить",
        ColumnsReorderHint => "Alt+↑/↓ или Ctrl+↑/↓ — переместить строку",
        ValueCopied => "Значение скопировано",
        FullNameCopied => "Полное имя скопировано",
        Copied => "Скопировано",
        ColumnMovedLeft => "Столбец \"{0}\" перемещён влево",
        ColumnMovedRight => "Столбец \"{0}\" перемещён вправо",
        ColStatus => "Статус",
        ColFullName => "Полное имя",
        ColWriteType => "Записываемый тип",
        ColType => "Тип",
        ColValue => "Значение",
        ColQuality => "Качество",
        ColTime => "Время изменения",
        ColServer => "OPC сервер",
        ColDescription => "Описание",
        HintStatus => "Состояние строки",
        HintFullName => "Полное имя сигнала в сервере",
        HintWriteType => "Тип сигнала, записываемый пользователем",
        HintType => "Тип сигнала в сервере",
        HintValue => "Текущее значение сигнала в сервере",
        HintQuality => "Характеристика достоверности значения сигнала",
        HintTime => "Время последнего изменения сигнала",
        HintServer => "Программный идентификатор сервера",
        HintDescription => "Описание сигнала",

        // ---- Status popup ----
        Server => "Сервер",
        ProtocolLabel => "Протокол:",
        EndpointLabel => "Точка подключения:",
        StateLabel => "Состояние:",
        DetailsLabel => "Сведения:",
        SubscribedLabel => "Подписано сигналов:",
        SubscriptionLabel => "Подписка:",
        SubscriptionActive => "активна",
        SubscriptionInactive => "неактивна",
        SubscriptionFailed => "ошибка",
        SelectedLabel => "Выбрано:",
        LastUpdateLabel => "Последнее обновление:",
        ReconnectLabel => "Переподключение:",
        RetriesLabel => "Попыток восстановления:",
        Close => "Закрыть",
        AutoReconnectStopped => "автовосстановление остановлено (попыток: {0})",
        ReconnectingLine => "переподключение: попытка {0}, повтор через {1} с (лимит {2})",
        ReconnectingAttempt => "переподключение: попытка {0}",
        RetriesLine => "{0} (лимит {1})",
        ServerHostSuffix => " на {0}",
        SearchingUaNetwork => "Поиск OPC UA серверов в сети (mDNS/LDS)...",

        // ---- Connect dialog ----
        ConnectToServer => "Подключение к OPC серверу",
        DaTab => "OPC DA (COM)",
        UaTab => "OPC UA",
        DaServersList => "Список OPC серверов:",
        FoundServers => "Найденные серверы:",
        PollingServers => "Опрос OPC серверов...",
        ListEmptyManual => "Список пуст. Укажите ProgID вручную.",
        SearchingUa => "Поиск OPC UA серверов...",
        ListEmptyUrl => "Список пуст. Укажите URL вручную.",
        NetworkAddr => "Сетевой адрес:",
        AddressLabel => "Адрес:",
        HintLocalAddr => "пусто = локальный (необязательно)",
        PortLabel => "Порт:",
        SearchLan => "Поиск в локальной сети",
        Search => "Поиск",
        EmptySearchHint => "Пусто — поиск на этом компьютере и в локальной сети",
        UaHostHint => "адрес или пусто",
        UaPortHint => "порт (по умолчанию 4840)",
        LdsHostLabel => "LDS-хост",
        LdsHostHint => "необязательно, хост[:порт]",
        LdsHostTip => {
            "Сервер обнаружения (LDS), у которого «Поиск через LDS/mDNS» спрашивает список серверов. Порт по умолчанию 4840. Пусто — только этот компьютер и mDNS."
        }
        LdsHostInvalid => {
            "LDS-хост: ожидается имя или адрес, при необходимости с портом (хост:порт)"
        }
        SearchingUaLds => "Поиск OPC UA серверов у LDS {0} (и mDNS)...",
        UserLabel => "Пользователь:",
        PasswordLabel => "Пароль:",
        UrlLabel => "URL:",
        HintProgId => "ProgID сервера, например Alpha.Server",
        HintUaUrl => "opc.tcp://host:4840",
        Recent => "Недавние",
        Cancel => "Отмена",
        SecPolicyLabel => "Политика безопасности:",
        SecModeLabel => "Режим безопасности:",
        AutoLabel => "Авто",
        SecPolicyNone => "Без политики",
        SecPolicyBasic128Rsa15 => "Basic128Rsa15",
        SecPolicyBasic256 => "Basic256",
        SecPolicyBasic256Sha256 => "Basic256Sha256",
        SecModeNone => "Без защиты",
        SecModeSign => "Подпись",
        SecModeSignAndEncrypt => "Подпись и шифрование",
        SecurityInvalidCombination => "Недопустимое сочетание политики и режима безопасности: {0}",
        SecurityResolveFailed => "Не удалось выбрать точку безопасности: {0}",
        TrustServerCertCheck => "Доверять сертификату сервера (только на эту сессию)",
        TrustServerCertTip => {
            "Сертификат сервера принимается только на время этого подключения и не сохраняется навсегда. Срок действия сертификата, имя хоста и ApplicationUri не проверяются. Чтобы доверять серверу постоянно, импортируйте его сертификат в PKI-хранилище."
        }
        IdentityLabel => "Идентичность:",
        IdentityAnonymous => "Анонимно",
        IdentityUserName => "Имя пользователя",
        IdentityCertificate => "Сертификат",
        SessionTimeoutLabel => "Таймаут сессии (мс):",
        SearchLdsMdns => "Поиск через LDS/mDNS",
        NetworkServersNotFound => "Серверы в сети не найдены (mDNS/LDS не отвечает)",
        CertIdentityUnsupported => {
            "Выбранная точка безопасности не поддерживает вход по сертификату"
        }
        PkiPathLabel => "PKI-хранилище:",
        ClientCertSubjectLabel => "Сертификат клиента:",
        NoClientCert => "не создан (появится после первого подключения)",
        SessionTimeoutInfo => {
            "Запрашиваемый таймаут сессии, мс (5000–3600000). Сервер может пересмотреть значение."
        }
        UaUserNameRequired => "Укажите имя пользователя для выбранной идентичности.",

        // ---- Add signal ----
        AddSignal => "Добавить сигнал",
        ServerLabel => "Сервер:",
        SignalName => "Имя сигнала:",
        HintItemId => "OPC DA item id или OPC UA NodeId",
        DisplayName => "Отображаемое имя:",
        Optional => "необязательно",
        NotSelected => "не выбран",

        // ---- Write dialog ----
        ChangeSignalValue => "Изменить значение сигнала",
        SignalLabel => "Сигнал: {0}",
        WriteTargetCount => "Количество выбранных сигналов: {0}",
        CurrentValue => "Текущее значение: {0}",
        ServerTypeLabel => "Тип сервера: {0}",
        ReadOnly => "Сигнал только для чтения (запись недоступна)",
        CanonicalDaType => "Канонический тип DA: {0}",
        ServerDataType => "Тип данных сервера: {0}",
        WriteTypeLabel => "Тип записи:",
        ValueLabel => "Значение:",
        HintWriteValue => "true / 123 / 12.5 / текст / 2026-01-31 12:00:00",
        ConfirmAfterWrite => "После нажатия \"Записать\" потребуется подтверждение.",
        Write => "Записать",

        // ---- Columns dialog ----
        ColumnsSetup => "Настройка столбцов",
        ColumnsHint => "Выберите столбцы и задайте порядок следования.",
        IndicatorNote => "(индикатор)",
        MovedFormat => "Перемещено: {0}",
        Reset => "Сбросить",
        Apply => "Применить",

        // ---- Options dialog ----
        OptionsTitle => "Параметры",
        UpdateInterval => "Интервал обновления",
        Seconds => "секунд",
        SecondsShortSuffix => " с",
        RecommendedOneSec => "Рекомендуемое значение: 1 секунда.",
        AutoReconnectTitle => "Автоматическое восстановление связи",
        AutoReconnectCheck => "Автоматически восстанавливать связь с сервером",
        AutoReconnectWarn => "Внимание! Установка флага может привести к запуску сервера.",
        DisplayOptions => "Параметры отображения",
        ShowColorsCheck => "Отображать сигналы различными цветами",
        ConfirmWriteCheck => "Запрашивать подтверждение изменения сигнала",
        AdvancedTitle => "Расширенные",
        OpTimeoutLabel => "Таймаут операций (1–1800 с). Рекомендуется 60 с.",
        TestIntervalLabel => "Интервал тестирования связи (1–1800 с). Рекомендуется 60 с.",
        DeadbandLabel => "Мёртвая зона (абс.)",
        DeadbandHint => {
            "OPC UA: изменение значения присылается, только если оно сменилось сильнее зоны (0 = выкл)."
        }

        // ---- About dialog ----
        AboutTitle => "О программе",
        TestOpcClient => "OPCLook",
        RustEgui2026 => "Rust + egui, 2026",
        OriginalProto => {
            "OPC DA 2.0: COM-интерфейсы IOPCServer / IOPCItemMgt / IOPCSyncIO / IOPCBrowseServerAddressSpace."
        }
        SupportedLabel => "Поддерживается:",
        AboutDaLine => "• OPC DA 2.0 через COM (локально и по DCOM);",
        AboutUaLine => {
            "• OPC UA через async-opcua 0.19 (SecurityPolicy None, anonymous / username);"
        }
        AboutTreeLine => {
            "• дерево адресного пространства, таблица сигналов, чтение/запись, журнал;"
        }
        AboutSettingsLine => "• сохранение настроек в %APPDATA%\\opclook\\settings.json.",
        ConnectedServers => "Подключённые серверы:",
        NoConnectedServers => "Нет подключённых серверов.",
        VendorNotAvail => {
            "Сведения о поставщике/версии/статусе сервера (GetStatus) не передаются текущим BFF-шлюзом; доступны только точка подключения и текст состояния."
        }
        LimitationsLabel => "Ограничения текущей версии:",
        LimitScriptLine => {
            "• интерпретатор скриптов не реализован (редактор и файлы .stx сохранены);"
        }
        LimitQualityLine => "• запись качества в OPC DA стандартом не предусмотрена;",
        LimitDcomLine => "• DCOM/OPC UA security настраивается средствами ОС/сервера.",

        // ---- Message/confirm ----
        ConfirmTitle => "Подтверждение",
        Yes => "Да",
        No => "Нет",

        // ---- App dialog content ----
        WriteError => "Ошибка записи",
        WriteFailedName => "Не удалось записать значение в сигнал \"{0}\".",
        WriteUnknownName => "Результат записи в сигнал \"{0}\" неизвестен.",
        WriteNotSubmitted => "Запись не отправлена",
        WriteNotSubmittedName => "Запись в сигнал \"{0}\" не отправлена.",
        AddSignalTitle => "Добавление сигнала",
        AddSignalBadNode => {
            "Выбран узел-ветка. Выберите сигнал или используйте \"Добавить дочерние\"."
        }
        ChangeValueTitle => "Изменение значения",
        SameServerMsg => "Выберите сигналы только одного сервера.",
        ConfirmWriteTitle => "Подтверждение записи",
        ConfirmWriteOne => {
            "Изменить значение сигнала \"{0}\" на \"{1}\"?\n\nДанная операция может привести к подаче сигнала управления."
        }
        ConfirmWriteMany => {
            "Изменить значения сигналов ({0} шт.) на \"{1}\"?\n\nДанная операция может привести к подаче сигнала управления."
        }
        DisconnectConfirmTitle => "Отключение",
        DisconnectConfirmText => "Отключиться от сервера \"{0}\" ?",

        // ---- Script dialog ----
        ScriptTitle => "Скрипт",
        ScriptNotImpl => "Интерпретатор скриптов не реализован в этой версии.",

        // ---- CSV dialog ----
        LoadTreeDialog => "Загрузка дерева сигналов",
        ImportStopped => "Импорт остановлен пользователем.",
        ImportDone => "Готово. Загружено сигналов: {0}",
        ImportProgress => "Обработано строк: {0} из {1}",

        // ---- App/journal ----
        AppStarted => "OPCLook запущен",
        Ready => "Готово",
        ConnectingTo => "Подключение к {0} ({1})...",
        DisconnectingFrom => "Отключение от {0}",
        Reconnecting => "переподключение...",
        ConnectedInfo => "{0}: {1}",
        AttemptsExhausted20 => "Попытки исчерпаны (20): автовосстановление остановлено",
        AutoReconnectAtt => "Автовосстановление (попытка {0})...",
        ReconnectRestoring => "Восстановление соединения...",
        AutoReconnectStoppedByUser => "Автовосстановление связи остановлено пользователем",
        AttemptsExhausted => "Попытки исчерпаны (20)",
        ReconnectBackoff => "Повторное подключение {0}/{1} через {2} с",
        ReconnectStopped => "Повторное подключение {0}/{1} — автовосстановление остановлено",
        AttemptsExhaustedFor => {
            "Попытки исчерпаны ({0}): автовосстановление остановлено для \"{1}\""
        }
        ConnLostReconnect => "Соединение потеряно. Автовосстановление: попытка {0}/{1} через {2} с",
        ReadTimeout => "Таймаут операции чтения; связь будет проверена",
        ServersFoundUa => "Найдено OPC UA серверов: {0}",
        ServersFoundDa => "Найдено OPC DA серверов: {0}",
        ReqServerList => "Запрос списка OPC DA серверов{0}...",
        SearchingUaHost => "Поиск OPC UA серверов на {0}:{1}...",
        AddFailedName => "Не удалось добавить сигнал \"{0}\": {1}",
        NoChildSignals => "В выбранной ветке нет дочерних сигналов",
        WrittenValue => "Записано значение в \"{0}\"",
        WriteAcceptedStatus => "Запись принята сервером; физическое действие не подтверждено",
        WriteAsyncStatus => "Запись принята для асинхронного завершения; источник не проверен",
        WriteUncertainStatus => "Результат записи неопределён",
        WriteRejectedStatus => "Запись отклонена сервером",
        WriteUnknownStatus => "Результат записи неизвестен; запись могла выполниться",
        WriteOutcomeForItem => "\"{0}\": {1}",
        WriteFailed => "Не удалось записать \"{0}\": {1}",
        WriteDoneSummary => "Сервер принял записи: {0} из {1}",
        StaleData => "устаревшие данные",
        NoData => "нет данных",
        WaitingValue => "ожидание значения",
        SignalAdded => "Сигнал добавлен в таблицу",
        AddedSignals => "Добавлено сигналов: {0}",
        AddFailedEmpty => "Сигналы не добавлены: пустой список или дубликаты",
        RemovedSignals => "Удалено сигналов: {0}",
        OptionsSaved => "Параметры сохранены",
        ScriptSaved => "Скрипт сохранён",
        ErrorOp => "{0}: {1}",
        OpConnect => "Подключение",
        OpDisconnect => "Отключение",
        OpBrowse => "Обзор",
        OpAdd => "Добавление сигналов",
        OpRemove => "Удаление сигналов",
        OpRead => "Чтение",
        OpWrite => "Запись",
        OpTest => "Проверка связи",
        OpDiscover => "Поиск серверов",
        OpSearchServers => "поиск серверов",
        OpBrowseViews => "Просмотр представлений",
        OpResolvePath => "Разрешение пути",
        OpSearchNetwork => "поиск через LDS/mDNS",
        OpSubscription => "Подписка",
        OpUaRuntime => "Среда OPC UA",
        NotWritable => "сигнал \"{0}\" не допускает запись (only-read)",
        WriteRequested => "Запрос записи для {0} сигнал(ов), тип {1}",
        ScriptNotImplNote => {
            "Интерпретатор скриптов пока не реализован в этой версии; редактор сохраняет текст."
        }
        ScriptSavedPath => "Скрипт сохранён: {0}",
        ScriptSaveErr => "Ошибка сохранения {0}: {1}",
        ScriptLoadedPath => "Скрипт загружен: {0}",
        ScriptLoadErr => "Ошибка загрузки {0}: {1}",
        JournalSaved => "Журнал сохранён: {0}",
        JournalSaveErr => "Ошибка сохранения журнала: {0}",
        JournalLoaded => "Журнал загружен: {0} строк",
        JournalLoadErr => "Ошибка загрузки журнала: {0}",
        TableSaved => "Таблица сигналов сохранена: {0}",
        CsvSaveErr => "Ошибка сохранения CSV: {0}",
        CsvReadErr => "Не удалось прочитать {0}: {1}",
        CsvNoSignals => "CSV не содержит строк сигналов",
        CsvServerNotConnected => "CSV: сервер \"{0}\" не подключён",
        NotInteger => "\"{0}\" не является целым числом",
        NotUnsigned => "\"{0}\" не является целым числом без знака",
        NotNumber => "\"{0}\" не является числом",
        NotBool => "\"{0}\" не является логическим значением",
        WriteInputTooLarge => {
            "Значение превышает 1 МиБ или расчётный размер пакета превышает 16 МиБ"
        }
        NotDatetime => "не удалось разобрать дату/время \"{0}\"",
        ValueNotServer => "не указан сервер",
        PortInvalid => "укажите корректный порт",
        HostInvalid => "укажите адрес сервера",
        NoServerSelected => "не выбран сервер",
        NameRequired => "укажите имя сигнала",

        // ---- Drag & drop ----
        DropToAdd => "Отпустите, чтобы добавить сигналы",
        DropCrossSession => "Перетаскивать можно только узлы активного сервера",
        DropEmptyBranch => "В ветке нет сигналов для добавления",
        AddChildrenProgress => "Добавление дочерних: {0}…",
        AddChildrenCapHit => "Достигнут предел узлов ({0}); добавление дочерних остановлено",
        FolChildrenNone => "Рекурсивное добавление не нашло сигналов в ветке",

        // ---- Session tabs ----
        AddTabTooltip => "Открыть диалог подключения",
        CloseTabTooltip => "Скрыть вкладку (контейнер сохраняется)",
        DoubleClickServerHint => {
            "Двойной щелчок — открыть вкладку (подключиться, если ещё не подключено)"
        }
        RemoveFromList => "Удалить из списка",
        RemoveFromListConfirmTitle => "Удаление сессии",
        RemoveFromListConfirmText => "Удалить сессию \"{0}\" и все её сигналы из списка?",
        EmptyTabsHint => "Двойной щелчок по серверу в диалоге подключения открывает вкладку",
        RefreshF5Button => "Обновить ({0})",
        AddNodeTooltip => "Добавить выбранный сигнал в таблицу",
        AddByNameTooltip => "Добавить сигнал по имени (Item ID / NodeId)",
        TreeExpandLabel => "Развернуть",
        TreeCollapseLabel => "Свернуть",

        // ---- Servers area (left panel) ----
        AddServer => "Добавить сервер",
        RemoveServerTooltip => "Удалить сервер и вкладку",
        ReconnectTooltip => "Переподключиться",
        ReconnectCount => "{0}/{1}",
        NewConnection => "Новое подключение",

        // ---- Network settings tab ----
        NetworkTab => "Сетевые настройки",
        NetworkAuto => "Автоматически",
        NetworkRefresh => "Обновить",
        NetworkApplyHint => "Изменения применяются сразу и сохраняются.",
        NetworkAdaptersTitle => "Сетевые адаптеры",
        NetworkPreferredLine => "Предпочтительный: {0}",
        NetworkUsedLine => "Используется: {0}",
        NetworkFallbackLine => "Причина fallback: {0}",
        NetworkAddressesLine => "Адреса: {0}",
        NetworkGatewayLine => "Шлюз: {0}",
        NetworkMetricLine => "Метрика: {0}",
        NetworkStateLabel => "Состояние: {0}",
        NetworkTypePhysical => "физический",
        NetworkTypeVirtual => "виртуальный",
        NetworkTypeLoopback => "loopback",
        NetworkNoAdapters => "Адаптеры не найдены",
        NetworkErrorLine => "Ошибка сети: {0}",
        NetworkFallbackPreferredMissing => "сохранённый адаптер не найден",
        NetworkFallbackPreferredDown => "сохранённый адаптер недоступен",
        NetworkFallbackPreferredNoAddress => "у сохранённого адаптера нет адреса",
        NetworkFallbackNoPhysical => "физический адаптер не найден",
        NetworkFallbackAutoPhysical => "автоматически выбран физический адаптер",
        NetworkFallbackAutoLoopback => "используется loopback",
        NetworkLoopbackName => "Loopback",
        NetworkAddressChoice => "Локальный IP",
        NetworkStateUp => "Работает",
        NetworkStateDown => "Отключён",
        NetworkStateTesting => "Проверка",
        NetworkStateDormant => "Ожидание",
        NetworkStateNotPresent => "Отсутствует",
        NetworkStateLowerLayerDown => "Нижний уровень отключён",
        NetworkStateUnknown => "Неизвестно",

        // ---- UA subnet scan (N+1) ----
        SearchUaSubnet => "Поиск в сети",
        SearchingUaSubnet => "Поиск OPC UA серверов в подсети...",
        UaScanProgress => "Проверено {0} из {1}, найдено {2}",
        UaScanStop => "Остановить",
        UaScanConfirmText => {
            "Подсеть содержит {0} адресов — больше предела {1}. Сканировать только этот компьютер (loopback и локальный адрес)?"
        }
        UaScanTooLarge => "Подсеть слишком велика для сканирования ({0} адресов). Сузьте маску.",
        UaScanFailed => "Ошибка сканирования: {0}",
        UaScanNoResults => "OPC UA серверы в подсети не найдены",
        UaScanNoLease => "Нет доступного сетевого адаптера для поиска",
        NetworkAdapterCombo => "Сетевой адаптер",
        NetworkNotSelected => "Не выбран",
        NetworkSelectAdapterHint => "Выберите сетевой адаптер для поиска и подключения по сети.",
        NetworkAdapterDescription => "Описание",
        NetworkDaRouteLine => "Маршрут DA: {0}",
        NetworkStaleAdapter => "Сохранённый адаптер недоступен. Выберите активный.",
        NetworkEmptyList => "Список не загружен. Нажмите «Обновить».",
        NetworkRefreshFailed => "Не удалось обновить список",
        NetworkNoIp => "Нет IP",
        NetworkForget => "Забыть",
        UaTreeSummary => "адресов: {0} · вариантов безопасности: {1}",
        UaTreeSecurityOnConnect => {
            "Сервер не сообщил варианты безопасности для этого адреса — они будут запрошены при подключении (Авто)"
        }
        UaTreeUnsupported => "не поддерживается клиентом",
        UaTreeNoEndpoints => "Сервер не сообщил адресов подключения",
        SecPolicyAes128Sha256RsaOaep => "Aes128_Sha256_RsaOaep",
        SecPolicyAes256Sha256RsaPss => "Aes256_Sha256_RsaPss",
        UaSessionCounter => "UA-сессии: {0}/{1}",
        UaSessionLimitReached => "Достигнут предел UA-сессий ({0}). Удалите или закройте сессию.",
        SelectAdapterRequired => "Выберите сетевой адаптер",
        DaRouteError => "Не удалось подготовить маршрут DA: {0}",
        DaRouteNoAdapter => "сетевой адаптер не выбран",
        DaRouteHelperStarting => "помощник маршрутов запускается; повторите подключение",
        DaRouteHelperUnavailable => {
            "помощник маршрутов недоступен (UAC отклонён или helper не найден)"
        }
        DaRouteConflict => "маршрут к {0} уже идёт через другой адаптер",
        DaRouteNoGateway => "у выбранного адаптера нет шлюза для {0}",
        DaRouteResolve => "не удалось разрешить хост {0}",
        DaRouteAdd => "не удалось добавить маршрут к {0}: {1}",
        DaRouteVerify => "маршрут к {0} не использует выбранный адаптер",
        DaRouteHelperRunning => "helper запущен, маршруты активны",
        DaRouteHelperIdle => "маршруты не созданы",
    }
}

fn en(key: Tr) -> &'static str {
    use Tr::*;
    match key {
        // ---- Top menu ----
        MenuClient => "OPC client",
        TraceDropped => "← #{0} (session {1}): no outstanding request, reply dropped",
        TraceReply => "← #{0} {1} (session {2}): {3} ms",
        TraceSent => "→ #{0} {1} (session {2})",
        DiagnosticTraceHint => {
            "Request id, operation, session and reply time in ms, for diagnosing hangs. The journal grows quickly."
        }
        DiagnosticTraceCheck => "Diagnostics: log requests and replies",
        NoReplyFromServer => "No reply from the server within {0} s",
        WorkerPanicked => "The {0} worker panicked",
        WorkerTimedOut => "The {0} worker did not shut down in time",
        GatewayClosed => "Gateway is shut down",
        WorkerClosed => "Worker command channel is closed",
        RequestIdsExhausted => "Request identifiers exhausted",
        ActionPayloadInvalid => {
            "Action parameters exceed the size, depth or item limits. Reduce the amount of data"
        }
        ReadBusy => "A read is already in progress for this session",
        AcquisitionTitle => "Data acquisition",
        AcquisitionEdit => "Edit connection profile",
        AcquisitionApplyNext => "Save for next manual connection",
        AcquisitionApplyReconnect => "Save and reconnect",
        AcquisitionPending => "Profile saved for the next manual connection",
        AcquisitionEpochChanged => "Connection changed or closed; reopen the editor",
        AcquisitionCollision => "A connection with this profile already exists",
        AcquisitionActive => "Active profile",
        AcquisitionDraft => "Draft",
        AcquisitionExplicit => "Use this connection profile",
        AcquisitionLegacy => "Legacy global settings and polling fallback",
        AcquisitionCacheGap => "UA cache mode is not yet implemented in OPCLook",
        AcquisitionMode => "Update mode",
        AcquisitionSubscription => "Subscription",
        AcquisitionPoll => "Poll",
        AcquisitionInterval => "Interval, ms",
        AcquisitionRequestedInterval => "Requested interval, ms",
        AcquisitionReadback => "Read after write",
        AcquisitionFreshness => "Read freshness",
        AcquisitionCache => "Cache allowed",
        AcquisitionSource => "Request source",
        AcquisitionUnsupported => "Unavailable in the current adapter",
        AcquisitionInvalid => "Invalid or unsupported acquisition settings",
        ReadEmpty => "There are no items to read",
        ItemKeyReused => "Item identifier was already used in this connection",
        ItemKeyLimit => "The item identifier limit for this connection was reached",
        ScanIdsExhausted => "Scan identifiers exhausted",
        SessionIdsExhausted => "Session identifiers exhausted",
        ItemIdsExhausted => "Item identifiers exhausted",
        InvalidItemHandle => "Received an invalid item identifier",
        CsvServerAmbiguous => {
            "CSV row skipped: server name \"{0}\" matches several connections, give the exact name"
        }
        CsvIssueLine => "CSV line {0}: {1}",
        CsvIssuesOmitted => "Additional CSV issues omitted: {0}",
        CsvMalformedQuote => "malformed quoting",
        CsvMissingFields => "missing fields",
        CsvEmptyItem => "empty item ID",
        CsvServerEmpty => "CSV row skipped: no server given for signal \"{0}\"",
        CjkFontMissing => {
            "No font for Chinese text was found (msyh.ttc, simsun.ttc): the 中文 interface will show boxes"
        }
        CjkFontLoaded => "Font for CJK text: {0}",
        ResolvingName => "Resolving node…",
        AddDuplicateResult => "The server returned duplicate registration results for an item",
        TimeSource => "Source time",
        TimeServer => "Server time",
        TimeDa => "DA time",
        TimeReceived => "Received locally",
        TimeOrigin => "Origin",
        TimeOriginRead => "Read",
        TimeOriginSubscription => "Subscription",
        TimeOriginUnknown => "Unknown",
        TimeOrderUnknown => "Source time ordering is not established",
        TimeOrderComparable => "Comparable source time",
        TimeOrderOlderHeld => "Older sample temporarily held",
        TimeOrderEqualConflict => "Equal time, different values: ordering is ambiguous",
        TimeOrderDiscontinuity => "Possible clock discontinuity: ordering is ambiguous",
        TimeOrderDiagnostic => "Diagnostic result",
        ReadNeedInitial => "initial",
        ReadNeedPoll => "poll",
        ReadNeedRefresh => "refresh",
        ReadNeedReadback => "read after write",
        ReadNeedDirect => "direct read",
        ReadTerminalLine => "Read: session {0}, epoch {1}, item {2}, request {3}, need {4}: {5}",
        ReadTerminalReceived => "Result received: quality {0}; reason: {1}",
        ReadTerminalInitial => "Initial notification: quality {0}; reason: {1}",
        ReadTerminalMissing => "The server returned no result for this item",
        ReadTerminalServiceError => "Read failed: {0}",
        ReadTerminalTimeout => "Read timed out, seconds: {0}",
        ReadTerminalCancelled => "Read cancelled: {0}",
        ReadCancelRemoved => "item removed",
        ReadCancelDisconnected => "connection closed",
        ReadCancelEpoch => "connection changed",
        ReadCancelExplicit => "operation cancelled",
        ReadTerminalDispatchFailed => "Could not send read: {0}",
        ReadTerminalUnavailable => "Read unavailable: identifiers exhausted",
        ReadTerminalUnexpected => "Invalid read response",
        ReadTerminalSummaryLoss => "Read results coalesced: {0}; buffer overflow: {1}",
        SettingsReadFailed => "Could not read settings file {0}: {1}. It will not be overwritten",
        SettingsParseFailed => {
            "Settings file {0} is damaged ({1}). A copy was saved to {2}. Defaults are used"
        }
        SettingsParseNoBackup => {
            "Settings file {0} is damaged ({1}) and could not be backed up ({2}). It will not be overwritten; defaults are used"
        }
        SettingsSaveFailed => "Could not save settings {0}: {1}",
        SettingsAcquisitionMigrated => {
            "Migrated profiles: UA {0}, DA {1}; interval {2} ms. UA: subscription, one-shot reads from source, no read after write; legacy poll_only fallback is disabled. DA: Sync poll, cache allowed, no read after write."
        }
        SettingsAcquisitionMigrationFailed => {
            "Could not migrate profiles: legacy interval {0} s is out of range or overflows. The original file is protected; correct the interval and restart the application."
        }
        SettingsAcquisitionMigrationBlocked => {
            "Legacy profile unavailable: correct the interval in the settings file and restart the application."
        }
        SettingsSaveBlocked => {
            "Could not save settings {0}: the existing file is damaged or unreadable and could not be backed up. It will not be overwritten"
        }
        NetworkCacheWarning => "Network adapter cache: {0}",
        NetworkCacheLoaded => "Network adapter cache loaded: {0}",
        NetworkCacheSkipped => "Network cache entry skipped: {0}",
        NetworkRefreshStart => "Network adapter refresh started",
        NetworkRefreshError => "Network adapter refresh failed: {0}",
        NetworkRefreshInventory => "Network adapters found: {0}",
        NetworkRefreshDone => "Network refresh complete, usable adapters: {0}",
        NetworkAdapterSkipped => "Network adapter skipped: {0}",
        NetworkAdapterDetails => "Network adapter: {0}",
        NetworkSelectionDetails => "Network adapter selected: {0}",
        NetworkSelectionStale => "Selected adapter is unavailable: {0}",
        NetworkSelectionNone => "No network adapter selected",
        NetworkSelectionCleared => "Network adapter selection cleared",
        ConnectInProgress => "Connecting…",
        MenuServers => "Servers",
        MenuSignals => "Signals",
        MenuScript => "Script",
        MenuJournal => "Journal",
        MenuHelp => "Help",
        MenuLanguage => "Language",
        LangRu => "Russian",
        LangEn => "English",
        LangZh => "Chinese (简体中文)",
        ConnectEllipsis => "Connect...",
        ActionWriteConnectionChanged => {
            "Write cancelled because the connection changed. Check the value and confirm again."
        }
        DiagnosticRedacted => "[redacted]",
        PaletteTitle => "Command palette",
        PaletteExecute => "Run",
        PaletteSearch => "Search commands",
        PaletteNoResults => "No commands found",
        PaletteNeedsContext => "Parameters and context are required",
        PaletteBrowseNode => "Branch NodeId (empty for root)",
        PaletteBrowseUnknown => "Invalid or overly long identifier",
        PaletteBrowseMismatch => "The server replied for a different branch",
        PaletteBrowseLimit => "Tree identifiers exhausted",
        PaletteTarget => "Connection: {0}",
        PaletteSelectOne => "Select one signal from the current connection",
        PaletteConnectionChanged => "Connection changed; reopen the palette",
        PaletteWriteStale => "Connection changed; reopen the write dialog",
        PaletteBrowse => "Browse branch",
        PaletteWrite => "Write value",
        ActionModalOpen => "Close the open dialog first",
        ActionSessionMissing => "Session not found",
        ActionSessionNotConnected => "Session is not connected",
        ActionProtocolMismatch => "Protocol does not match the session",
        ActionImportInProgress => "Wait for the import to finish",
        Disconnect => "Disconnect",
        SettingsEllipsis => "Settings...",
        Exit => "Exit",
        Reconnect => "Reconnect",
        RefreshBranch => "Refresh branch",
        LoadTreeCsv => "Load signal tree (CSV)...",
        SaveTreeCsv => "Save signal tree (CSV)...",
        AddSelected => "Add selected",
        AddChildrenSignals => "Add child signals",
        AddByName => "Add by name...",
        RemoveSelected => "Remove selected",
        ChangeValue => "Change value...",
        ChangeSelectedValues => "Change selected values...",
        SelectAll => "Select all",
        ClearSelection => "Clear selection",
        ColumnsEllipsis => "Columns...",
        Save => "Save",
        Load => "Load...",
        CheckSyntax => "Check syntax",
        ShowScript => "Show script",
        ShowJournal => "Show journal",
        ShowWorkJournal => "Show work journal",
        Clear => "Clear",
        SaveJournal => "Save...",
        LoadJournal => "Load...",
        AboutProgram => "About...",

        // ---- Tooltips ----
        HintConnectUa => "Connect to an OPC DA/UA server",
        HintRefreshF5 => "{0} — refresh branch",
        HintLoadCsv => "Load signals from a CSV file",
        HintSaveCsv => "Save signals to a CSV file",
        HintEnterTree => "Enter in the tree",
        HintAddChildren => "Add all signals of the branch",
        HintDelSelected => "{0} — remove selected",
        HintEnterF2 => "{0} — change value",
        HintCtrlA => "{0} — select all",
        HintEditColumns => "Configure columns and their order",

        // ---- Status bar ----
        Connected => "Connected",
        Connecting => "Connecting",
        NoLink => "No connection",
        NoActiveServer => "no active server",
        SignalsCount => "Signals: {0} | Selected: {1}",
        Writing => "Writing...",
        WrittenSummary => "Accepted by server: {0} of {1}",
        UpdatedAt => "Updated {0}",
        RowsHint => {
            "Table: ↑↓ Shift+PgUp/PgDn Home/End {0} {1} {3} {4} {5} · {6} — tree · {2} — server"
        }
        TreeHint => "Tree: ↑↓ ← → Space {1} {2} · {3} — table · {0} — server",
        KeyboardIdleHint => "{1} — table / tree · {0} — connect",

        // ---- Bottom panels ----
        ScriptTab => "Script",
        JournalTab => "Journal",
        HintScriptTab => "Script — editor and execution",
        HintJournalTab => "Journal — operation log",
        SaveScriptLabel => "Save script",
        LoadScriptLabel => "Load script",
        ClearJournalLabel => "Clear journal",
        SaveJournalLabel => "Save journal...",
        ScriptsNotImpl => "Scripts: interpreter not implemented",
        ThemeTitle => "Colour theme",
        ThemeSystem => "Follow system",
        ThemeLight => "Light",
        ThemeDark => "Dark",
        ThemeHint => "Applied after Save",
        HintScriptTabNotImpl => {
            "Script — not implemented: there is no interpreter yet, the text can only be edited and saved"
        }
        EntriesCount => "entries: {0}",

        // ---- Navigation ----
        Servers => "Servers",
        Connect => "Connect",
        ConnectPrompt => "Connect to an OPC DA or OPC UA server.",
        Add => "Add",
        AddChildren => "Children",
        AddByNameShort => "By name...",
        BrowseProto => "Browse: {0}",
        UpdatingTree => "Updating tree...",
        LoadingTree => "Loading tree...",
        TreeNotLoaded => "Tree not loaded.",
        NoConnection => "No connection.",
        ResumeAttempts => "Resume attempts",
        StopAttempts => "Stop attempts",
        Refresh => "Refresh",
        CopyNodeName => "Copy node name",
        NodeNameCopied => "Node name copied",
        ExpandCollapse => "Expand / collapse",
        ViewsRoot => "Views",
        OnlyVars => "Variables only",
        NameNotResolved => "Name \"{0}\" did not resolve as an OPC UA BrowsePath",

        // ---- Table ----
        TableEmpty => "The signal table is empty.",
        TableEmptyHelp => "Connect to a server, expand a branch and add signals.",
        TableHint => "Hint: double-click a signal in the tree or right-click → Add.",
        TableEmptyInner => "No signals — add from the tree, via right-click or drag them here",
        TableValueExpand => "Expand value",
        TableValueCollapse => "Collapse value",
        TableEmptyKeys => "{0} — connect to a server · {1} — tree and table",
        TableEmptyDragHint => "Drag tags from the tree or press Enter on a node",
        TableEmptyConnect => "Connect ({0})",
        EscHint => "Press Esc to clear the selection. Right-click a row for the menu.",
        PkmMoveColumn => "Right-click to move the column",
        MoveLeft => "Left",
        MoveRight => "Right",
        AddThisNode => "Add (from this node)",
        AddChildrenCtx => "Add children",
        CopyFullNameCtx => "Copy full name",
        CopyValueCtx => "Copy value",
        SelectAllCtx => "Select all",
        ClearSelectionCtx => "Clear selection",
        SaveCsvCtx => "Save CSV",
        CtxColumns => "Columns...",
        CtxAddByName => "Add by name...",
        CtxRefreshBranch => "Refresh branch",
        CtxSaveCsv => "Save CSV...",
        CtxSelectAll => "Select all",
        CtxClearSelection => "Clear selection",
        ColumnsDragHandle => "Drag to reorder",
        ColumnsReorderHint => "Alt+Up/Down or Ctrl+Up/Down to reorder",
        ValueCopied => "Value copied",
        FullNameCopied => "Full name copied",
        Copied => "Copied",
        ColumnMovedLeft => "Column \"{0}\" moved left",
        ColumnMovedRight => "Column \"{0}\" moved right",
        ColStatus => "Status",
        ColFullName => "Full name",
        ColWriteType => "Write type",
        ColType => "Type",
        ColValue => "Value",
        ColQuality => "Quality",
        ColTime => "Change time",
        ColServer => "OPC server",
        ColDescription => "Description",
        HintStatus => "Row status",
        HintFullName => "Full signal name on the server",
        HintWriteType => "User-assigned write type",
        HintType => "Signal type on the server",
        HintValue => "Current signal value on the server",
        HintQuality => "Quality characteristic of the signal value",
        HintTime => "Time of the last signal change",
        HintServer => "Programmatic server identifier",
        HintDescription => "Signal description",

        // ---- Status popup ----
        Server => "Server",
        ProtocolLabel => "Protocol:",
        EndpointLabel => "Endpoint:",
        StateLabel => "State:",
        DetailsLabel => "Details:",
        SubscribedLabel => "Subscribed signals:",
        SubscriptionLabel => "Subscription:",
        SubscriptionActive => "active",
        SubscriptionInactive => "inactive",
        SubscriptionFailed => "failed",
        SelectedLabel => "Selected:",
        LastUpdateLabel => "Last update:",
        ReconnectLabel => "Reconnect:",
        RetriesLabel => "Retry count:",
        Close => "Close",
        AutoReconnectStopped => "auto-reconnect stopped (attempts: {0})",
        ReconnectingLine => "reconnecting: attempt {0}, retry in {1} s (limit {2})",
        ReconnectingAttempt => "reconnecting: attempt {0}",
        RetriesLine => "{0} (limit {1})",
        ServerHostSuffix => " on {0}",
        SearchingUaNetwork => "Searching for OPC UA servers on the network (mDNS/LDS)...",

        // ---- Connect dialog ----
        ConnectToServer => "Connect to OPC server",
        DaTab => "OPC DA (COM)",
        UaTab => "OPC UA",
        DaServersList => "OPC servers list:",
        FoundServers => "Found servers:",
        PollingServers => "Polling OPC servers...",
        ListEmptyManual => "The list is empty. Specify a ProgID manually.",
        SearchingUa => "Searching for OPC UA servers...",
        ListEmptyUrl => "The list is empty. Specify a URL manually.",
        NetworkAddr => "Network address:",
        AddressLabel => "Address:",
        HintLocalAddr => "empty = local (optional)",
        PortLabel => "Port:",
        SearchLan => "Search local network",
        Search => "Search",
        EmptySearchHint => "Empty — search this computer and the local network",
        UaHostHint => "address or empty",
        UaPortHint => "port (defaults to 4840)",
        LdsHostLabel => "LDS host",
        LdsHostHint => "optional, host[:port]",
        LdsHostTip => {
            "Discovery server (LDS) that \"Search via LDS/mDNS\" asks for the server list. The port defaults to 4840. Empty: this computer and mDNS only."
        }
        LdsHostInvalid => {
            "LDS host: expected a name or address, optionally with a port (host:port)"
        }
        SearchingUaLds => "Searching for OPC UA servers at LDS {0} (and mDNS)...",
        UserLabel => "User:",
        PasswordLabel => "Password:",
        UrlLabel => "URL:",
        HintProgId => "Server ProgID, e.g. Alpha.Server",
        HintUaUrl => "opc.tcp://host:4840",
        Recent => "Recent",
        Cancel => "Cancel",
        SecPolicyLabel => "Security policy:",
        SecModeLabel => "Security mode:",
        AutoLabel => "Auto",
        SecPolicyNone => "None",
        SecPolicyBasic128Rsa15 => "Basic128Rsa15",
        SecPolicyBasic256 => "Basic256",
        SecPolicyBasic256Sha256 => "Basic256Sha256",
        SecModeNone => "None",
        SecModeSign => "Sign",
        SecModeSignAndEncrypt => "Sign and Encrypt",
        SecurityInvalidCombination => "Invalid security policy/mode combination: {0}",
        SecurityResolveFailed => "Could not select a security endpoint: {0}",
        TrustServerCertCheck => "Trust the server certificate (this session only)",
        TrustServerCertTip => {
            "The server certificate is accepted only for the duration of this connection and is not kept permanently. The certificate's validity period, host name and ApplicationUri are not checked. To trust the server permanently, import its certificate into the PKI store."
        }
        IdentityLabel => "Identity:",
        IdentityAnonymous => "Anonymous",
        IdentityUserName => "User name",
        IdentityCertificate => "Certificate",
        SessionTimeoutLabel => "Session timeout (ms):",
        SearchLdsMdns => "Search via LDS/mDNS",
        NetworkServersNotFound => "No network servers found (mDNS/LDS not responding)",
        CertIdentityUnsupported => {
            "The selected security endpoint does not support certificate identity"
        }
        PkiPathLabel => "PKI store:",
        ClientCertSubjectLabel => "Client certificate:",
        NoClientCert => "not created (appears after the first connect)",
        SessionTimeoutInfo => {
            "Requested session timeout, ms (5000-3600000). The server may revise it."
        }
        UaUserNameRequired => "Enter a user name for the selected identity.",

        // ---- Add signal ----
        AddSignal => "Add signal",
        ServerLabel => "Server:",
        SignalName => "Signal name:",
        HintItemId => "OPC DA item id or OPC UA NodeId",
        DisplayName => "Display name:",
        Optional => "optional",
        NotSelected => "not selected",

        // ---- Write dialog ----
        ChangeSignalValue => "Change signal value",
        SignalLabel => "Signal: {0}",
        WriteTargetCount => "Number of selected signals: {0}",
        CurrentValue => "Current value: {0}",
        ServerTypeLabel => "Server type: {0}",
        ReadOnly => "Signal is read-only (writing is unavailable)",
        CanonicalDaType => "Canonical DA type: {0}",
        ServerDataType => "Server data type: {0}",
        WriteTypeLabel => "Write type:",
        ValueLabel => "Value:",
        HintWriteValue => "true / 123 / 12.5 / text / 2026-01-31 12:00:00",
        ConfirmAfterWrite => "Clicking \"Write\" will require confirmation.",
        Write => "Write",

        // ---- Columns dialog ----
        ColumnsSetup => "Column settings",
        ColumnsHint => "Select columns and set their order.",
        IndicatorNote => "(indicator)",
        MovedFormat => "Moved: {0}",
        Reset => "Reset",
        Apply => "Apply",

        // ---- Options dialog ----
        OptionsTitle => "Options",
        UpdateInterval => "Update interval",
        Seconds => "seconds",
        SecondsShortSuffix => " s",
        RecommendedOneSec => "Recommended value: 1 second.",
        AutoReconnectTitle => "Automatic reconnection",
        AutoReconnectCheck => "Automatically restore connection to the server",
        AutoReconnectWarn => "Warning! Setting this flag may start the server.",
        DisplayOptions => "Display options",
        ShowColorsCheck => "Display signals in different colors",
        ConfirmWriteCheck => "Ask for confirmation when changing a signal",
        AdvancedTitle => "Advanced",
        OpTimeoutLabel => "Operation timeout (1–1800 s). Recommended 60 s.",
        TestIntervalLabel => "Link test interval (1–1800 s). Recommended 60 s.",
        DeadbandLabel => "Deadband (absolute)",
        DeadbandHint => "OPC UA: a value change is pushed only when it exceeds the zone (0 = off).",

        // ---- About dialog ----
        AboutTitle => "About",
        TestOpcClient => "OPCLook",
        RustEgui2026 => "Rust + egui, 2026",
        OriginalProto => {
            "OPC DA 2.0: COM interfaces IOPCServer / IOPCItemMgt / IOPCSyncIO / IOPCBrowseServerAddressSpace."
        }
        SupportedLabel => "Supported:",
        AboutDaLine => "• OPC DA 2.0 via COM (local and over DCOM);",
        AboutUaLine => "• OPC UA via async-opcua 0.19 (SecurityPolicy None, anonymous / username);",
        AboutTreeLine => "• address space tree, signal table, read/write, journal;",
        AboutSettingsLine => "• settings saved to %APPDATA%\\opclook\\settings.json.",
        ConnectedServers => "Connected servers:",
        NoConnectedServers => "No connected servers.",
        VendorNotAvail => {
            "Vendor/version/status (GetStatus) is not exposed by the current BFF gateway; only the endpoint and the status text are available."
        }
        LimitationsLabel => "Limitations of this version:",
        LimitScriptLine => {
            "• the script interpreter is not implemented (editor and .stx files are kept);"
        }
        LimitQualityLine => "• quality writes are not part of the OPC DA standard;",
        LimitDcomLine => "• DCOM/OPC UA security is configured via the OS/server.",

        // ---- Message/confirm ----
        ConfirmTitle => "Confirmation",
        Yes => "Yes",
        No => "No",

        // ---- App dialog content ----
        WriteError => "Write error",
        WriteFailedName => "Failed to write a value to signal \"{0}\".",
        WriteUnknownName => "The write outcome for signal \"{0}\" is unknown.",
        WriteNotSubmitted => "Write not submitted",
        WriteNotSubmittedName => "The write to signal \"{0}\" was not submitted.",
        AddSignalTitle => "Add signal",
        AddSignalBadNode => "A branch node is selected. Select a signal or use \"Add children\".",
        ChangeValueTitle => "Change value",
        SameServerMsg => "Select signals from one server only.",
        ConfirmWriteTitle => "Confirm write",
        ConfirmWriteOne => {
            "Change the value of signal \"{0}\" to \"{1}\"?\n\nThis operation may issue a control signal."
        }
        ConfirmWriteMany => {
            "Change the values of {0} signals to \"{1}\"?\n\nThis operation may issue a control signal."
        }
        DisconnectConfirmTitle => "Disconnect",
        DisconnectConfirmText => "Disconnect from server \"{0}\" ?",

        // ---- Script dialog ----
        ScriptTitle => "Script",
        ScriptNotImpl => "The script interpreter is not implemented in this version.",

        // ---- CSV dialog ----
        LoadTreeDialog => "Load signal tree",
        ImportStopped => "Import stopped by the user.",
        ImportDone => "Done. Signals loaded: {0}",
        ImportProgress => "Rows processed: {0} of {1}",

        // ---- App/journal ----
        AppStarted => "OPCLook started",
        Ready => "Ready",
        ConnectingTo => "Connecting to {0} ({1})...",
        DisconnectingFrom => "Disconnecting from {0}",
        Reconnecting => "reconnecting...",
        ConnectedInfo => "{0}: {1}",
        AttemptsExhausted20 => "Attempts exhausted (20): auto-reconnect stopped",
        AutoReconnectAtt => "Auto-reconnect (attempt {0})...",
        ReconnectRestoring => "Restoring connection...",
        AutoReconnectStoppedByUser => "Auto-reconnect stopped by the user",
        AttemptsExhausted => "Attempts exhausted (20)",
        ReconnectBackoff => "Reconnecting {0}/{1} in {2} s",
        ReconnectStopped => "Reconnecting {0}/{1} — auto-reconnect stopped",
        AttemptsExhaustedFor => "Attempts exhausted ({0}): auto-reconnect stopped for \"{1}\"",
        ConnLostReconnect => "Connection lost. Auto-reconnect: attempt {0}/{1} in {2} s",
        ReadTimeout => "Read operation timeout; connectivity will be checked",
        ServersFoundUa => "OPC UA servers found: {0}",
        ServersFoundDa => "OPC DA servers found: {0}",
        ReqServerList => "Requesting OPC DA server list{0}...",
        SearchingUaHost => "Searching for OPC UA servers on {0}:{1}...",
        AddFailedName => "Failed to add signal \"{0}\": {1}",
        NoChildSignals => "The selected branch has no child signals",
        WrittenValue => "Value written to \"{0}\"",
        WriteAcceptedStatus => "Write accepted by the server; physical action is not confirmed",
        WriteAsyncStatus => "Write accepted for asynchronous completion; source not verified",
        WriteUncertainStatus => "Write result is uncertain",
        WriteRejectedStatus => "Write rejected by the server",
        WriteUnknownStatus => "Write outcome is unknown; the write may have occurred",
        WriteOutcomeForItem => "\"{0}\": {1}",
        WriteFailed => "Failed to write \"{0}\": {1}",
        WriteDoneSummary => "Server accepted writes: {0} of {1}",
        StaleData => "stale data",
        NoData => "no data",
        WaitingValue => "waiting for value",
        SignalAdded => "Signal added to the table",
        AddedSignals => "Signals added: {0}",
        AddFailedEmpty => "No signals added: empty list or duplicates",
        RemovedSignals => "Signals removed: {0}",
        OptionsSaved => "Options saved",
        ScriptSaved => "Script saved",
        ErrorOp => "{0}: {1}",
        OpConnect => "Connect",
        OpDisconnect => "Disconnect",
        OpBrowse => "Browse",
        OpAdd => "Adding signals",
        OpRemove => "Removing signals",
        OpRead => "Read",
        OpWrite => "Write",
        OpTest => "Link check",
        OpDiscover => "Server search",
        OpSearchServers => "server search",
        OpBrowseViews => "Browse views",
        OpResolvePath => "Path resolution",
        OpSearchNetwork => "LDS/mDNS search",
        OpSubscription => "Subscription",
        OpUaRuntime => "OPC UA runtime",
        NotWritable => "signal \"{0}\" does not allow writing (read-only)",
        WriteRequested => "Write request for {0} signal(s), type {1}",
        ScriptNotImplNote => {
            "The script interpreter is not implemented yet in this version; the editor saves text."
        }
        ScriptSavedPath => "Script saved: {0}",
        ScriptSaveErr => "Error saving {0}: {1}",
        ScriptLoadedPath => "Script loaded: {0}",
        ScriptLoadErr => "Error loading {0}: {1}",
        JournalSaved => "Journal saved: {0}",
        JournalSaveErr => "Error saving journal: {0}",
        JournalLoaded => "Journal loaded: {0} lines",
        JournalLoadErr => "Error loading journal: {0}",
        TableSaved => "Signal table saved: {0}",
        CsvSaveErr => "Error saving CSV: {0}",
        CsvReadErr => "Failed to read {0}: {1}",
        CsvNoSignals => "CSV contains no signal rows",
        CsvServerNotConnected => "CSV: server \"{0}\" is not connected",
        NotInteger => "\"{0}\" is not an integer",
        NotUnsigned => "\"{0}\" is not an unsigned integer",
        NotNumber => "\"{0}\" is not a number",
        NotBool => "\"{0}\" is not a boolean value",
        WriteInputTooLarge => "The value exceeds 1 MiB or the estimated write batch exceeds 16 MiB",
        NotDatetime => "failed to parse date/time \"{0}\"",
        ValueNotServer => "no server specified",
        PortInvalid => "specify a valid port",
        HostInvalid => "specify the server address",
        NoServerSelected => "no server selected",
        NameRequired => "specify the signal name",

        // ---- Drag & drop ----
        DropToAdd => "Release to add signals",
        DropCrossSession => "Only nodes of the active server can be dragged here",
        DropEmptyBranch => "The branch has no signals to add",
        AddChildrenProgress => "Adding children: {0}…",
        AddChildrenCapHit => "Node cap reached ({0}); adding children stopped",
        FolChildrenNone => "Recursive add found no signals in the branch",

        // ---- Session tabs ----
        AddTabTooltip => "Open the connect dialog",
        CloseTabTooltip => "Hide this tab (the container stays loaded)",
        DoubleClickServerHint => "Double-click to open the tab (connect if not yet connected)",
        RemoveFromList => "Remove from list",
        RemoveFromListConfirmTitle => "Remove session",
        RemoveFromListConfirmText => "Remove session \"{0}\" and all its signals from the list?",
        EmptyTabsHint => "Double-click a server in the connect dialog to open a tab",
        RefreshF5Button => "Refresh ({0})",
        AddNodeTooltip => "Add the selected signal to the table",
        AddByNameTooltip => "Add a signal by name (item id / NodeId)",
        TreeExpandLabel => "Expand",
        TreeCollapseLabel => "Collapse",

        // ---- Servers area (left panel) ----
        AddServer => "Add server",
        RemoveServerTooltip => "Remove server and tab",
        ReconnectTooltip => "Reconnect",
        ReconnectCount => "{0}/{1}",
        NewConnection => "New connection",

        // ---- Network settings tab ----
        NetworkTab => "Network settings",
        NetworkAuto => "Automatic",
        NetworkRefresh => "Refresh",
        NetworkApplyHint => "Changes apply immediately and are saved.",
        NetworkAdaptersTitle => "Network adapters",
        NetworkPreferredLine => "Preferred: {0}",
        NetworkUsedLine => "In use: {0}",
        NetworkFallbackLine => "Fallback reason: {0}",
        NetworkAddressesLine => "Addresses: {0}",
        NetworkGatewayLine => "Gateway: {0}",
        NetworkMetricLine => "Metric: {0}",
        NetworkStateLabel => "State: {0}",
        NetworkTypePhysical => "physical",
        NetworkTypeVirtual => "virtual",
        NetworkTypeLoopback => "loopback",
        NetworkNoAdapters => "No adapters found",
        NetworkErrorLine => "Network error: {0}",
        NetworkFallbackPreferredMissing => "saved adapter not found",
        NetworkFallbackPreferredDown => "saved adapter is unavailable",
        NetworkFallbackPreferredNoAddress => "saved adapter has no address",
        NetworkFallbackNoPhysical => "no physical adapter found",
        NetworkFallbackAutoPhysical => "physical adapter selected automatically",
        NetworkFallbackAutoLoopback => "using loopback",
        NetworkLoopbackName => "Loopback",
        NetworkAddressChoice => "Local IP",
        NetworkStateUp => "Up",
        NetworkStateDown => "Down",
        NetworkStateTesting => "Testing",
        NetworkStateDormant => "Dormant",
        NetworkStateNotPresent => "Not present",
        NetworkStateLowerLayerDown => "Lower layer down",
        NetworkStateUnknown => "Unknown",

        // ---- UA subnet scan (N+1) ----
        SearchUaSubnet => "Scan network",
        SearchingUaSubnet => "Scanning the subnet for OPC UA servers...",
        UaScanProgress => "Checked {0} of {1}, found {2}",
        UaScanStop => "Stop",
        UaScanConfirmText => {
            "The subnet has {0} addresses, above the {1} limit. Scan only this computer (loopback and the local address)?"
        }
        UaScanTooLarge => "The subnet is too large to scan ({0} addresses). Narrow the mask.",
        UaScanFailed => "Scan failed: {0}",
        UaScanNoResults => "No OPC UA servers found in the subnet",
        UaScanNoLease => "No network adapter is available for the scan",
        NetworkAdapterCombo => "Network adapter",
        NetworkNotSelected => "Not selected",
        NetworkSelectAdapterHint => {
            "Select a network adapter to search and connect over the network."
        }
        NetworkAdapterDescription => "Description",
        NetworkDaRouteLine => "DA route: {0}",
        NetworkStaleAdapter => "The saved adapter is unavailable. Select an active one.",
        NetworkEmptyList => "The list is not loaded. Press Refresh.",
        NetworkRefreshFailed => "Could not refresh the list",
        NetworkNoIp => "No IP",
        NetworkForget => "Forget",
        UaTreeSummary => "URLs: {0} · security options: {1}",
        UaTreeSecurityOnConnect => {
            "The server did not report security options for this URL — they are requested on connect (Auto)"
        }
        UaTreeUnsupported => "not supported by this client",
        UaTreeNoEndpoints => "The server reported no connection URL",
        SecPolicyAes128Sha256RsaOaep => "Aes128_Sha256_RsaOaep",
        SecPolicyAes256Sha256RsaPss => "Aes256_Sha256_RsaPss",
        UaSessionCounter => "UA sessions: {0}/{1}",
        UaSessionLimitReached => {
            "The UA session limit ({0}) is reached. Remove or close a session."
        }
        SelectAdapterRequired => "Select a network adapter",
        DaRouteError => "Could not prepare the DA route: {0}",
        DaRouteNoAdapter => "no network adapter is selected",
        DaRouteHelperStarting => "the route helper is starting; retry the connection",
        DaRouteHelperUnavailable => {
            "the route helper is unavailable (UAC declined or helper missing)"
        }
        DaRouteConflict => "a route to {0} already uses another adapter",
        DaRouteNoGateway => "the selected adapter has no gateway for {0}",
        DaRouteResolve => "could not resolve host {0}",
        DaRouteAdd => "could not add a route to {0}: {1}",
        DaRouteVerify => "the route to {0} does not use the selected adapter",
        DaRouteHelperRunning => "helper is running, routes are active",
        DaRouteHelperIdle => "no routes created",
    }
}

fn zh(key: Tr) -> &'static str {
    use Tr::*;
    match key {
        // ---- Top menu ----
        MenuClient => "OPC 客户端",
        TraceDropped => "← #{0}（会话 {1}）：无待处理请求，响应已丢弃",
        TraceReply => "← #{0} {1}（会话 {2}）：{3} 毫秒",
        TraceSent => "→ #{0} {1}（会话 {2}）",
        DiagnosticTraceHint => {
            "请求编号、操作、会话和响应耗时（毫秒），用于排查卡死。日志增长很快。"
        }
        DiagnosticTraceCheck => "诊断：将请求和响应写入日志",
        NoReplyFromServer => "服务器 {0} 秒内无响应",
        WorkerPanicked => "{0} 工作线程异常终止",
        WorkerTimedOut => "{0} 工作线程未能及时关闭",
        GatewayClosed => "网关已关闭",
        WorkerClosed => "工作线程命令通道已关闭",
        RequestIdsExhausted => "请求标识符已耗尽",
        ActionPayloadInvalid => "操作参数超出大小、深度或元素数量限制。请减少数据量",
        ReadBusy => "此会话已有读取正在进行",
        AcquisitionTitle => "数据采集",
        AcquisitionEdit => "编辑连接配置",
        AcquisitionApplyNext => "保存供下次手动连接使用",
        AcquisitionApplyReconnect => "保存并重新连接",
        AcquisitionPending => "配置已保存，将在下次手动连接时使用",
        AcquisitionEpochChanged => "连接已更改或关闭；请重新打开编辑器",
        AcquisitionCollision => "具有此配置的连接已存在",
        AcquisitionActive => "当前配置",
        AcquisitionDraft => "草稿",
        AcquisitionExplicit => "使用此连接配置",
        AcquisitionLegacy => "旧版全局设置和轮询回退",
        AcquisitionCacheGap => "OPCLook 尚未实现 UA 缓存模式",
        AcquisitionMode => "更新模式",
        AcquisitionSubscription => "订阅",
        AcquisitionPoll => "轮询",
        AcquisitionInterval => "间隔，毫秒",
        AcquisitionRequestedInterval => "请求的间隔，毫秒",
        AcquisitionReadback => "写入后读取",
        AcquisitionFreshness => "读取新鲜度",
        AcquisitionCache => "允许缓存",
        AcquisitionSource => "请求源",
        AcquisitionUnsupported => "当前适配器不支持",
        AcquisitionInvalid => "采集设置无效或不受支持",
        ReadEmpty => "没有要读取的项目",
        ItemKeyReused => "此连接中已使用该项目标识符",
        ItemKeyLimit => "已达到此连接的项目标识符数量上限",
        ScanIdsExhausted => "扫描标识符已耗尽",
        SessionIdsExhausted => "会话标识符已耗尽",
        ItemIdsExhausted => "信号标识符已耗尽",
        InvalidItemHandle => "收到无效的信号标识符",
        CsvServerAmbiguous => "已跳过 CSV 行：服务器名“{0}”匹配多个连接，请指定准确名称",
        CsvIssueLine => "CSV 第 {0} 行：{1}",
        CsvIssuesOmitted => "省略的其他 CSV 问题：{0}",
        CsvMalformedQuote => "引号格式错误",
        CsvMissingFields => "缺少字段",
        CsvEmptyItem => "信号标识为空",
        CsvServerEmpty => "已跳过 CSV 行：信号“{0}”未指定服务器",
        CjkFontMissing => "未找到可显示中文的字体（msyh.ttc、simsun.ttc）：界面文字可能显示为方块",
        CjkFontLoaded => "中文字体：{0}",
        ResolvingName => "正在解析节点…",
        AddDuplicateResult => "服务器为同一信号返回了多个注册结果",
        TimeSource => "源时间",
        TimeServer => "服务器时间",
        TimeDa => "DA 时间",
        TimeReceived => "本地接收时间",
        TimeOrigin => "来源",
        TimeOriginRead => "读取",
        TimeOriginSubscription => "订阅",
        TimeOriginUnknown => "未知",
        TimeOrderUnknown => "源时间顺序未确定",
        TimeOrderComparable => "源时间可比较",
        TimeOrderOlderHeld => "暂时保留较旧样本",
        TimeOrderEqualConflict => "时间相同但值不同：顺序不确定",
        TimeOrderDiscontinuity => "时钟可能跳变：顺序不确定",
        TimeOrderDiagnostic => "诊断结果",
        ReadNeedInitial => "初始值",
        ReadNeedPoll => "轮询",
        ReadNeedRefresh => "刷新",
        ReadNeedReadback => "写入后读取",
        ReadNeedDirect => "直接读取",
        ReadTerminalLine => "读取：会话 {0}，周期 {1}，信号 {2}，请求 {3}，用途 {4}：{5}",
        ReadTerminalReceived => "已收到结果：质量 {0}；原因：{1}",
        ReadTerminalInitial => "初始通知：质量 {0}；原因：{1}",
        ReadTerminalMissing => "服务器未返回该信号的结果",
        ReadTerminalServiceError => "读取失败：{0}",
        ReadTerminalTimeout => "读取超时，秒：{0}",
        ReadTerminalCancelled => "读取已取消：{0}",
        ReadCancelRemoved => "信号已删除",
        ReadCancelDisconnected => "连接已关闭",
        ReadCancelEpoch => "连接已更改",
        ReadCancelExplicit => "操作已取消",
        ReadTerminalDispatchFailed => "无法发送读取：{0}",
        ReadTerminalUnavailable => "无法读取：标识符已耗尽",
        ReadTerminalUnexpected => "读取响应无效",
        ReadTerminalSummaryLoss => "已合并读取结果：{0}；缓冲区溢出：{1}",
        SettingsReadFailed => "无法读取设置文件 {0}：{1}。不会覆盖该文件",
        SettingsParseFailed => "设置文件 {0} 已损坏（{1}）。副本已保存到 {2}。使用默认值",
        SettingsParseNoBackup => {
            "设置文件 {0} 已损坏（{1}），且无法备份（{2}）。不会覆盖该文件，使用默认值"
        }
        SettingsSaveFailed => "无法保存设置 {0}：{1}",
        SettingsAcquisitionMigrated => {
            "已迁移配置：UA {0}，DA {1}；间隔 {2} 毫秒。UA：订阅、单次读取源、不在写入后读取；旧的 poll_only 备用轮询已停用。DA：同步轮询、允许缓存、不在写入后读取。"
        }
        SettingsAcquisitionMigrationFailed => {
            "无法迁移配置：旧间隔 {0} 秒超出范围或溢出。原文件已保护；请修正间隔并重启应用。"
        }
        SettingsAcquisitionMigrationBlocked => "旧配置不可用：请修正设置文件中的间隔并重启应用。",
        SettingsSaveBlocked => {
            "无法保存设置 {0}：现有文件已损坏或无法读取，且无法备份。不会覆盖该文件"
        }
        NetworkCacheWarning => "网络适配器缓存：{0}",
        NetworkCacheLoaded => "已加载网络适配器缓存：{0}",
        NetworkCacheSkipped => "已跳过缓存记录：{0}",
        NetworkRefreshStart => "开始刷新网络适配器",
        NetworkRefreshError => "网络适配器刷新失败：{0}",
        NetworkRefreshInventory => "发现网络适配器：{0}",
        NetworkRefreshDone => "网络刷新完成，可用适配器：{0}",
        NetworkAdapterSkipped => "已跳过网络适配器：{0}",
        NetworkAdapterDetails => "网络适配器：{0}",
        NetworkSelectionDetails => "已选择网络适配器：{0}",
        NetworkSelectionStale => "所选适配器不可用：{0}",
        NetworkSelectionNone => "未选择网络适配器",
        NetworkSelectionCleared => "已清除网络适配器选择",
        ConnectInProgress => "连接中…",
        MenuServers => "服务器",
        MenuSignals => "信号",
        MenuScript => "脚本",
        MenuJournal => "日志",
        MenuHelp => "帮助",
        MenuLanguage => "语言",
        LangRu => "俄语",
        LangEn => "英语",
        LangZh => "简体中文",
        ConnectEllipsis => "连接...",
        ActionWriteConnectionChanged => "连接已更改，写入已取消。请检查值并重新确认。",
        DiagnosticRedacted => "[已隐藏]",
        PaletteTitle => "命令面板",
        PaletteExecute => "执行",
        PaletteSearch => "搜索命令",
        PaletteNoResults => "未找到命令",
        PaletteNeedsContext => "需要参数和上下文",
        PaletteBrowseNode => "分支 NodeId（留空表示根）",
        PaletteBrowseUnknown => "标识符无效或过长",
        PaletteBrowseMismatch => "服务器返回了其他分支的响应",
        PaletteBrowseLimit => "树节点标识符已耗尽",
        PaletteTarget => "连接：{0}",
        PaletteSelectOne => "请选择当前连接中的一个信号",
        PaletteConnectionChanged => "连接已更改，请重新打开命令面板",
        PaletteWriteStale => "连接已更改，请重新打开写入对话框",
        PaletteBrowse => "浏览分支",
        PaletteWrite => "写入值",
        ActionModalOpen => "请先关闭打开的对话框",
        ActionSessionMissing => "找不到会话",
        ActionSessionNotConnected => "会话未连接",
        ActionProtocolMismatch => "协议与会话不匹配",
        ActionImportInProgress => "请等待导入完成",
        Disconnect => "断开连接",
        SettingsEllipsis => "设置...",
        Exit => "退出",
        Reconnect => "重新连接",
        RefreshBranch => "刷新分支",
        LoadTreeCsv => "加载信号树 (CSV)...",
        SaveTreeCsv => "保存信号树 (CSV)...",
        AddSelected => "添加所选",
        AddChildrenSignals => "添加子信号",
        AddByName => "按名称添加...",
        RemoveSelected => "删除所选",
        ChangeValue => "更改值...",
        ChangeSelectedValues => "更改所选值...",
        SelectAll => "全选",
        ClearSelection => "清除选择",
        ColumnsEllipsis => "列...",
        Save => "保存",
        Load => "加载...",
        CheckSyntax => "检查语法",
        ShowScript => "显示脚本",
        ShowJournal => "显示日志",
        ShowWorkJournal => "显示工作日志",
        Clear => "清除",
        SaveJournal => "保存...",
        LoadJournal => "加载...",
        AboutProgram => "关于...",

        // ---- Tooltips ----
        HintConnectUa => "连接到 OPC DA/UA 服务器",
        HintRefreshF5 => "{0} — 刷新分支",
        HintLoadCsv => "从 CSV 文件加载信号",
        HintSaveCsv => "将信号保存到 CSV 文件",
        HintEnterTree => "在树中按 Enter",
        HintAddChildren => "添加该分支的所有信号",
        HintDelSelected => "{0} — 删除所选",
        HintEnterF2 => "{0} — 更改值",
        HintCtrlA => "{0} — 全选",
        HintEditColumns => "配置列及其顺序",

        // ---- Status bar ----
        Connected => "已连接",
        Connecting => "连接中",
        NoLink => "无连接",
        NoActiveServer => "无活动服务器",
        SignalsCount => "信号: {0} | 已选: {1}",
        Writing => "写入中...",
        WrittenSummary => "服务器已接受：{0} / {1}",
        UpdatedAt => "更新于 {0}",
        RowsHint => {
            "表格: ↑↓ Shift+PgUp/PgDn Home/End {0} {1} {3} {4} {5} · {6} — 树 · {2} — 服务器"
        }
        TreeHint => "树: ↑↓ ← → 空格 {1} {2} · {3} — 表格 · {0} — 服务器",
        KeyboardIdleHint => "{1} — 表格 / 树 · {0} — 连接",

        // ---- Bottom panels ----
        ScriptTab => "脚本",
        JournalTab => "日志",
        HintScriptTab => "脚本 — 编辑与执行",
        HintJournalTab => "日志 — 操作记录",
        SaveScriptLabel => "保存脚本",
        LoadScriptLabel => "加载脚本",
        ClearJournalLabel => "清除日志",
        SaveJournalLabel => "保存日志...",
        ScriptsNotImpl => "脚本: 解释器未实现",
        ThemeTitle => "外观主题",
        ThemeSystem => "跟随系统",
        ThemeLight => "浅色",
        ThemeDark => "深色",
        ThemeHint => "保存后生效",
        HintScriptTabNotImpl => "脚本 — 未实现: 暂无解释器，只能编辑和保存文本",
        EntriesCount => "条目: {0}",

        // ---- Navigation ----
        Servers => "服务器",
        Connect => "连接",
        ConnectPrompt => "连接到 OPC DA 或 OPC UA 服务器。",
        Add => "添加",
        AddChildren => "子项",
        AddByNameShort => "按名称...",
        BrowseProto => "浏览: {0}",
        UpdatingTree => "正在更新树...",
        LoadingTree => "正在加载树...",
        TreeNotLoaded => "树未加载。",
        NoConnection => "无连接。",
        ResumeAttempts => "恢复尝试",
        StopAttempts => "停止尝试",
        Refresh => "刷新",
        CopyNodeName => "复制节点名称",
        NodeNameCopied => "节点名称已复制",
        ExpandCollapse => "展开 / 折叠",
        ViewsRoot => "视图",
        OnlyVars => "仅变量",
        NameNotResolved => "名称 \"{0}\" 无法解析为 OPC UA BrowsePath",

        // ---- Table ----
        TableEmpty => "信号表为空。",
        TableEmptyHelp => "连接到服务器，展开分支并添加信号。",
        TableHint => "提示: 在树中双击信号或右键单击 → 添加。",
        TableEmptyInner => "无信号 — 请从树中添加、右键添加或拖放到此处",
        TableValueExpand => "展开值",
        TableValueCollapse => "折叠值",
        TableEmptyKeys => "{0} — 连接服务器 · {1} — 切换树和表格",
        TableEmptyDragHint => "从树中拖动标签，或在节点上按 Enter",
        TableEmptyConnect => "连接 ({0})",
        EscHint => "按 Esc 清除选择。右键单击行可打开菜单。",
        PkmMoveColumn => "右键单击以移动列",
        MoveLeft => "左移",
        MoveRight => "右移",
        AddThisNode => "添加 (从此节点)",
        AddChildrenCtx => "添加子项",
        CopyFullNameCtx => "复制完整名称",
        CopyValueCtx => "复制值",
        SelectAllCtx => "全选",
        ClearSelectionCtx => "清除选择",
        SaveCsvCtx => "保存 CSV",
        CtxColumns => "列...",
        CtxAddByName => "按名称添加...",
        CtxRefreshBranch => "刷新分支",
        CtxSaveCsv => "保存 CSV...",
        CtxSelectAll => "全选",
        CtxClearSelection => "清除选择",
        ColumnsDragHandle => "拖动以排序",
        ColumnsReorderHint => "Alt+↑/↓ 或 Ctrl+↑/↓ 移动行",
        ValueCopied => "值已复制",
        FullNameCopied => "完整名称已复制",
        Copied => "已复制",
        ColumnMovedLeft => "列 \"{0}\" 已左移",
        ColumnMovedRight => "列 \"{0}\" 已右移",
        ColStatus => "状态",
        ColFullName => "全名",
        ColWriteType => "写入类型",
        ColType => "类型",
        ColValue => "值",
        ColQuality => "质量",
        ColTime => "更改时间",
        ColServer => "OPC 服务器",
        ColDescription => "描述",
        HintStatus => "行状态",
        HintFullName => "服务器上的完整信号名称",
        HintWriteType => "用户指定的写入类型",
        HintType => "服务器上的信号类型",
        HintValue => "服务器上的当前信号值",
        HintQuality => "信号值的可靠性特征",
        HintTime => "信号最后一次更改的时间",
        HintServer => "服务器的程序标识符",
        HintDescription => "信号描述",

        // ---- Status popup ----
        Server => "服务器",
        ProtocolLabel => "协议:",
        EndpointLabel => "连接点:",
        StateLabel => "状态:",
        DetailsLabel => "详情:",
        SubscribedLabel => "已订阅信号:",
        SubscriptionLabel => "订阅:",
        SubscriptionActive => "活动",
        SubscriptionInactive => "非活动",
        SubscriptionFailed => "失败",
        SelectedLabel => "已选:",
        LastUpdateLabel => "上次更新:",
        ReconnectLabel => "重新连接:",
        RetriesLabel => "重试次数:",
        Close => "关闭",
        AutoReconnectStopped => "自动重连已停止 (尝试: {0})",
        ReconnectingLine => "重新连接: 尝试 {0}，{1} 秒后重试 (上限 {2})",
        ReconnectingAttempt => "重新连接: 尝试 {0}",
        RetriesLine => "{0} (上限 {1})",
        ServerHostSuffix => " 在 {0}",
        SearchingUaNetwork => "正在搜索网络中的 OPC UA 服务器 (mDNS/LDS)...",

        // ---- Connect dialog ----
        ConnectToServer => "连接到 OPC 服务器",
        DaTab => "OPC DA (COM)",
        UaTab => "OPC UA",
        DaServersList => "OPC 服务器列表:",
        FoundServers => "找到的服务器:",
        PollingServers => "正在轮询 OPC 服务器...",
        ListEmptyManual => "列表为空。请手动指定 ProgID。",
        SearchingUa => "正在搜索 OPC UA 服务器...",
        ListEmptyUrl => "列表为空。请手动指定 URL。",
        NetworkAddr => "网络地址:",
        AddressLabel => "地址:",
        HintLocalAddr => "留空 = 本地 (可选)",
        PortLabel => "端口:",
        SearchLan => "搜索本地网络",
        Search => "搜索",
        EmptySearchHint => "留空 — 搜索此计算机和本地网络",
        UaHostHint => "地址或留空",
        UaPortHint => "端口（默认 4840）",
        LdsHostLabel => "LDS 主机",
        LdsHostHint => "可选，主机[:端口]",
        LdsHostTip => {
            "“通过 LDS/mDNS 搜索”向其查询服务器列表的发现服务器 (LDS)。端口默认为 4840。留空：仅查询本机和 mDNS。"
        }
        LdsHostInvalid => "LDS 主机：应为名称或地址，可带端口（主机:端口）",
        SearchingUaLds => "正在向 LDS {0} 搜索 OPC UA 服务器（及 mDNS）...",
        UserLabel => "用户:",
        PasswordLabel => "密码:",
        UrlLabel => "URL:",
        HintProgId => "服务器 ProgID，例如 Alpha.Server",
        HintUaUrl => "opc.tcp://host:4840",
        Recent => "最近使用",
        Cancel => "取消",
        SecPolicyLabel => "安全策略:",
        SecModeLabel => "安全模式:",
        AutoLabel => "自动",
        SecPolicyNone => "无策略",
        SecPolicyBasic128Rsa15 => "Basic128Rsa15",
        SecPolicyBasic256 => "Basic256",
        SecPolicyBasic256Sha256 => "Basic256Sha256",
        SecModeNone => "无安全",
        SecModeSign => "签名",
        SecModeSignAndEncrypt => "签名并加密",
        SecurityInvalidCombination => "安全策略/模式组合无效: {0}",
        SecurityResolveFailed => "无法选择安全端点: {0}",
        TrustServerCertCheck => "信任服务器证书（仅本次会话）",
        TrustServerCertTip => {
            "仅在本次连接期间接受服务器证书，不会永久保存。不检查证书的有效期、主机名和 ApplicationUri。若要永久信任该服务器，请将其证书导入 PKI 存储。"
        }
        IdentityLabel => "身份:",
        IdentityAnonymous => "匿名",
        IdentityUserName => "用户名",
        IdentityCertificate => "证书",
        SessionTimeoutLabel => "会话超时（毫秒）:",
        SearchLdsMdns => "通过 LDS/mDNS 搜索",
        NetworkServersNotFound => "未找到网络服务器（mDNS/LDS 未响应）",
        CertIdentityUnsupported => "所选安全端点不支持证书身份验证",
        PkiPathLabel => "PKI 存储:",
        ClientCertSubjectLabel => "客户端证书:",
        NoClientCert => "未创建（首次连接后出现）",
        SessionTimeoutInfo => "请求的会话超时，毫秒（5000-3600000）。服务器可能会修改该值。",
        UaUserNameRequired => "请输入所选身份对应的用户名。",

        // ---- Add signal ----
        AddSignal => "添加信号",
        ServerLabel => "服务器:",
        SignalName => "信号名称:",
        HintItemId => "OPC DA item id 或 OPC UA NodeId",
        DisplayName => "显示名称:",
        Optional => "可选",
        NotSelected => "未选择",

        // ---- Write dialog ----
        ChangeSignalValue => "更改信号值",
        SignalLabel => "信号: {0}",
        WriteTargetCount => "所选信号数: {0}",
        CurrentValue => "当前值: {0}",
        ServerTypeLabel => "服务器类型: {0}",
        ReadOnly => "信号为只读 (无法写入)",
        CanonicalDaType => "规范 DA 类型: {0}",
        ServerDataType => "服务器数据类型: {0}",
        WriteTypeLabel => "写入类型:",
        ValueLabel => "值:",
        HintWriteValue => "true / 123 / 12.5 / 文本 / 2026-01-31 12:00:00",
        ConfirmAfterWrite => "点击 \"写入\" 后将需要确认。",
        Write => "写入",

        // ---- Columns dialog ----
        ColumnsSetup => "列设置",
        ColumnsHint => "选择列并设置其顺序。",
        IndicatorNote => "(指示器)",
        MovedFormat => "已移动: {0}",
        Reset => "重置",
        Apply => "应用",

        // ---- Options dialog ----
        OptionsTitle => "选项",
        UpdateInterval => "更新间隔",
        Seconds => "秒",
        SecondsShortSuffix => " 秒",
        RecommendedOneSec => "建议值: 1 秒。",
        AutoReconnectTitle => "自动重新连接",
        AutoReconnectCheck => "自动恢复与服务器的连接",
        AutoReconnectWarn => "注意! 设置此选项可能会启动服务器。",
        DisplayOptions => "显示选项",
        ShowColorsCheck => "以不同颜色显示信号",
        ConfirmWriteCheck => "更改信号时请求确认",
        AdvancedTitle => "高级",
        OpTimeoutLabel => "操作超时 (1–1800 秒)。建议 60 秒。",
        TestIntervalLabel => "链路测试间隔 (1–1800 秒)。建议 60 秒。",
        DeadbandLabel => "死区 (绝对值)",
        DeadbandHint => "OPC UA: 仅当数值变化超过死区时才推送更新 (0 = 关闭)。",

        // ---- About dialog ----
        AboutTitle => "关于",
        TestOpcClient => "OPCLook",
        RustEgui2026 => "Rust + egui, 2026",
        OriginalProto => {
            "OPC DA 2.0，COM 接口 IOPCServer / IOPCItemMgt / IOPCSyncIO / IOPCBrowseServerAddressSpace。"
        }
        SupportedLabel => "支持:",
        AboutDaLine => "• OPC DA 2.0 通过 COM (本地及 DCOM);",
        AboutUaLine => "• OPC UA 通过 async-opcua 0.19 (SecurityPolicy None，匿名 / 用户名);",
        AboutTreeLine => "• 地址空间树、信号表、读写、日志;",
        AboutSettingsLine => "• 设置保存到 %APPDATA%\\opclook\\settings.json。",
        ConnectedServers => "已连接的服务器:",
        NoConnectedServers => "无已连接的服务器。",
        VendorNotAvail => {
            "当前 BFF 网关不提供供应商/版本/状态 (GetStatus)；仅提供连接点和状态文本。"
        }
        LimitationsLabel => "本版本的局限:",
        LimitScriptLine => "• 脚本解释器未实现 (保留编辑器及 .stx 文件);",
        LimitQualityLine => "• OPC DA 标准不提供质量写入;",
        LimitDcomLine => "• DCOM/OPC UA 安全由操作系统/服务器配置。",

        // ---- Message/confirm ----
        ConfirmTitle => "确认",
        Yes => "是",
        No => "否",

        // ---- App dialog content ----
        WriteError => "写入错误",
        WriteFailedName => "无法将值写入信号 \"{0}\"。",
        WriteUnknownName => "信号 \"{0}\" 的写入结果未知。",
        WriteNotSubmitted => "写入未提交",
        WriteNotSubmittedName => "信号 \"{0}\" 的写入未提交。",
        AddSignalTitle => "添加信号",
        AddSignalBadNode => "已选择分支节点。请选择信号或使用 \"添加子项\"。",
        ChangeValueTitle => "更改值",
        SameServerMsg => "请仅选择同一服务器的信号。",
        ConfirmWriteTitle => "确认写入",
        ConfirmWriteOne => "将信号 \"{0}\" 的值更改为 \"{1}\"?\n\n此操作可能发出控制信号。",
        ConfirmWriteMany => "将 {0} 个信号的值更改为 \"{1}\"?\n\n此操作可能发出控制信号。",
        DisconnectConfirmTitle => "断开连接",
        DisconnectConfirmText => "断开与服务器 \"{0}\" 的连接 ?",

        // ---- Script dialog ----
        ScriptTitle => "脚本",
        ScriptNotImpl => "此版本未实现脚本解释器。",

        // ---- CSV dialog ----
        LoadTreeDialog => "加载信号树",
        ImportStopped => "用户已停止导入。",
        ImportDone => "完成。已加载信号: {0}",
        ImportProgress => "已处理行: {0} / {1}",

        // ---- App/journal ----
        AppStarted => "OPCLook 已启动",
        Ready => "就绪",
        ConnectingTo => "正在连接到 {0} ({1})...",
        DisconnectingFrom => "正在断开与 {0} 的连接",
        Reconnecting => "正在重新连接...",
        ConnectedInfo => "{0}: {1}",
        AttemptsExhausted20 => "尝试次数已用尽 (20): 自动重连已停止",
        AutoReconnectAtt => "自动重连 (尝试 {0})...",
        ReconnectRestoring => "正在恢复连接...",
        AutoReconnectStoppedByUser => "用户已停止自动重连",
        AttemptsExhausted => "尝试次数已用尽 (20)",
        ReconnectBackoff => "正在重新连接 {0}/{1}，{2} 秒后重试",
        ReconnectStopped => "正在重新连接 {0}/{1} — 自动重连已停止",
        AttemptsExhaustedFor => "尝试次数已用尽 ({0}): 已停止 \"{1}\" 的自动重连",
        ConnLostReconnect => "连接丢失。自动重连: 尝试 {0}/{1}，{2} 秒后重试",
        ReadTimeout => "读取操作超时；将检查连接状况",
        ServersFoundUa => "找到 OPC UA 服务器: {0}",
        ServersFoundDa => "找到 OPC DA 服务器: {0}",
        ReqServerList => "正在请求 OPC DA 服务器列表{0}...",
        SearchingUaHost => "正在搜索 {0}:{1} 上的 OPC UA 服务器...",
        AddFailedName => "无法添加信号 \"{0}\": {1}",
        NoChildSignals => "所选分支没有子信号",
        WrittenValue => "值已写入 \"{0}\"",
        WriteAcceptedStatus => "服务器已接受写入；物理动作未确认",
        WriteAsyncStatus => "写入已接受并将异步完成；尚未验证源设备",
        WriteUncertainStatus => "写入结果不确定",
        WriteRejectedStatus => "服务器拒绝了写入",
        WriteUnknownStatus => "写入结果未知；写入可能已经发生",
        WriteOutcomeForItem => "\"{0}\": {1}",
        WriteFailed => "无法写入 \"{0}\": {1}",
        WriteDoneSummary => "服务器接受的写入：{0} / {1}",
        StaleData => "过期数据",
        NoData => "无数据",
        WaitingValue => "等待值",
        SignalAdded => "信号已添加到表格",
        AddedSignals => "已添加信号: {0}",
        AddFailedEmpty => "未添加信号: 列表为空或有重复",
        RemovedSignals => "已删除信号: {0}",
        OptionsSaved => "选项已保存",
        ScriptSaved => "脚本已保存",
        ErrorOp => "{0}: {1}",
        OpConnect => "连接",
        OpDisconnect => "断开连接",
        OpBrowse => "浏览",
        OpAdd => "添加信号",
        OpRemove => "删除信号",
        OpRead => "读取",
        OpWrite => "写入",
        OpTest => "链路检查",
        OpDiscover => "服务器搜索",
        OpSearchServers => "服务器搜索",
        OpBrowseViews => "浏览视图",
        OpResolvePath => "解析路径",
        OpSearchNetwork => "LDS/mDNS 搜索",
        OpSubscription => "订阅",
        OpUaRuntime => "OPC UA 运行时",
        NotWritable => "信号 \"{0}\" 不允许写入 (只读)",
        WriteRequested => "为 {0} 个信号发起写入请求，类型 {1}",
        ScriptNotImplNote => "此版本尚未实现脚本解释器；编辑器保存文本。",
        ScriptSavedPath => "脚本已保存: {0}",
        ScriptSaveErr => "保存 {0} 出错: {1}",
        ScriptLoadedPath => "脚本已加载: {0}",
        ScriptLoadErr => "加载 {0} 出错: {1}",
        JournalSaved => "日志已保存: {0}",
        JournalSaveErr => "保存日志出错: {0}",
        JournalLoaded => "日志已加载: {0} 行",
        JournalLoadErr => "加载日志出错: {0}",
        TableSaved => "信号表已保存: {0}",
        CsvSaveErr => "保存 CSV 出错: {0}",
        CsvReadErr => "无法读取 {0}: {1}",
        CsvNoSignals => "CSV 不包含信号行",
        CsvServerNotConnected => "CSV: 服务器 \"{0}\" 未连接",
        NotInteger => "\"{0}\" 不是整数",
        NotUnsigned => "\"{0}\" 不是无符号整数",
        NotNumber => "\"{0}\" 不是数字",
        NotBool => "\"{0}\" 不是布尔值",
        WriteInputTooLarge => "值超过 1 MiB 或写入批次的估计大小超过 16 MiB",
        NotDatetime => "无法解析日期/时间 \"{0}\"",
        ValueNotServer => "未指定服务器",
        PortInvalid => "请指定有效端口",
        HostInvalid => "请指定服务器地址",
        NoServerSelected => "未选择服务器",
        NameRequired => "请指定信号名称",

        // ---- Drag & drop ----
        DropToAdd => "松开以添加信号",
        DropCrossSession => "只能拖放来自活动服务器的节点",
        DropEmptyBranch => "该分支没有可添加的信号",
        AddChildrenProgress => "正在添加子项: {0}…",
        AddChildrenCapHit => "已达节点上限 ({0})；已停止添加子项",
        FolChildrenNone => "递归添加未在此分支找到信号",

        // ---- Session tabs ----
        AddTabTooltip => "打开连接对话框",
        CloseTabTooltip => "隐藏该选项卡（容器仍保留）",
        DoubleClickServerHint => "双击以打开选项卡（如果尚未连接则连接）",
        RemoveFromList => "从列表中删除",
        RemoveFromListConfirmTitle => "删除会话",
        RemoveFromListConfirmText => "从列表中删除会话 \"{0}\" 及其所有信号？",
        EmptyTabsHint => "在连接对话框中双击服务器可打开选项卡",
        RefreshF5Button => "刷新 ({0})",
        AddNodeTooltip => "将所选信号添加到表格",
        AddByNameTooltip => "按名称添加信号 (Item ID / NodeId)",
        TreeExpandLabel => "展开",
        TreeCollapseLabel => "折叠",

        // ---- Servers area (left panel) ----
        AddServer => "添加服务器",
        RemoveServerTooltip => "删除服务器和标签页",
        ReconnectTooltip => "重新连接",
        ReconnectCount => "{0}/{1}",
        NewConnection => "新连接",

        // ---- Network settings tab ----
        NetworkTab => "网络设置",
        NetworkAuto => "自动",
        NetworkRefresh => "刷新",
        NetworkApplyHint => "更改会立即生效并保存。",
        NetworkAdaptersTitle => "网络适配器",
        NetworkPreferredLine => "首选：{0}",
        NetworkUsedLine => "正在使用：{0}",
        NetworkFallbackLine => "回退原因：{0}",
        NetworkAddressesLine => "地址：{0}",
        NetworkGatewayLine => "网关：{0}",
        NetworkMetricLine => "跃点数：{0}",
        NetworkStateLabel => "状态：{0}",
        NetworkTypePhysical => "物理",
        NetworkTypeVirtual => "虚拟",
        NetworkTypeLoopback => "环回",
        NetworkNoAdapters => "未找到适配器",
        NetworkErrorLine => "网络错误：{0}",
        NetworkFallbackPreferredMissing => "找不到已保存的适配器",
        NetworkFallbackPreferredDown => "已保存的适配器不可用",
        NetworkFallbackPreferredNoAddress => "已保存的适配器没有地址",
        NetworkFallbackNoPhysical => "找不到物理适配器",
        NetworkFallbackAutoPhysical => "已自动选择物理适配器",
        NetworkFallbackAutoLoopback => "使用环回",
        NetworkLoopbackName => "环回",
        NetworkAddressChoice => "本地 IP",
        NetworkStateUp => "已连接",
        NetworkStateDown => "已断开",
        NetworkStateTesting => "测试中",
        NetworkStateDormant => "休眠",
        NetworkStateNotPresent => "不存在",
        NetworkStateLowerLayerDown => "下层断开",
        NetworkStateUnknown => "未知",

        // ---- UA subnet scan (N+1) ----
        SearchUaSubnet => "扫描网络",
        SearchingUaSubnet => "正在扫描子网中的 OPC UA 服务器...",
        UaScanProgress => "已检查 {0}/{1}，找到 {2}",
        UaScanStop => "停止",
        UaScanConfirmText => "子网包含 {0} 个地址，超过 {1} 的上限。仅扫描本机（环回和本地地址）？",
        UaScanTooLarge => "子网过大，无法扫描（{0} 个地址）。请缩小子网掩码。",
        UaScanFailed => "扫描失败：{0}",
        UaScanNoResults => "子网中未找到 OPC UA 服务器",
        UaScanNoLease => "没有可用于扫描的网络适配器",
        NetworkAdapterCombo => "网络适配器",
        NetworkNotSelected => "未选择",
        NetworkSelectAdapterHint => "请选择网络适配器以进行网络搜索和连接。",
        NetworkAdapterDescription => "描述",
        NetworkDaRouteLine => "DA 路由：{0}",
        NetworkStaleAdapter => "已保存的适配器不可用。请选择可用适配器。",
        NetworkEmptyList => "列表未加载。请点击“刷新”。",
        NetworkRefreshFailed => "无法刷新列表",
        NetworkNoIp => "无 IP",
        NetworkForget => "忘记",
        UaTreeSummary => "地址：{0} · 安全配置：{1}",
        UaTreeSecurityOnConnect => "服务器未报告此地址的安全配置 — 将在连接时请求（自动）",
        UaTreeUnsupported => "此客户端不支持",
        UaTreeNoEndpoints => "服务器未报告连接地址",
        SecPolicyAes128Sha256RsaOaep => "Aes128_Sha256_RsaOaep",
        SecPolicyAes256Sha256RsaPss => "Aes256_Sha256_RsaPss",
        UaSessionCounter => "UA 会话：{0}/{1}",
        UaSessionLimitReached => "已达到 UA 会话上限（{0}）。请删除或关闭一个会话。",
        SelectAdapterRequired => "请选择网络适配器",
        DaRouteError => "无法准备 DA 路由：{0}",
        DaRouteNoAdapter => "未选择网络适配器",
        DaRouteHelperStarting => "路由助手正在启动；请重试连接",
        DaRouteHelperUnavailable => "路由助手不可用（UAC 被拒绝或找不到助手）",
        DaRouteConflict => "到 {0} 的路由已使用其他适配器",
        DaRouteNoGateway => "所选适配器没有到 {0} 的网关",
        DaRouteResolve => "无法解析主机 {0}",
        DaRouteAdd => "无法添加到 {0} 的路由：{1}",
        DaRouteVerify => "到 {0} 的路由未使用所选适配器",
        DaRouteHelperRunning => "助手正在运行，路由有效",
        DaRouteHelperIdle => "未创建路由",
    }
}

// ---------------------------------------------------------------------------
// Backend messages (`BackendMsg`)
// ---------------------------------------------------------------------------

/// Picks the wording of one language and formats it with the variant's fields
/// (named arguments are captured from the surrounding bindings).
macro_rules! l {
    ($lang:expr, $ru:literal, $en:literal, $zh:literal) => {
        match $lang {
            Lang::Ru => format!($ru),
            Lang::En => format!($en),
            Lang::Zh => format!($zh),
        }
    };
}

fn protection_text(lang: Lang, protection: UaProtection) -> &'static str {
    match (protection, lang) {
        (UaProtection::None, Lang::Ru) => "без шифрования",
        (UaProtection::None, Lang::En) => "not encrypted",
        (UaProtection::None, Lang::Zh) => "未加密",
        (UaProtection::Sign, Lang::Ru) => "только подпись, данные не шифруются",
        (UaProtection::Sign, Lang::En) => "signed only, data is not encrypted",
        (UaProtection::Sign, Lang::Zh) => "仅签名，数据未加密",
        (UaProtection::SignAndEncrypt, Lang::Ru) => "подпись и шифрование",
        (UaProtection::SignAndEncrypt, Lang::En) => "signed and encrypted",
        (UaProtection::SignAndEncrypt, Lang::Zh) => "签名并加密",
        (UaProtection::Unknown, Lang::Ru) => "режим защиты не определён",
        (UaProtection::Unknown, Lang::En) => "protection mode is not defined",
        (UaProtection::Unknown, Lang::Zh) => "保护模式未定义",
    }
}

pub fn da_route_error(lang: Lang, error: &opclook_contract::da_route::DaRouteError) -> String {
    use opclook_contract::da_route::DaRouteError as E;
    let detail = match error {
        E::RequestIdsExhausted => return tr(lang, Tr::RequestIdsExhausted).to_string(),
        E::NoAdapter => tr(lang, Tr::DaRouteNoAdapter).to_string(),
        E::AdapterUnavailable => tr(lang, Tr::SelectAdapterRequired).to_string(),
        E::Resolve(host) => fmt(lang, Tr::DaRouteResolve, &[host]),
        E::InvalidTarget(ip) => fmt(lang, Tr::DaRouteResolve, &[&ip.to_string()]),
        E::NoGateway(ip) => fmt(lang, Tr::DaRouteNoGateway, &[&ip.to_string()]),
        E::Conflict(ip) => fmt(lang, Tr::DaRouteConflict, &[&ip.to_string()]),
        E::HelperStarting => tr(lang, Tr::DaRouteHelperStarting).to_string(),
        E::HelperUnavailable(detail) => {
            format!("{}: {}", tr(lang, Tr::DaRouteHelperUnavailable), detail)
        }
        E::UacDeclined => tr(lang, Tr::DaRouteHelperUnavailable).to_string(),
        E::Add(ip, detail) => fmt(lang, Tr::DaRouteAdd, &[&ip.to_string(), detail]),
        E::Verify(ip) => fmt(lang, Tr::DaRouteVerify, &[&ip.to_string()]),
    };
    fmt(lang, Tr::DaRouteError, &[&detail])
}

/// Render abnormal worker shutdown for the requested interface language.
pub fn shutdown_error_text(
    lang: Lang,
    protocol: &str,
    outcome: opclook_contract::model::ShutdownOutcome,
) -> Option<String> {
    use opclook_contract::model::ShutdownOutcome;
    let key = match outcome {
        ShutdownOutcome::Joined => return None,
        ShutdownOutcome::Panicked => Tr::WorkerPanicked,
        ShutdownOutcome::TimedOut => Tr::WorkerTimedOut,
    };
    Some(fmt(lang, key, &[&protocol]))
}

pub fn send_error_text(lang: Lang, error: opclook_contract::model::SendError) -> &'static str {
    use opclook_contract::model::SendError;
    tr(
        lang,
        match error {
            SendError::GatewayClosed => Tr::GatewayClosed,
            SendError::WorkerClosed => Tr::WorkerClosed,
        },
    )
}

/// Render every backend message in the interface language.
pub fn write_outcome_text(lang: Lang, outcome: &opclook_contract::model::WriteOutcome) -> String {
    use opclook_contract::model::{NativeWriteStatus, WriteDisposition};
    let key = match outcome.disposition {
        WriteDisposition::Accepted => Tr::WriteAcceptedStatus,
        WriteDisposition::AcceptedAsynchronously => Tr::WriteAsyncStatus,
        WriteDisposition::Uncertain => Tr::WriteUncertainStatus,
        WriteDisposition::Rejected => Tr::WriteRejectedStatus,
        WriteDisposition::Unknown => Tr::WriteUnknownStatus,
    };
    let mut result = tr(lang, key).to_string();
    if let Some(raw) = outcome.raw {
        let (protocol, code) = match raw {
            NativeWriteStatus::Ua(code) => ("UA", code),
            NativeWriteStatus::Da(code) => ("DA", code as u32),
            NativeWriteStatus::UaService(code) => ("UA service", code),
            NativeWriteStatus::DaService(code) => ("DA service", code as u32),
        };
        result.push_str(&format!(" ({protocol} 0x{code:08X})"));
    }
    if let Some(reason) = &outcome.reason {
        result.push_str(": ");
        result.push_str(&backend_text(lang, reason));
    }
    if !outcome.detail.is_empty() {
        result.push_str(": ");
        result.push_str(&outcome.detail);
    }
    result
}

pub fn backend_text(lang: Lang, msg: &BackendMsg) -> String {
    use BackendMsg as M;
    match msg {
        // ---- pass-through / composition ----
        M::InvalidItemHandle => tr(lang, Tr::InvalidItemHandle).to_string(),
        M::Raw(text) => text.clone(),
        M::SendFailed(error) => send_error_text(lang, *error).to_string(),
        M::UaServiceError { service, detail } => {
            use opclook_contract::model::UaService;
            let label = match service {
                UaService::Browse => l!(lang, "обзор", "browse", "浏览"),
                UaService::BrowseNext => l!(lang, "продолжение обзора", "browse next", "继续浏览"),
                UaService::BrowseViews => {
                    l!(lang, "обзор представлений", "browse views", "浏览视图")
                }
                UaService::TranslatePath => {
                    l!(lang, "разрешение пути", "translate path", "解析路径")
                }
                UaService::Read => l!(lang, "чтение", "read", "读取"),
                UaService::Write => l!(lang, "запись", "write", "写入"),
            };
            format!("{label}: {detail}")
        }
        M::DaRoute(error) => da_route_error(lang, error),
        M::RequestTimeout { seconds } => fmt(lang, Tr::NoReplyFromServer, &[seconds]),
        M::Context { context, cause } => {
            format!("{context}: {}", backend_text(lang, cause))
        }
        M::Lines(lines) => lines
            .iter()
            .map(|line| backend_text(lang, line))
            .collect::<Vec<_>>()
            .join("\n"),
        // ---- session state ----
        M::NoConnection => l!(lang, "нет соединения", "no connection", "未连接"),
        M::SessionAlreadyConnected => l!(
            lang,
            "сессия уже подключена",
            "the session is already connected",
            "会话已连接"
        ),
        M::ConnectInProgress => l!(
            lang,
            "подключение уже выполняется",
            "a connection attempt is already in progress",
            "正在连接中"
        ),
        M::ConnectCancelled => l!(
            lang,
            "подключение отменено",
            "connection cancelled",
            "连接已取消"
        ),
        M::ConnectionClosed => l!(
            lang,
            "соединение закрыто",
            "connection closed",
            "连接已关闭"
        ),
        M::SessionClosedAfterTimeout => l!(
            lang,
            "сессия закрыта после таймаута запроса",
            "the session was closed after a request timeout",
            "请求超时后会话已关闭"
        ),
        M::RequestTimedOutSessionClosed => l!(
            lang,
            "таймаут запроса OPC UA: сессия закрыта",
            "OPC UA request timed out: the session was closed",
            "OPC UA 请求超时：会话已关闭"
        ),
        M::SignalNotSubscribed => l!(
            lang,
            "сигнал не подписан",
            "the signal is not subscribed",
            "信号未订阅"
        ),
        M::NodeNotSubscribed => l!(
            lang,
            "узел не подписан",
            "the node is not subscribed",
            "节点未订阅"
        ),
        M::ServerReturnedNoValue => l!(
            lang,
            "сервер не вернул значение",
            "the server returned no value",
            "服务器未返回值"
        ),
        M::ItemNoResult => l!(
            lang,
            "сервер не вернул результат для элемента",
            "the server returned no result for the item",
            "服务器未返回该项的结果"
        ),
        // ---- unsupported / routing ----
        M::DaOnlyOnWindows => l!(
            lang,
            "OPC DA поддерживается только в Windows (COM)",
            "OPC DA is supported on Windows only (COM)",
            "OPC DA 仅在 Windows 上受支持（COM）"
        ),
        M::DaNoUaServerSearch => l!(
            lang,
            "поиск OPC UA серверов не поддерживается для OPC DA",
            "searching for OPC UA servers is not supported for OPC DA",
            "OPC DA 不支持搜索 OPC UA 服务器"
        ),
        M::DaNoUaNetworkSearch => l!(
            lang,
            "поиск OPC UA сети не поддерживается для OPC DA",
            "the OPC UA network search is not supported for OPC DA",
            "OPC DA 不支持 OPC UA 网络搜索"
        ),
        M::DaNoViews => l!(
            lang,
            "представления поддерживаются только для OPC UA",
            "views are supported for OPC UA only",
            "仅 OPC UA 支持视图"
        ),
        M::DaNoBrowsePath => l!(
            lang,
            "разрешение пути BrowsePath поддерживается только для OPC UA",
            "resolving a BrowsePath is supported for OPC UA only",
            "仅 OPC UA 支持解析 BrowsePath"
        ),
        M::UaNeedsServerUrl => l!(
            lang,
            "для OPC UA укажите URL сервера (opc.tcp://host:port)",
            "for OPC UA enter the server URL (opc.tcp://host:port)",
            "OPC UA 请输入服务器 URL (opc.tcp://host:port)"
        ),
        M::UaUrlScheme => l!(
            lang,
            "URL OPC UA должен начинаться с opc.tcp://",
            "the OPC UA URL must start with opc.tcp://",
            "OPC UA URL 必须以 opc.tcp:// 开头"
        ),
        M::InvalidDiscoveredUaUrl => l!(
            lang,
            "сервер сообщил недопустимый URL OPC UA; адрес отклонён",
            "the server advertised an invalid OPC UA URL; the address was rejected",
            "服务器公布了无效的 OPC UA URL，地址已拒绝"
        ),
        M::UaRuntimeStartFailed { detail } => l!(
            lang,
            "не удалось запустить tokio runtime: {detail}",
            "could not start the tokio runtime: {detail}",
            "无法启动 tokio 运行时：{detail}"
        ),
        // ---- DA router ----
        M::DaSessionThreadCrashed => l!(
            lang,
            "поток сессии OPC DA завершился аварийно",
            "the OPC DA session thread terminated abnormally",
            "OPC DA 会话线程异常终止"
        ),
        M::DaSessionThreadSpawnFailed { detail } => l!(
            lang,
            "не удалось создать поток сессии: {detail}",
            "could not create the session thread: {detail}",
            "无法创建会话线程：{detail}"
        ),
        M::DaTooManyDiscoveries => l!(
            lang,
            "слишком много одновременных запросов обнаружения, подождите",
            "too many simultaneous discovery requests, please wait",
            "同时进行的发现请求过多，请稍候"
        ),
        M::DaDiscoveryCrashed => l!(
            lang,
            "обнаружение серверов завершилось аварийно",
            "server discovery terminated abnormally",
            "服务器发现异常终止"
        ),
        M::DaDiscoverySpawnFailed { detail } => l!(
            lang,
            "не удалось создать поток обнаружения: {detail}",
            "could not create the discovery thread: {detail}",
            "无法创建发现线程：{detail}"
        ),
        M::DaDcomSecurityNotSet { detail } => l!(
            lang,
            "параметры безопасности DCOM не заданы (уже настроены процессом): {detail}",
            "DCOM security was not set (already configured by the process): {detail}",
            "未设置 DCOM 安全参数（进程已配置）：{detail}"
        ),
        // ---- DA connect / browse / test ----
        M::DaProgIdMissing => l!(
            lang,
            "не указан программный идентификатор сервера",
            "the server ProgID is not specified",
            "未指定服务器 ProgID"
        ),
        M::DaNoOpcServer { detail } => l!(
            lang,
            "сервер не поддерживает IOPCServer: {detail}",
            "the server does not support IOPCServer: {detail}",
            "服务器不支持 IOPCServer：{detail}"
        ),
        M::DaGroupCreateFailed { detail } => l!(
            lang,
            "ошибка создания группы на сервере: {detail}",
            "could not create a group on the server: {detail}",
            "在服务器上创建组失败：{detail}"
        ),
        M::DaGroupEmpty => l!(
            lang,
            "сервер вернул пустую группу",
            "the server returned an empty group",
            "服务器返回了空组"
        ),
        M::DaNoItemMgt { detail } => l!(
            lang,
            "группа не поддерживает IOPCItemMgt: {detail}",
            "the group does not support IOPCItemMgt: {detail}",
            "组不支持 IOPCItemMgt：{detail}"
        ),
        M::DaNoSyncIo { detail } => l!(
            lang,
            "группа не поддерживает IOPCSyncIO: {detail}",
            "the group does not support IOPCSyncIO: {detail}",
            "组不支持 IOPCSyncIO：{detail}"
        ),
        M::DaEmptyResult => l!(
            lang,
            "сервер вернул пустой результат",
            "the server returned an empty result",
            "服务器返回了空结果"
        ),
        M::DaNoBrowse => l!(
            lang,
            "сервер не поддерживает обзор адресного пространства",
            "the server does not support browsing the address space",
            "服务器不支持浏览地址空间"
        ),
        M::DaBrowseRootFailed { detail } => l!(
            lang,
            "не удалось перейти в корень: {detail}",
            "could not move to the root: {detail}",
            "无法转到根节点：{detail}"
        ),
        M::DaBrowseBranchFailed { segment, detail } => l!(
            lang,
            "не удалось открыть ветку \"{segment}\": {detail}",
            "could not open the branch \"{segment}\": {detail}",
            "无法打开分支“{segment}”：{detail}"
        ),
        M::DaNamespaceFailed { detail } => l!(
            lang,
            "не удалось определить организацию пространства имён: {detail}",
            "could not determine the namespace organization: {detail}",
            "无法确定命名空间组织方式：{detail}"
        ),
        M::DaLinkLost { detail } => l!(
            lang,
            "нет связи с сервером: {detail}",
            "no connection to the server: {detail}",
            "与服务器失去联系：{detail}"
        ),
        M::DaNoState => l!(
            lang,
            "сервер не вернул состояние",
            "the server returned no state",
            "服务器未返回状态"
        ),
        M::DaServerListFailed { cause } => {
            let detail = backend_text(lang, cause);
            l!(
                lang,
                "не удалось получить список OPC серверов: {detail}",
                "could not get the list of OPC servers: {detail}",
                "无法获取 OPC 服务器列表：{detail}"
            )
        }
        M::DaServerListUnavailable { detail } => l!(
            lang,
            "OPC Server List недоступен: {detail}",
            "OPC Server List is unavailable: {detail}",
            "OPC Server List 不可用：{detail}"
        ),
        M::DaServersNotFound => l!(
            lang,
            "OPC серверы не найдены",
            "no OPC servers found",
            "未找到 OPC 服务器"
        ),
        M::DaEmptyInterface { host } => l!(
            lang,
            "сервер {host} вернул пустой интерфейс",
            "the server {host} returned an empty interface",
            "服务器 {host} 返回了空接口"
        ),
        M::DaProgIdLookupFailed { prog_id, detail } => l!(
            lang,
            "не удалось получить идентификатор сервера \"{prog_id}\": {detail}",
            "could not get the identifier of the server \"{prog_id}\": {detail}",
            "无法获取服务器“{prog_id}”的标识符：{detail}"
        ),
        M::DaOpcEnumUnavailable { host, cause } => {
            let detail = backend_text(lang, cause);
            l!(
                lang,
                "OPCEnum на {host} недоступен: {detail}",
                "OPCEnum on {host} is unavailable: {detail}",
                "{host} 上的 OPCEnum 不可用：{detail}"
            )
        }
        M::DaServerListHostUnavailable { host, detail } => l!(
            lang,
            "OPC Server List на {host} недоступен: {detail}",
            "OPC Server List on {host} is unavailable: {detail}",
            "{host} 上的 OPC Server List 不可用：{detail}"
        ),
        M::DaClsidResolveFailed {
            prog_id,
            host,
            remote,
            local,
        } => {
            let (remote, local) = (backend_text(lang, remote), backend_text(lang, local));
            l!(
                lang,
                "не удалось получить идентификатор сервера \"{prog_id}\" на {host}: OPCEnum: {remote}; локальный реестр: {local}",
                "could not get the identifier of the server \"{prog_id}\" on {host}: OPCEnum: {remote}; local registry: {local}",
                "无法获取 {host} 上服务器“{prog_id}”的标识符：OPCEnum：{remote}；本地注册表：{local}"
            )
        }
        M::DaStateOk => l!(
            lang,
            "связь с сервером в порядке (состояние RUNNING)",
            "the server connection is fine (state RUNNING)",
            "与服务器的连接正常（状态 RUNNING）"
        ),
        M::DaNotReady { name, state } => l!(
            lang,
            "сервер не готов к работе: состояние {name} ({state})",
            "the server is not ready: state {name} ({state})",
            "服务器未就绪：状态 {name} ({state})"
        ),
        M::DaUnknownState { state } => l!(
            lang,
            "сервер вернул неизвестное состояние ({state})",
            "the server returned an unknown state ({state})",
            "服务器返回了未知状态 ({state})"
        ),
        M::DaConnected {
            group_handle,
            update_rate_ms,
            server,
        } => {
            let mut text = l!(
                lang,
                "подключено, группа #{group_handle}, период {update_rate_ms} мс",
                "connected, group #{group_handle}, period {update_rate_ms} ms",
                "已连接，组 #{group_handle}，周期 {update_rate_ms} 毫秒"
            );
            if let Some(status) = server {
                let (major, minor, build, count) =
                    (status.major, status.minor, status.build, status.group_count);
                text.push_str(&l!(
                    lang,
                    ", версия сервера {major}.{minor}.{build}, групп на сервере: {count}",
                    ", server version {major}.{minor}.{build}, groups on the server: {count}",
                    "，服务器版本 {major}.{minor}.{build}，服务器上的组数：{count}"
                ));
                let vendor = status.vendor.trim();
                if !vendor.is_empty() {
                    text.push_str(", ");
                    text.push_str(vendor);
                }
            }
            text
        }
        // ---- DA value conversion ----
        M::DaDateRequired => l!(
            lang,
            "для типа DATE требуется дата/время",
            "the DATE type requires a date/time",
            "DATE 类型需要日期/时间"
        ),
        M::DaBadDate => l!(lang, "некорректная дата", "invalid date", "日期无效"),
        M::DaVtUnsupported { vt } => l!(
            lang,
            "запись типа VT {vt} не поддерживается",
            "writing type VT {vt} is not supported",
            "不支持写入 VT {vt} 类型"
        ),
        M::DaArrayCreateFailed { vt } => l!(
            lang,
            "не удалось создать массив VT {vt}",
            "could not create an array of VT {vt}",
            "无法创建 VT {vt} 数组"
        ),
        M::DaArrayVtUnsupported { vt } => l!(
            lang,
            "запись массива VT {vt} не поддерживается",
            "writing an array of VT {vt} is not supported",
            "不支持写入 VT {vt} 数组"
        ),
        M::DaArrayLockFailed { vt } => l!(
            lang,
            "не удалось заблокировать массив VT {vt}",
            "could not lock the array of VT {vt}",
            "无法锁定 VT {vt} 数组"
        ),
        M::DaArrayAccessFailed => l!(
            lang,
            "не удалось получить доступ к данным массива",
            "could not access the array data",
            "无法访问数组数据"
        ),
        M::DaNotInteger { value } => l!(
            lang,
            "значение \"{value}\" не является целым числом",
            "the value \"{value}\" is not an integer",
            "值“{value}”不是整数"
        ),
        M::DaNotUnsigned { value } => l!(
            lang,
            "значение \"{value}\" не является целым числом без знака",
            "the value \"{value}\" is not an unsigned integer",
            "值“{value}”不是无符号整数"
        ),
        M::DaNegativeValue { value } => l!(
            lang,
            "значение {value} отрицательное",
            "the value {value} is negative",
            "值 {value} 为负数"
        ),
        M::DaNotIntegerAny => l!(
            lang,
            "значение не приводится к целому",
            "the value cannot be converted to an integer",
            "值无法转换为整数"
        ),
        M::DaNotUnsignedAny => l!(
            lang,
            "значение не приводится к целому без знака",
            "the value cannot be converted to an unsigned integer",
            "值无法转换为无符号整数"
        ),
        M::DaNotNumberAny => l!(
            lang,
            "значение не приводится к числу",
            "the value cannot be converted to a number",
            "值无法转换为数字"
        ),
        // ---- UA value conversion ----
        M::WriteMultiDim => l!(
            lang,
            "запись многомерных массивов не поддерживается; введите одномерный массив",
            "writing multi-dimensional arrays is not supported; enter a one-dimensional array",
            "不支持写入多维数组；请输入一维数组"
        ),
        M::WriteNodeIdInvalid { value } => l!(
            lang,
            "некорректный NodeId: \"{value}\"",
            "invalid NodeId: \"{value}\"",
            "NodeId 无效：“{value}”"
        ),
        M::WriteOtherType { value } => l!(
            lang,
            "нельзя записать значение типа Other: {value}",
            "a value of type Other cannot be written: {value}",
            "无法写入 Other 类型的值：{value}"
        ),
        M::WriteNotBoolean { value } => l!(
            lang,
            "значение \"{value}\" не является логическим для Boolean",
            "the value \"{value}\" is not a Boolean",
            "值“{value}”不是 Boolean 类型的逻辑值"
        ),
        M::WriteOutOfRange { value, ty } => l!(
            lang,
            "значение {value} вне диапазона {ty}",
            "the value {value} is out of range for {ty}",
            "值 {value} 超出 {ty} 的范围"
        ),
        M::WriteNotInteger { value, ty } => l!(
            lang,
            "значение \"{value}\" не является целым числом для {ty}",
            "the value \"{value}\" is not an integer for {ty}",
            "值“{value}”不是 {ty} 所需的整数"
        ),
        M::WriteNotUnsigned { value } => l!(
            lang,
            "значение {value} не является беззнаковым целым для UInt64",
            "the value {value} is not an unsigned integer for UInt64",
            "值 {value} 不是 UInt64 所需的无符号整数"
        ),
        M::WriteNotDateTime { value } => l!(
            lang,
            "значение \"{value}\" не является датой/временем для DateTime",
            "the value \"{value}\" is not a date/time for DateTime",
            "值“{value}”不是 DateTime 所需的日期/时间"
        ),
        M::WriteNotByteString { value } => l!(
            lang,
            "значение {value} не является байтовой строкой для ByteString",
            "the value {value} is not a byte string for ByteString",
            "值 {value} 不是 ByteString 所需的字节串"
        ),
        M::WriteNotText { value } => l!(
            lang,
            "значение {value} не является текстом для LocalizedText",
            "the value {value} is not text for LocalizedText",
            "值 {value} 不是 LocalizedText 所需的文本"
        ),
        M::WriteNotNodeId { value } => l!(
            lang,
            "значение {value} не является NodeId",
            "the value {value} is not a NodeId",
            "值 {value} 不是 NodeId"
        ),
        M::WriteNotQualifiedName { value } => l!(
            lang,
            "значение {value} не является QualifiedName",
            "the value {value} is not a QualifiedName",
            "值 {value} 不是 QualifiedName"
        ),
        M::WriteBadNsPrefix { value } => l!(
            lang,
            "некорректный префикс пространства имён в \"{value}\"",
            "invalid namespace prefix in \"{value}\"",
            "“{value}”中的命名空间前缀无效"
        ),
        M::WriteNotNumber { value, ty } => l!(
            lang,
            "значение \"{value}\" не является числом для {ty}",
            "the value \"{value}\" is not a number for {ty}",
            "值“{value}”不是 {ty} 所需的数字"
        ),
        M::WriteStructUnsupported { ty } => l!(
            lang,
            "запись типа {ty} (структурированных данных) не поддерживается",
            "writing type {ty} (structured data) is not supported",
            "不支持写入 {ty} 类型（结构化数据）"
        ),
        M::WriteTypeUnsupported { ty } => l!(
            lang,
            "запись типа {ty} не поддерживается",
            "writing type {ty} is not supported",
            "不支持写入 {ty} 类型"
        ),
        M::WriteArrayBuildFailed { detail } => l!(
            lang,
            "не удалось собрать массив: {detail}",
            "could not build the array: {detail}",
            "无法构建数组：{detail}"
        ),
        // ---- UA timeouts ----
        M::TimeoutBrowse => l!(
            lang,
            "таймаут обзора OPC UA",
            "OPC UA browse timed out",
            "OPC UA 浏览超时"
        ),
        M::TimeoutBrowseNext => l!(
            lang,
            "таймаут продолжения обзора OPC UA",
            "OPC UA browse continuation timed out",
            "OPC UA 继续浏览超时"
        ),
        M::TimeoutViews => l!(
            lang,
            "таймаут обзора представлений OPC UA",
            "OPC UA views browse timed out",
            "OPC UA 视图浏览超时"
        ),
        M::TimeoutResolvePath => l!(
            lang,
            "таймаут разрешения пути OPC UA",
            "OPC UA path resolution timed out",
            "OPC UA 路径解析超时"
        ),
        M::TimeoutRead => l!(
            lang,
            "таймаут чтения OPC UA",
            "OPC UA read timed out",
            "OPC UA 读取超时"
        ),
        M::TimeoutWrite => l!(
            lang,
            "таймаут записи OPC UA",
            "OPC UA write timed out",
            "OPC UA 写入超时"
        ),
        M::TimeoutCreateSubscription => l!(
            lang,
            "таймаут создания подписки OPC UA",
            "OPC UA subscription creation timed out",
            "OPC UA 订阅创建超时"
        ),
        M::TimeoutPublishingInterval => l!(
            lang,
            "таймаут изменения интервала публикации",
            "changing the publishing interval timed out",
            "更改发布间隔超时"
        ),
        M::TimeoutDeadband => l!(
            lang,
            "таймаут изменения мёртвой зоны",
            "changing the deadband timed out",
            "更改死区超时"
        ),
        M::TimeoutMonitoredItems => l!(
            lang,
            "таймаут создания контролируемых элементов",
            "creating monitored items timed out",
            "创建监视项超时"
        ),
        M::TimeoutActivateSession => l!(
            lang,
            "таймаут активации OPC UA сессии",
            "OPC UA session activation timed out",
            "OPC UA 会话激活超时"
        ),
        M::TimeoutSearch => l!(
            lang,
            "таймаут поиска серверов",
            "server search timed out",
            "服务器搜索超时"
        ),
        M::TimeoutEndpoints { url } => l!(
            lang,
            "таймаут запроса точек безопасности у {url}",
            "timed out requesting the security endpoints from {url}",
            "向 {url} 请求安全端点超时"
        ),
        // ---- UA browse / path / read / write / test ----
        M::NoViews => l!(
            lang,
            "сервер не имеет представлений (views)",
            "the server has no views",
            "服务器没有视图 (views)"
        ),
        M::BrowsePathRequired => l!(
            lang,
            "укажите путь обзора (BrowsePath)",
            "enter a browse path (BrowsePath)",
            "请输入浏览路径 (BrowsePath)"
        ),
        M::ResolvePathFailed { status } => l!(
            lang,
            "не удалось разрешить путь: {status}",
            "could not resolve the path: {status}",
            "无法解析路径：{status}"
        ),
        M::PathNotFound => l!(
            lang,
            "путь не найден: ни одного совпадения",
            "path not found: no matches",
            "未找到路径：没有匹配项"
        ),
        M::PathBadNsPrefix { part } => l!(
            lang,
            "некорректный префикс пространства имён в \"{part}\"",
            "invalid namespace prefix in \"{part}\"",
            "“{part}”中的命名空间前缀无效"
        ),
        M::PathEmptyName { part } => l!(
            lang,
            "пустое имя в пути \"{part}\"",
            "empty name in the path \"{part}\"",
            "路径“{part}”中的名称为空"
        ),
        M::PathEmpty => l!(
            lang,
            "путь обзора пуст",
            "the browse path is empty",
            "浏览路径为空"
        ),
        M::InvalidNodeId { value } => l!(
            lang,
            "некорректный NodeId: {value}",
            "invalid NodeId: {value}",
            "NodeId 无效：{value}"
        ),
        M::WriteRejected { status } => l!(
            lang,
            "сервер отклонил запись: {status}",
            "the server rejected the write: {status}",
            "服务器拒绝了写入：{status}"
        ),
        M::NoWriteResult => l!(
            lang,
            "сервер не вернул результат записи",
            "the server returned no write result",
            "服务器未返回写入结果"
        ),
        M::UaLinkOk => l!(
            lang,
            "связь с OPC UA сервером в порядке",
            "the OPC UA server connection is fine",
            "与 OPC UA 服务器的连接正常"
        ),
        M::UaBadStatus { status } => l!(
            lang,
            "сервер вернул статус {status}",
            "the server returned status {status}",
            "服务器返回状态 {status}"
        ),
        M::NoStatus => l!(
            lang,
            "сервер не вернул статус",
            "the server returned no status",
            "服务器未返回状态"
        ),
        // ---- UA subscription ----
        M::AttributesReadFailed { cause } => {
            let detail = backend_text(lang, cause);
            l!(
                lang,
                "не удалось прочитать атрибуты узлов OPC UA, тип/доступ не определён: {detail}",
                "could not read the OPC UA node attributes, type/access is unknown: {detail}",
                "无法读取 OPC UA 节点属性，类型/访问权限未知：{detail}"
            )
        }
        M::TypesUnresolved { count } => l!(
            lang,
            "не удалось определить тип {count} узлов OPC UA, запись выполняется с авто-типом",
            "could not determine the type of {count} OPC UA nodes, writes use the automatic type",
            "无法确定 {count} 个 OPC UA 节点的类型，写入将使用自动类型"
        ),
        M::PollOnlyItems { count } => l!(
            lang,
            "{count} тегов сверх лимитов подписки сервера — они обновляются опросом",
            "{count} tags exceed the server's subscription limits; they are updated by polling",
            "{count} 个标签超出服务器订阅限制，将通过轮询更新"
        ),
        M::SubscriptionItemsUnavailable { count } => l!(
            lang,
            "{count} тегов не удалось подписать; опрос не включён",
            "{count} tags could not be monitored; polling is not enabled",
            "无法订阅 {count} 个标签；未启用轮询"
        ),
        M::StructTypesUnavailable { cause } => {
            let cause = backend_text(lang, cause);
            l!(
                lang,
                "типы структур сервера не загружены, структуры показаны текстом: {cause}",
                "the server's structure types were not loaded, structures are shown as text: {cause}",
                "未加载服务器结构类型，结构以文本显示：{cause}"
            )
        }
        M::TimeoutStructTypes => l!(
            lang,
            "таймаут загрузки типов структур OPC UA",
            "loading OPC UA structure types timed out",
            "加载 OPC UA 结构类型超时"
        ),
        M::SubscriptionNotCreated { cause } => {
            let cause = backend_text(lang, cause);
            l!(
                lang,
                "подписка не создана: {cause}",
                "the subscription was not created: {cause}",
                "未创建订阅：{cause}"
            )
        }
        M::CreateSubscriptionFailed { detail } => l!(
            lang,
            "не удалось создать подписку OPC UA: {detail}",
            "could not create the OPC UA subscription: {detail}",
            "无法创建 OPC UA 订阅：{detail}"
        ),
        M::SubscriptionProblem { cause } => {
            let cause = backend_text(lang, cause);
            l!(
                lang,
                "подписка: {cause}",
                "subscription: {cause}",
                "订阅：{cause}"
            )
        }
        M::ChangeIntervalFailed { detail } => l!(
            lang,
            "не удалось изменить интервал: {detail}",
            "could not change the interval: {detail}",
            "无法更改间隔：{detail}"
        ),
        M::ChangeDeadbandFailed { detail } => l!(
            lang,
            "не удалось изменить мёртвую зону: {detail}",
            "could not change the deadband: {detail}",
            "无法更改死区：{detail}"
        ),
        M::DeadbandRejected { count } => l!(
            lang,
            "сервер отклонил изменение мёртвой зоны для {count} элементов",
            "the server rejected the deadband change for {count} items",
            "服务器拒绝了 {count} 个项的死区更改"
        ),
        M::DeleteRejected { count } => l!(
            lang,
            "сервер отклонил удаление {count} элементов",
            "the server rejected the removal of {count} items",
            "服务器拒绝删除 {count} 个项"
        ),
        M::DeleteFailed { detail } => l!(
            lang,
            "не удалось удалить элементы: {detail}",
            "could not remove the items: {detail}",
            "无法删除项：{detail}"
        ),
        M::MonitorFailed { detail } => l!(
            lang,
            "не удалось контролировать элементы: {detail}",
            "could not monitor the items: {detail}",
            "无法监视项：{detail}"
        ),
        // ---- UA connect ----
        M::UaCertUntrusted { url, detail, pki } => l!(
            lang,
            "сертификат сервера {url} не доверенный: {detail}\nВключите «Доверять сертификату сервера (только на эту сессию)» или импортируйте сертификат в PKI-хранилище: {pki}",
            "the certificate of the server {url} is not trusted: {detail}\nEnable \"Trust the server certificate (this session only)\" or import the certificate into the PKI store: {pki}",
            "服务器 {url} 的证书不受信任：{detail}\n请启用“信任服务器证书（仅本次会话）”，或将证书导入 PKI 存储：{pki}"
        ),
        M::UaConnectFailed { url, detail } => l!(
            lang,
            "не удалось подключиться к {url}: {detail}",
            "could not connect to {url}: {detail}",
            "无法连接到 {url}：{detail}"
        ),
        M::UaSessionClosedOnConnect { url } => l!(
            lang,
            "сервер {url} закрыл сессию при подключении",
            "the server {url} closed the session while connecting",
            "服务器 {url} 在连接时关闭了会话"
        ),
        M::UaCertLoginUnsupported => l!(
            lang,
            "выбранная точка безопасности не поддерживает вход по сертификату (Certificate)",
            "the selected endpoint does not support certificate login (Certificate)",
            "所选端点不支持证书登录 (Certificate)"
        ),
        M::UaClientCertMissing { cert, key } => l!(
            lang,
            "клиентский сертификат не найден в PKI ({cert}, {key}) — подключитесь один раз с обычным входом, чтобы сгенерировать его",
            "the client certificate was not found in the PKI ({cert}, {key}) — connect once with an ordinary login to generate it",
            "在 PKI 中未找到客户端证书 ({cert}, {key}) — 请先用普通登录连接一次以生成证书"
        ),
        M::UaClientCertLoadFailed { detail } => l!(
            lang,
            "не удалось загрузить клиентский сертификат для X509: {detail}",
            "could not load the client certificate for X509: {detail}",
            "无法加载 X509 客户端证书：{detail}"
        ),
        M::EndpointsFailed { url, detail } => l!(
            lang,
            "не удалось получить точки безопасности от {url}: {detail}",
            "could not get the security endpoints from {url}: {detail}",
            "无法从 {url} 获取安全端点：{detail}"
        ),
        M::SecurityResolveFailed { detail } => fmt(lang, Tr::SecurityResolveFailed, &[detail]),
        M::UaConnected {
            url,
            policy,
            mode,
            protection,
            secure_available,
        } => {
            let protection = protection_text(lang, *protection);
            let mut text = l!(
                lang,
                "подключено к {url} [{policy} / {mode}] — {protection}",
                "connected to {url} [{policy} / {mode}] — {protection}",
                "已连接到 {url} [{policy} / {mode}] — {protection}"
            );
            if *secure_available {
                text.push_str(match lang {
                    Lang::Ru => {
                        "; сервер предлагает защищённое соединение — выберите политику и режим вручную"
                    }
                    Lang::En => {
                        "; the server offers a secure connection — choose the policy and mode manually"
                    }
                    Lang::Zh => "；服务器提供安全连接 — 请手动选择策略和模式",
                });
            }
            text
        }
        M::CertParseFailed { detail } => l!(
            lang,
            "не удалось разобрать сертификат сервера: {detail}",
            "could not parse the server certificate: {detail}",
            "无法解析服务器证书：{detail}"
        ),
        M::FsCreateFailed { path, detail } => l!(
            lang,
            "не удалось создать {path}: {detail}",
            "could not create {path}: {detail}",
            "无法创建 {path}：{detail}"
        ),
        M::FsSaveFailed { path, detail } => l!(
            lang,
            "не удалось сохранить {path}: {detail}",
            "could not save {path}: {detail}",
            "无法保存 {path}：{detail}"
        ),
        M::FsWriteFailed { path, detail } => l!(
            lang,
            "не удалось записать {path}: {detail}",
            "could not write {path}: {detail}",
            "无法写入 {path}：{detail}"
        ),
        // ---- UA / LDS / mDNS discovery ----
        M::HostRequired => l!(
            lang,
            "укажите адрес сервера (хост)",
            "enter the server address (host)",
            "请输入服务器地址（主机）"
        ),
        M::PortInvalid => l!(
            lang,
            "укажите корректный порт",
            "enter a valid port",
            "请输入有效的端口"
        ),
        M::ResolveAddressFailed { host, detail } => l!(
            lang,
            "не удалось разрешить адрес \"{host}\": {detail}",
            "could not resolve the address \"{host}\": {detail}",
            "无法解析地址“{host}”：{detail}"
        ),
        M::AddressNoIp { host } => l!(
            lang,
            "адрес \"{host}\" не дал ни одного IP-адреса",
            "the address \"{host}\" gave no IP addresses",
            "地址“{host}”未解析出任何 IP 地址"
        ),
        M::UaNoServers { failures } => {
            let mut text = l!(
                lang,
                "OPC UA серверы не найдены",
                "no OPC UA servers found",
                "未找到 OPC UA 服务器"
            );
            if !failures.is_empty() {
                let details = failures
                    .iter()
                    .map(|failure| backend_text(lang, failure))
                    .collect::<Vec<_>>()
                    .join("; ");
                text.push_str(": ");
                text.push_str(&details);
            }
            text
        }
        M::AtUrl { url, cause } => format!("{url}: {}", backend_text(lang, cause)),
        M::ServersNotFoundShort => l!(
            lang,
            "серверы не найдены",
            "no servers found",
            "未找到服务器"
        ),
        M::SearchRejected { detail } => l!(
            lang,
            "запрос поиска отклонён: {detail}",
            "the search request was rejected: {detail}",
            "搜索请求被拒绝：{detail}"
        ),
        M::LdsTimeout { url } => l!(
            lang,
            "LDS {url}: нет ответа (таймаут)",
            "LDS {url}: no response (timeout)",
            "LDS {url}：无响应（超时）"
        ),
        M::LdsFailed { url, detail } => l!(
            lang,
            "LDS {url}: {detail}",
            "LDS {url}: {detail}",
            "LDS {url}：{detail}"
        ),
        M::LdsSearchTimedOut => l!(
            lang,
            "LDS: поиск прерван по таймауту",
            "LDS: the search was interrupted by a timeout",
            "LDS：搜索因超时而中断"
        ),
        M::LdsUnavailable { cause } => {
            let detail = backend_text(lang, cause);
            l!(
                lang,
                "LDS: поиск недоступен: {detail}",
                "LDS: the search is unavailable: {detail}",
                "LDS：搜索不可用：{detail}"
            )
        }
        M::MdnsFailed { cause } => {
            let detail = backend_text(lang, cause);
            l!(
                lang,
                "mDNS: поиск не удался: {detail}",
                "mDNS: the search failed: {detail}",
                "mDNS：搜索失败：{detail}"
            )
        }
        M::MdnsCrashed { detail } => l!(
            lang,
            "mDNS: поиск аварийно завершился: {detail}",
            "mDNS: the search terminated abnormally: {detail}",
            "mDNS：搜索异常终止：{detail}"
        ),
        M::MdnsNotInTime => l!(
            lang,
            "mDNS: поиск не завершился вовремя",
            "mDNS: the search did not finish in time",
            "mDNS：搜索未能按时完成"
        ),
        M::MdnsDiscarded => l!(
            lang,
            "mDNS: результаты отброшены, нет клиента OPC UA для проверки",
            "mDNS: the results were discarded, there is no OPC UA client to verify them",
            "mDNS：结果已丢弃，没有可用于验证的 OPC UA 客户端"
        ),
        M::MdnsNotSupported => l!(
            lang,
            "mDNS: поиск не поддерживается на этой платформе",
            "mDNS: the search is not supported on this platform",
            "mDNS：此平台不支持搜索"
        ),
        M::MdnsVerifyFailed { url, cause } => {
            let detail = backend_text(lang, cause);
            l!(
                lang,
                "mDNS {url}: проверка не удалась: {detail}",
                "mDNS {url}: verification failed: {detail}",
                "mDNS {url}：验证失败：{detail}"
            )
        }
        M::MdnsVerifyTimeout { url } => l!(
            lang,
            "mDNS {url}: проверка не уложилась во время",
            "mDNS {url}: verification did not finish in time",
            "mDNS {url}：验证未能按时完成"
        ),
        M::MdnsApiUnavailable { function } => l!(
            lang,
            "Windows DNS-SD API ({function}) недоступен",
            "the Windows DNS-SD API ({function}) is unavailable",
            "Windows DNS-SD API ({function}) 不可用"
        ),
        M::MdnsCallFailed { function, code } => l!(
            lang,
            "{function} завершился с кодом {code}",
            "{function} failed with code {code}",
            "{function} 失败，代码 {code}"
        ),
        M::MdnsBrowseCancelFailed { code } => l!(
            lang,
            "mDNS: не удалось отменить обзор, код {code}",
            "mDNS: cancelling the browse failed with code {code}",
            "mDNS：取消浏览失败，代码 {code}"
        ),
        M::MdnsResolveStartFailed { instance, code } => l!(
            lang,
            "mDNS: не удалось начать разрешение {instance}, код {code}",
            "mDNS: could not start resolving {instance}, code {code}",
            "mDNS：无法开始解析 {instance}，代码 {code}"
        ),
        M::MdnsResolveCompleted { code } => l!(
            lang,
            "mDNS: разрешение завершилось с кодом {code}",
            "mDNS: the resolution finished with code {code}",
            "mDNS：解析完成，代码 {code}"
        ),
        M::MdnsResolveCancelFailed { code } => l!(
            lang,
            "mDNS: не удалось отменить разрешение, код {code}",
            "mDNS: cancelling the resolution failed with code {code}",
            "mDNS：取消解析失败，代码 {code}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opclook_contract::model::DaServerStatus;

    #[test]
    fn lang_serde_roundtrips() {
        for lang in Lang::ALL {
            let json = serde_json::to_string(&lang).unwrap();
            let back: Lang = serde_json::from_str(&json).unwrap();
            assert_eq!(back, lang);
        }
        // Explicit string forms accepted on load (backward compatible).
        assert_eq!(Lang::from_str("ru"), Some(Lang::Ru));
        assert_eq!(Lang::from_str("en"), Some(Lang::En));
        assert_eq!(Lang::from_str("zh"), Some(Lang::Zh));
        assert_eq!(Lang::from_str("xx"), None);
    }

    #[test]
    fn helper_unavailable_retains_detail_in_every_language() {
        let error =
            opclook_contract::da_route::DaRouteError::HelperUnavailable("pipe failure 42".into());
        for lang in [Lang::Ru, Lang::En, Lang::Zh] {
            let rendered = da_route_error(lang, &error);
            assert!(rendered.contains("pipe failure 42"));
            assert!(rendered.contains(tr(lang, Tr::DaRouteHelperUnavailable)));
        }
    }

    /// One instance of every `BackendMsg` variant with the markers that its
    /// text must contain (a substitution dropped from a language is a bug).
    fn backend_samples() -> Vec<(BackendMsg, Vec<&'static str>)> {
        vec![
            (BackendMsg::Raw("§v§".to_string()), vec!["§v§"]),
            (
                BackendMsg::SendFailed(opclook_contract::model::SendError::WorkerClosed),
                vec![],
            ),
            (
                BackendMsg::UaServiceError {
                    service: opclook_contract::model::UaService::Browse,
                    detail: "§v§".into(),
                },
                vec!["§v§"],
            ),
            (BackendMsg::RequestTimeout { seconds: 17 }, vec!["17"]),
            (
                BackendMsg::DaRoute(opclook_contract::da_route::DaRouteError::NoAdapter),
                vec![],
            ),
            (
                BackendMsg::Context {
                    context: "§context§".to_string(),
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§context§", "§cause§"],
            ),
            (BackendMsg::Lines(vec![BackendMsg::raw("§v§")]), vec!["§v§"]),
            (BackendMsg::NoConnection, vec![]),
            (BackendMsg::InvalidItemHandle, vec![]),
            (BackendMsg::SessionAlreadyConnected, vec![]),
            (BackendMsg::ConnectInProgress, vec![]),
            (BackendMsg::ConnectCancelled, vec![]),
            (BackendMsg::ConnectionClosed, vec![]),
            (BackendMsg::SessionClosedAfterTimeout, vec![]),
            (BackendMsg::RequestTimedOutSessionClosed, vec![]),
            (BackendMsg::SignalNotSubscribed, vec![]),
            (BackendMsg::NodeNotSubscribed, vec![]),
            (BackendMsg::ServerReturnedNoValue, vec![]),
            (BackendMsg::ItemNoResult, vec![]),
            (BackendMsg::DaOnlyOnWindows, vec![]),
            (BackendMsg::DaNoUaServerSearch, vec![]),
            (BackendMsg::DaNoUaNetworkSearch, vec![]),
            (BackendMsg::DaNoViews, vec![]),
            (BackendMsg::DaNoBrowsePath, vec![]),
            (BackendMsg::UaNeedsServerUrl, vec![]),
            (BackendMsg::UaUrlScheme, vec![]),
            (BackendMsg::InvalidDiscoveredUaUrl, vec![]),
            (
                BackendMsg::UaRuntimeStartFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaSessionThreadCrashed, vec![]),
            (
                BackendMsg::DaSessionThreadSpawnFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaTooManyDiscoveries, vec![]),
            (BackendMsg::DaDiscoveryCrashed, vec![]),
            (
                BackendMsg::DaDiscoverySpawnFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::DaDcomSecurityNotSet {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaProgIdMissing, vec![]),
            (
                BackendMsg::DaNoOpcServer {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::DaGroupCreateFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaGroupEmpty, vec![]),
            (
                BackendMsg::DaNoItemMgt {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::DaNoSyncIo {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaEmptyResult, vec![]),
            (BackendMsg::DaNoBrowse, vec![]),
            (
                BackendMsg::DaBrowseRootFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::DaBrowseBranchFailed {
                    segment: "§segment§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§segment§", "§detail§"],
            ),
            (
                BackendMsg::DaNamespaceFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::DaLinkLost {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaNoState, vec![]),
            (
                BackendMsg::DaServerListFailed {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (
                BackendMsg::DaServerListUnavailable {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DaServersNotFound, vec![]),
            (
                BackendMsg::DaEmptyInterface {
                    host: "§host§".to_string(),
                },
                vec!["§host§"],
            ),
            (
                BackendMsg::DaProgIdLookupFailed {
                    prog_id: "§prog_id§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§prog_id§", "§detail§"],
            ),
            (
                BackendMsg::DaOpcEnumUnavailable {
                    host: "§host§".to_string(),
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§host§", "§cause§"],
            ),
            (
                BackendMsg::DaServerListHostUnavailable {
                    host: "§host§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§host§", "§detail§"],
            ),
            (
                BackendMsg::DaClsidResolveFailed {
                    prog_id: "§prog_id§".to_string(),
                    host: "§host§".to_string(),
                    remote: Box::new(BackendMsg::raw("§remote§")),
                    local: Box::new(BackendMsg::raw("§local§")),
                },
                vec!["§prog_id§", "§host§", "§remote§", "§local§"],
            ),
            (BackendMsg::DaStateOk, vec![]),
            (
                BackendMsg::DaNotReady {
                    name: "§name§".to_string(),
                    state: 4242,
                },
                vec!["§name§", "4242"],
            ),
            (BackendMsg::DaUnknownState { state: 4242 }, vec!["4242"]),
            (
                BackendMsg::DaConnected {
                    group_handle: 4242,
                    update_rate_ms: 4242,
                    server: Some(DaServerStatus {
                        major: 7001,
                        minor: 7002,
                        build: 7003,
                        group_count: 7004,
                        vendor: "§vendor§".to_string(),
                    }),
                },
                vec!["4242", "4242", "7001", "7002", "7003", "7004", "§vendor§"],
            ),
            (BackendMsg::DaDateRequired, vec![]),
            (BackendMsg::DaBadDate, vec![]),
            (BackendMsg::DaVtUnsupported { vt: 4242 }, vec!["4242"]),
            (BackendMsg::DaArrayCreateFailed { vt: 4242 }, vec!["4242"]),
            (BackendMsg::DaArrayVtUnsupported { vt: 4242 }, vec!["4242"]),
            (BackendMsg::DaArrayLockFailed { vt: 4242 }, vec!["4242"]),
            (BackendMsg::DaArrayAccessFailed, vec![]),
            (
                BackendMsg::DaNotInteger {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::DaNotUnsigned {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::DaNegativeValue {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (BackendMsg::DaNotIntegerAny, vec![]),
            (BackendMsg::DaNotUnsignedAny, vec![]),
            (BackendMsg::DaNotNumberAny, vec![]),
            (BackendMsg::WriteMultiDim, vec![]),
            (
                BackendMsg::WriteNodeIdInvalid {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteOtherType {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotBoolean {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteOutOfRange {
                    value: "§value§".to_string(),
                    ty: "§ty§".to_string(),
                },
                vec!["§value§", "§ty§"],
            ),
            (
                BackendMsg::WriteNotInteger {
                    value: "§value§".to_string(),
                    ty: "§ty§".to_string(),
                },
                vec!["§value§", "§ty§"],
            ),
            (
                BackendMsg::WriteNotUnsigned {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotDateTime {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotByteString {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotText {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotNodeId {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotQualifiedName {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteBadNsPrefix {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteNotNumber {
                    value: "§value§".to_string(),
                    ty: "§ty§".to_string(),
                },
                vec!["§value§", "§ty§"],
            ),
            (
                BackendMsg::WriteStructUnsupported {
                    ty: "§ty§".to_string(),
                },
                vec!["§ty§"],
            ),
            (
                BackendMsg::WriteTypeUnsupported {
                    ty: "§ty§".to_string(),
                },
                vec!["§ty§"],
            ),
            (
                BackendMsg::WriteArrayBuildFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::TimeoutBrowse, vec![]),
            (BackendMsg::TimeoutBrowseNext, vec![]),
            (BackendMsg::TimeoutViews, vec![]),
            (BackendMsg::TimeoutResolvePath, vec![]),
            (BackendMsg::TimeoutRead, vec![]),
            (BackendMsg::TimeoutWrite, vec![]),
            (BackendMsg::TimeoutCreateSubscription, vec![]),
            (BackendMsg::TimeoutPublishingInterval, vec![]),
            (BackendMsg::TimeoutDeadband, vec![]),
            (BackendMsg::TimeoutMonitoredItems, vec![]),
            (BackendMsg::TimeoutActivateSession, vec![]),
            (BackendMsg::TimeoutSearch, vec![]),
            (
                BackendMsg::TimeoutEndpoints {
                    url: "§url§".to_string(),
                },
                vec!["§url§"],
            ),
            (BackendMsg::NoViews, vec![]),
            (BackendMsg::BrowsePathRequired, vec![]),
            (
                BackendMsg::ResolvePathFailed {
                    status: "§status§".to_string(),
                },
                vec!["§status§"],
            ),
            (BackendMsg::PathNotFound, vec![]),
            (
                BackendMsg::PathBadNsPrefix {
                    part: "§part§".to_string(),
                },
                vec!["§part§"],
            ),
            (
                BackendMsg::PathEmptyName {
                    part: "§part§".to_string(),
                },
                vec!["§part§"],
            ),
            (BackendMsg::PathEmpty, vec![]),
            (
                BackendMsg::InvalidNodeId {
                    value: "§value§".to_string(),
                },
                vec!["§value§"],
            ),
            (
                BackendMsg::WriteRejected {
                    status: "§status§".to_string(),
                },
                vec!["§status§"],
            ),
            (BackendMsg::NoWriteResult, vec![]),
            (BackendMsg::UaLinkOk, vec![]),
            (
                BackendMsg::UaBadStatus {
                    status: "§status§".to_string(),
                },
                vec!["§status§"],
            ),
            (BackendMsg::NoStatus, vec![]),
            (
                BackendMsg::AttributesReadFailed {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (BackendMsg::TypesUnresolved { count: 4242 }, vec!["4242"]),
            (
                BackendMsg::SubscriptionNotCreated {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (
                BackendMsg::StructTypesUnavailable {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (BackendMsg::TimeoutStructTypes, vec![]),
            (BackendMsg::PollOnlyItems { count: 4343 }, vec!["4343"]),
            (
                BackendMsg::SubscriptionItemsUnavailable { count: 4343 },
                vec!["4343"],
            ),
            (
                BackendMsg::CreateSubscriptionFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::SubscriptionProblem {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (
                BackendMsg::ChangeIntervalFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::ChangeDeadbandFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::DeadbandRejected { count: 4242 }, vec!["4242"]),
            (BackendMsg::DeleteRejected { count: 4242 }, vec!["4242"]),
            (
                BackendMsg::DeleteFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::MonitorFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::UaCertUntrusted {
                    url: "§url§".to_string(),
                    detail: "§detail§".to_string(),
                    pki: "§pki§".to_string(),
                },
                vec!["§url§", "§detail§", "§pki§"],
            ),
            (
                BackendMsg::UaConnectFailed {
                    url: "§url§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§url§", "§detail§"],
            ),
            (
                BackendMsg::UaSessionClosedOnConnect {
                    url: "§url§".to_string(),
                },
                vec!["§url§"],
            ),
            (BackendMsg::UaCertLoginUnsupported, vec![]),
            (
                BackendMsg::UaClientCertMissing {
                    cert: "§cert§".to_string(),
                    key: "§key§".to_string(),
                },
                vec!["§cert§", "§key§"],
            ),
            (
                BackendMsg::UaClientCertLoadFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::EndpointsFailed {
                    url: "§url§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§url§", "§detail§"],
            ),
            (
                BackendMsg::SecurityResolveFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::UaConnected {
                    url: "§url§".to_string(),
                    policy: "§policy§".to_string(),
                    mode: "§mode§".to_string(),
                    protection: UaProtection::Sign,
                    secure_available: true,
                },
                vec!["§url§", "§policy§", "§mode§"],
            ),
            (
                BackendMsg::CertParseFailed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::FsCreateFailed {
                    path: "§path§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§path§", "§detail§"],
            ),
            (
                BackendMsg::FsSaveFailed {
                    path: "§path§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§path§", "§detail§"],
            ),
            (
                BackendMsg::FsWriteFailed {
                    path: "§path§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§path§", "§detail§"],
            ),
            (BackendMsg::HostRequired, vec![]),
            (BackendMsg::PortInvalid, vec![]),
            (
                BackendMsg::ResolveAddressFailed {
                    host: "§host§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§host§", "§detail§"],
            ),
            (
                BackendMsg::AddressNoIp {
                    host: "§host§".to_string(),
                },
                vec!["§host§"],
            ),
            (
                BackendMsg::UaNoServers {
                    failures: vec![BackendMsg::raw("§failures§")],
                },
                vec!["§failures§"],
            ),
            (
                BackendMsg::AtUrl {
                    url: "§url§".to_string(),
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§url§", "§cause§"],
            ),
            (BackendMsg::ServersNotFoundShort, vec![]),
            (
                BackendMsg::SearchRejected {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (
                BackendMsg::LdsTimeout {
                    url: "§url§".to_string(),
                },
                vec!["§url§"],
            ),
            (
                BackendMsg::LdsFailed {
                    url: "§url§".to_string(),
                    detail: "§detail§".to_string(),
                },
                vec!["§url§", "§detail§"],
            ),
            (BackendMsg::LdsSearchTimedOut, vec![]),
            (
                BackendMsg::LdsUnavailable {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (
                BackendMsg::MdnsFailed {
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§cause§"],
            ),
            (
                BackendMsg::MdnsCrashed {
                    detail: "§detail§".to_string(),
                },
                vec!["§detail§"],
            ),
            (BackendMsg::MdnsNotInTime, vec![]),
            (BackendMsg::MdnsDiscarded, vec![]),
            (BackendMsg::MdnsNotSupported, vec![]),
            (
                BackendMsg::MdnsVerifyFailed {
                    url: "§url§".to_string(),
                    cause: Box::new(BackendMsg::raw("§cause§")),
                },
                vec!["§url§", "§cause§"],
            ),
            (
                BackendMsg::MdnsVerifyTimeout {
                    url: "§url§".to_string(),
                },
                vec!["§url§"],
            ),
            (
                BackendMsg::MdnsApiUnavailable {
                    function: "§function§".to_string(),
                },
                vec!["§function§"],
            ),
            (
                BackendMsg::MdnsCallFailed {
                    function: "§function§".to_string(),
                    code: 4242,
                },
                vec!["§function§", "4242"],
            ),
            (
                BackendMsg::MdnsBrowseCancelFailed { code: 4242 },
                vec!["4242"],
            ),
            (
                BackendMsg::MdnsResolveStartFailed {
                    instance: "§instance§".to_string(),
                    code: 4242,
                },
                vec!["§instance§", "4242"],
            ),
            (
                BackendMsg::MdnsResolveCompleted { code: 4242 },
                vec!["4242"],
            ),
            (
                BackendMsg::MdnsResolveCancelFailed { code: 4242 },
                vec!["4242"],
            ),
        ]
    }

    fn has_cyrillic(text: &str) -> bool {
        text.chars().any(|c| matches!(c as u32, 0x0400..=0x052F))
    }

    #[test]
    fn every_backend_message_is_localized_in_all_languages() {
        for (msg, markers) in backend_samples() {
            for lang in Lang::ALL {
                let text = backend_text(lang, &msg);
                assert!(
                    !text.trim().is_empty(),
                    "empty backend text for {msg:?} in {lang:?}"
                );
                for marker in &markers {
                    assert!(
                        text.contains(marker),
                        "{lang:?}: the text of {msg:?} dropped the substitution {marker}: {text}"
                    );
                }
            }
        }
    }

    /// U13: the trust checkbox says that trust lasts one session, the tooltip
    /// says what is not checked, and the "untrusted certificate" error points at
    /// the checkbox by its exact label — in every language.
    #[test]
    fn u13_trust_texts_say_session_only_and_what_is_not_checked() {
        for lang in Lang::ALL {
            let label = tr(lang, Tr::TrustServerCertCheck);
            let tip = tr(lang, Tr::TrustServerCertTip);
            let error = backend_text(
                lang,
                &BackendMsg::UaCertUntrusted {
                    url: "opc.tcp://h:1".to_string(),
                    detail: "d".to_string(),
                    pki: "p".to_string(),
                },
            );
            assert!(error.contains(label), "{lang:?}: {error}");
            assert!(tip.contains("ApplicationUri"), "{lang:?}: {tip}");
            assert!(tip.contains("PKI"), "{lang:?}: {tip}");
        }
        assert!(tr(Lang::Ru, Tr::TrustServerCertCheck).contains("только на эту сессию"));
        assert!(tr(Lang::En, Tr::TrustServerCertCheck).contains("this session only"));
        assert!(tr(Lang::Zh, Tr::TrustServerCertCheck).contains("仅本次会话"));
        assert!(tr(Lang::Ru, Tr::TrustServerCertTip).contains("не сохраняется навсегда"));
        assert!(tr(Lang::En, Tr::TrustServerCertTip).contains("not kept permanently"));
        assert!(tr(Lang::Zh, Tr::TrustServerCertTip).contains("不会永久保存"));
    }

    #[test]
    fn backend_messages_follow_the_interface_language() {
        // English and Chinese carry no Russian wording (the substitutions of the
        // samples are ASCII); Russian does (variants that are only a
        // substitution are the pass-through ones).
        for (msg, _) in backend_samples() {
            let passthrough = matches!(
                msg,
                BackendMsg::Raw(_)
                    | BackendMsg::Context { .. }
                    | BackendMsg::Lines(_)
                    | BackendMsg::LdsFailed { .. }
                    | BackendMsg::AtUrl { .. }
            );
            for lang in [Lang::En, Lang::Zh] {
                let text = backend_text(lang, &msg);
                assert!(!has_cyrillic(&text), "{lang:?}: {msg:?} -> {text}");
            }
            if !passthrough {
                let ru = backend_text(Lang::Ru, &msg);
                assert!(has_cyrillic(&ru), "RU text of {msg:?} is not Russian: {ru}");
            }
        }
    }

    #[test]
    fn ua_service_errors_localize_every_operation_and_keep_detail() {
        use opclook_contract::model::UaService;
        for service in [
            UaService::Browse,
            UaService::BrowseNext,
            UaService::BrowseViews,
            UaService::TranslatePath,
            UaService::Read,
            UaService::Write,
        ] {
            let message = BackendMsg::UaServiceError {
                service,
                detail: "detail-marker".into(),
            };
            let ru = backend_text(Lang::Ru, &message);
            let en = backend_text(Lang::En, &message);
            let zh = backend_text(Lang::Zh, &message);
            assert!(
                ru.contains("detail-marker")
                    && en.contains("detail-marker")
                    && zh.contains("detail-marker")
            );
            assert_ne!(ru, en);
            assert_ne!(zh, en);
        }
    }

    #[test]
    fn backend_samples_cover_every_variant() {
        // The enum body is counted from the source so that a variant added
        // without a sample (and so without the checks above) fails here.
        let source = include_str!("../../contract/src/model/message.rs");
        let start = source
            .find("pub enum BackendMsg {")
            .expect("BackendMsg is defined in model.rs");
        let body = &source[start..];
        let end = body.find("\n}\n").expect("end of the enum");
        let variants = body[..end]
            .lines()
            .skip(1)
            .filter(|line| {
                line.starts_with("    ")
                    && !line.starts_with("     ")
                    && line
                        .trim_start()
                        .starts_with(|c: char| c.is_ascii_uppercase())
            })
            .count();
        assert_eq!(backend_samples().len(), variants);
    }

    #[test]
    fn every_key_is_nonempty_in_all_languages() {
        for lang in Lang::ALL {
            for key in Tr::all() {
                let text = tr(lang, key);
                assert!(
                    !text.trim().is_empty(),
                    "empty translation for {key:?} in {lang:?}"
                );
            }
        }
    }

    #[test]
    fn default_language_is_russian() {
        // The application default/fallback language is now Russian.
        assert_eq!(Lang::default(), Lang::Ru);
        assert_eq!(tr(Lang::Ru, Tr::MenuSignals), "Сигналы");
        assert_eq!(fmt(Lang::Ru, Tr::SignalLabel, &[&"SigA"]), "Сигнал: SigA");
        // Chinese and English still translate when explicitly selected.
        assert_eq!(tr(Lang::Zh, Tr::MenuSignals), "信号");
        assert_eq!(tr(Lang::En, Tr::MenuSignals), "Signals");
    }

    #[test]
    fn english_and_chinese_switch_known_labels() {
        assert_eq!(tr(Lang::En, Tr::MenuSignals), "Signals");
        assert_eq!(tr(Lang::Zh, Tr::MenuSignals), "信号");
        assert_eq!(
            fmt(Lang::En, Tr::AddFailedName, &[&"x", &"y"]),
            "Failed to add signal \"x\": y"
        );
        // The language menu renders the three options in every UI language.
        assert_eq!(tr(Lang::En, Tr::LangEn), "English");
        assert_eq!(tr(Lang::Ru, Tr::LangEn), "Английский");
        assert_eq!(tr(Lang::Zh, Tr::LangZh), "简体中文");
        assert_eq!(tr(Lang::En, Tr::MenuLanguage), "Language");
        assert_eq!(tr(Lang::Zh, Tr::MenuLanguage), "语言");
    }

    #[test]
    fn fmt_substitutes_positional_and_named() {
        // {} sequential
        assert_eq!(fmt(Lang::Ru, Tr::Writing, &[]).contains("Запись"), true);
        assert_eq!(
            fmt(Lang::Ru, Tr::ReconnectBackoff, &[&"2", &"20", &"4"]),
            "Повторное подключение 2/20 через 4 с"
        );
        // Repeated named index {0}
        assert_eq!(
            fmt(Lang::Ru, Tr::ReqServerList, &[&" на box"]),
            "Запрос списка OPC DA серверов на box..."
        );
        // Missing arg renders '?'
        assert_eq!(fmt(Lang::Ru, Tr::ConnectedInfo, &[&"a"]), "a: ?");
    }
}
