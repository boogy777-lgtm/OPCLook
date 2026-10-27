//! Runtime language switching with a compile-checked string dictionary.
//!
//! All user-visible strings live in the `Tr` enum. `tr()` maps a key to text
//! for the current `Lang`. Each language is a separate exhaustive `match`, so
//! the compiler guarantees no string can be missing a translation: every `Tr`
//! variant must appear in each dictionary (RU/EN/ZH). A unit test walks
//! `Tr::ALL` to assert every variant yields non-empty text in all languages.

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

/// Every translatable string in the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Tr {
    // ---- Top menu ----
    MenuClient,
    CsvServerAmbiguous,
    CsvServerEmpty,
    CjkFontMissing,
    CjkFontLoaded,
    ResolvingName,
    SettingsReadFailed,
    SettingsParseFailed,
    SettingsParseNoBackup,
    SettingsSaveFailed,
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
    RewrittenFrom,
    OriginalProto,
    ReverseInfo,
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
    OriginNote,
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

impl Tr {
    /// Number of variants (the enum is fieldless `#[repr(u16)]`, so the last
    /// variant's discriminant is `COUNT - 1`).
    pub const COUNT: usize = Tr::DaRouteHelperIdle as usize + 1;

    /// Every variant, in declaration order. Built from discriminants so the
    /// list can never drift out of sync with the enum definition.
    pub fn all() -> Vec<Tr> {
        (0..Self::COUNT)
            .map(|i| {
                // SAFETY: i is a valid discriminant (0 <= i < COUNT), so this
                // is always a valid `Tr` variant.
                unsafe { std::mem::transmute::<u16, Tr>(i as u16) }
            })
            .collect()
    }
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
                Some(a) => out.push_str(&a.to_string()),
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
        CsvServerAmbiguous => {
            "Строка CSV пропущена: имени сервера «{0}» соответствует несколько подключений, укажите точное имя"
        }
        CsvServerEmpty => "Строка CSV пропущена: для сигнала «{0}» не указан сервер",
        CjkFontMissing => {
            "Не найден шрифт для китайского текста (msyh.ttc, simsun.ttc): интерфейс на 中文 будет показан квадратами"
        }
        CjkFontLoaded => "Шрифт для китайского текста: {0}",
        ResolvingName => "Поиск узла…",
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
        HintRefreshF5 => "F5 — обновить ветку",
        HintLoadCsv => "Загрузить сигналы из CSV-файла",
        HintSaveCsv => "Сохранить сигналы в CSV-файл",
        HintEnterTree => "Enter в дереве",
        HintAddChildren => "Добавить все сигналы ветки",
        HintDelSelected => "Del — удалить выбранные",
        HintEnterF2 => "Enter / F2 — изменить значение",
        HintCtrlA => "Ctrl+A — выделить все",
        HintEditColumns => "Настроить состав и порядок столбцов",

        // ---- Status bar ----
        Connected => "Подключено",
        Connecting => "Подключение",
        NoLink => "Нет связи",
        NoActiveServer => "нет активного сервера",
        SignalsCount => "Сигналов: {0} | Выбрано: {1}",
        Writing => "Запись...",
        WrittenSummary => "Записано: {0} из {1}",
        UpdatedAt => "Обновлено {0}",
        RowsHint => "Таблица: ↑↓ Shift+PgUp/PgDn Home/End Ctrl+A Ctrl+C Del Enter F5",
        TreeHint => "Дерево: ↑↓ ← → Space Enter F5",

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
        TreeNotLoaded => "Дерево не загружено — нажмите \"Обновить ветку\".",
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
        TrustServerCertCheck => "Доверять сертификату сервера",
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
        TestOpcClient => "Тестовый OPC клиент",
        RustEgui2026 => "Rust + egui, 2026",
        RewrittenFrom => "Переписан с EleSy OPC Test Client 2.1.0.139 (Delphi, 2012).",
        OriginalProto => {
            "Оригинал: OPC DA 2.0, COM-интерфейсы IOPCServer / IOPCItemMgt / IOPCSyncIO / IOPCBrowseServerAddressSpace."
        }
        ReverseInfo => {
            "Версия для реверс-инжиниринга восстановлена из PE-ресурсов и дизассемблера."
        }
        SupportedLabel => "Поддерживается:",
        AboutDaLine => "• OPC DA 2.0 через COM (локально и по DCOM);",
        AboutUaLine => {
            "• OPC UA через async-opcua 0.19 (SecurityPolicy None, anonymous / username);"
        }
        AboutTreeLine => {
            "• дерево адресного пространства, таблица сигналов, чтение/запись, журнал;"
        }
        AboutSettingsLine => "• сохранение настроек в %APPDATA%\\opcda-egui\\settings.json.",
        ConnectedServers => "Подключённые серверы:",
        NoConnectedServers => "Нет подключённых серверов.",
        VendorNotAvail => {
            "Сведения о поставщике/версии/статусе сервера (GetStatus) не передаются текущим BFF-шлюзом; доступны только точка подключения и текст состояния."
        }
        LimitationsLabel => "Ограничения текущей версии:",
        LimitScriptLine => {
            "• интерпретатор скриптов не реализован (редактор и файлы .stx сохранены);"
        }
        LimitQualityLine => {
            "• запись качества в OPC DA стандартом не предусмотрена (в оригинале — проприетарный EleSy.OPCIntercom);"
        }
        LimitDcomLine => "• DCOM/OPC UA security настраивается средствами ОС/сервера.",

        // ---- Message/confirm ----
        ConfirmTitle => "Подтверждение",
        Yes => "Да",
        No => "Нет",

        // ---- App dialog content ----
        WriteError => "Ошибка записи",
        WriteFailedName => "Не удалось записать значение в сигнал \"{0}\".",
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
        AppStarted => "OPC DA/UA Test Client (Rust + egui) запущен",
        OriginNote => "Оригинал: EleSy OPC Test Client 2.1.0.139 (OPC DA 2.0)",
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
        WriteFailed => "Не удалось записать \"{0}\": {1}",
        WriteDoneSummary => "Запись завершена: {0} из {1} успешно",
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
        CsvServerAmbiguous => {
            "CSV row skipped: server name \"{0}\" matches several connections, give the exact name"
        }
        CsvServerEmpty => "CSV row skipped: no server given for signal \"{0}\"",
        CjkFontMissing => {
            "No font for Chinese text was found (msyh.ttc, simsun.ttc): the 中文 interface will show boxes"
        }
        CjkFontLoaded => "Font for CJK text: {0}",
        ResolvingName => "Resolving node…",
        SettingsReadFailed => "Could not read settings file {0}: {1}. It will not be overwritten",
        SettingsParseFailed => {
            "Settings file {0} is damaged ({1}). A copy was saved to {2}. Defaults are used"
        }
        SettingsParseNoBackup => {
            "Settings file {0} is damaged ({1}) and could not be backed up ({2}). It will not be overwritten; defaults are used"
        }
        SettingsSaveFailed => "Could not save settings {0}: {1}",
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
        HintRefreshF5 => "F5 — refresh branch",
        HintLoadCsv => "Load signals from a CSV file",
        HintSaveCsv => "Save signals to a CSV file",
        HintEnterTree => "Enter in the tree",
        HintAddChildren => "Add all signals of the branch",
        HintDelSelected => "Del — remove selected",
        HintEnterF2 => "Enter / F2 — change value",
        HintCtrlA => "Ctrl+A — select all",
        HintEditColumns => "Configure columns and their order",

        // ---- Status bar ----
        Connected => "Connected",
        Connecting => "Connecting",
        NoLink => "No connection",
        NoActiveServer => "no active server",
        SignalsCount => "Signals: {0} | Selected: {1}",
        Writing => "Writing...",
        WrittenSummary => "Written: {0} of {1}",
        UpdatedAt => "Updated {0}",
        RowsHint => "Table: ↑↓ Shift+PgUp/PgDn Home/End Ctrl+A Ctrl+C Del Enter F5",
        TreeHint => "Tree: ↑↓ ← → Space Enter F5",

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
        TreeNotLoaded => "Tree not loaded — press \"Refresh branch\".",
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
        TrustServerCertCheck => "Trust server certificate",
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
        TestOpcClient => "Test OPC client",
        RustEgui2026 => "Rust + egui, 2026",
        RewrittenFrom => "Rewritten from EleSy OPC Test Client 2.1.0.139 (Delphi, 2012).",
        OriginalProto => {
            "Original: OPC DA 2.0, COM interfaces IOPCServer / IOPCItemMgt / IOPCSyncIO / IOPCBrowseServerAddressSpace."
        }
        ReverseInfo => {
            "The reverse-engineering version was recovered from PE resources and the disassembler."
        }
        SupportedLabel => "Supported:",
        AboutDaLine => "• OPC DA 2.0 via COM (local and over DCOM);",
        AboutUaLine => "• OPC UA via async-opcua 0.19 (SecurityPolicy None, anonymous / username);",
        AboutTreeLine => "• address space tree, signal table, read/write, journal;",
        AboutSettingsLine => "• settings saved to %APPDATA%\\opcda-egui\\settings.json.",
        ConnectedServers => "Connected servers:",
        NoConnectedServers => "No connected servers.",
        VendorNotAvail => {
            "Vendor/version/status (GetStatus) is not exposed by the current BFF gateway; only the endpoint and the status text are available."
        }
        LimitationsLabel => "Limitations of this version:",
        LimitScriptLine => {
            "• the script interpreter is not implemented (editor and .stx files are kept);"
        }
        LimitQualityLine => {
            "• quality writes are not part of the OPC DA standard (proprietary EleSy.OPCIntercom in the original);"
        }
        LimitDcomLine => "• DCOM/OPC UA security is configured via the OS/server.",

        // ---- Message/confirm ----
        ConfirmTitle => "Confirmation",
        Yes => "Yes",
        No => "No",

        // ---- App dialog content ----
        WriteError => "Write error",
        WriteFailedName => "Failed to write a value to signal \"{0}\".",
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
        AppStarted => "OPC DA/UA Test Client (Rust + egui) started",
        OriginNote => "Original: EleSy OPC Test Client 2.1.0.139 (OPC DA 2.0)",
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
        WriteFailed => "Failed to write \"{0}\": {1}",
        WriteDoneSummary => "Write done: {0} of {1} successful",
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
        CsvServerAmbiguous => "已跳过 CSV 行：服务器名“{0}”匹配多个连接，请指定准确名称",
        CsvServerEmpty => "已跳过 CSV 行：信号“{0}”未指定服务器",
        CjkFontMissing => "未找到可显示中文的字体（msyh.ttc、simsun.ttc）：界面文字可能显示为方块",
        CjkFontLoaded => "中文字体：{0}",
        ResolvingName => "正在解析节点…",
        SettingsReadFailed => "无法读取设置文件 {0}：{1}。不会覆盖该文件",
        SettingsParseFailed => "设置文件 {0} 已损坏（{1}）。副本已保存到 {2}。使用默认值",
        SettingsParseNoBackup => {
            "设置文件 {0} 已损坏（{1}），且无法备份（{2}）。不会覆盖该文件，使用默认值"
        }
        SettingsSaveFailed => "无法保存设置 {0}：{1}",
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
        HintRefreshF5 => "F5 — 刷新分支",
        HintLoadCsv => "从 CSV 文件加载信号",
        HintSaveCsv => "将信号保存到 CSV 文件",
        HintEnterTree => "在树中按 Enter",
        HintAddChildren => "添加该分支的所有信号",
        HintDelSelected => "Delete — 删除所选",
        HintEnterF2 => "Enter / F2 — 更改值",
        HintCtrlA => "Ctrl+A — 全选",
        HintEditColumns => "配置列及其顺序",

        // ---- Status bar ----
        Connected => "已连接",
        Connecting => "连接中",
        NoLink => "无连接",
        NoActiveServer => "无活动服务器",
        SignalsCount => "信号: {0} | 已选: {1}",
        Writing => "写入中...",
        WrittenSummary => "已写入: {0} / {1}",
        UpdatedAt => "更新于 {0}",
        RowsHint => "表格: ↑↓ Shift+PgUp/PgDn Home/End Ctrl+A Ctrl+C Delete Enter F5",
        TreeHint => "树: ↑↓ ← → 空格 Enter F5",

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
        TreeNotLoaded => "树未加载 — 请按 \"刷新分支\"。",
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
        TrustServerCertCheck => "信任服务器证书",
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
        TestOpcClient => "测试 OPC 客户端",
        RustEgui2026 => "Rust + egui, 2026",
        RewrittenFrom => "从 EleSy OPC Test Client 2.1.0.139 (Delphi, 2012) 重写。",
        OriginalProto => {
            "原始版本: OPC DA 2.0，COM 接口 IOPCServer / IOPCItemMgt / IOPCSyncIO / IOPCBrowseServerAddressSpace。"
        }
        ReverseInfo => "逆向工程版本从 PE 资源和反汇编中恢复。",
        SupportedLabel => "支持:",
        AboutDaLine => "• OPC DA 2.0 通过 COM (本地及 DCOM);",
        AboutUaLine => "• OPC UA 通过 async-opcua 0.19 (SecurityPolicy None，匿名 / 用户名);",
        AboutTreeLine => "• 地址空间树、信号表、读写、日志;",
        AboutSettingsLine => "• 设置保存到 %APPDATA%\\opcda-egui\\settings.json。",
        ConnectedServers => "已连接的服务器:",
        NoConnectedServers => "无已连接的服务器。",
        VendorNotAvail => {
            "当前 BFF 网关不提供供应商/版本/状态 (GetStatus)；仅提供连接点和状态文本。"
        }
        LimitationsLabel => "本版本的局限:",
        LimitScriptLine => "• 脚本解释器未实现 (保留编辑器及 .stx 文件);",
        LimitQualityLine => "• OPC DA 标准不提供质量写入 (原版为专有的 EleSy.OPCIntercom);",
        LimitDcomLine => "• DCOM/OPC UA 安全由操作系统/服务器配置。",

        // ---- Message/confirm ----
        ConfirmTitle => "确认",
        Yes => "是",
        No => "否",

        // ---- App dialog content ----
        WriteError => "写入错误",
        WriteFailedName => "无法将值写入信号 \"{0}\"。",
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
        AppStarted => "OPC DA/UA 测试客户端 (Rust + egui) 已启动",
        OriginNote => "原始版本: EleSy OPC Test Client 2.1.0.139 (OPC DA 2.0)",
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
        WriteFailed => "无法写入 \"{0}\": {1}",
        WriteDoneSummary => "写入完成: {0} / {1} 成功",
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

#[cfg(test)]
mod tests {
    use super::*;

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
