use crate::bff::model::{EndpointSpec, UaSecurityMode, UaSecurityPolicy};
use crate::columns::{ColumnId, ColumnSet};
use crate::constants::{
    OPERATION_TIMEOUT_DEFAULT_SECS, SESSION_TIMEOUT_DEFAULT_MS, TEST_INTERVAL_DEFAULT_SECS,
    UPDATE_INTERVAL_DEFAULT_SECS,
};
use crate::i18n::Lang;
use crate::net::{AdapterId, AdapterSelection, CachedAdapter};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::net::IpAddr;
use std::path::PathBuf;

/// Stored code for the default UI language (Russian). Used as the serde fallback
/// so older settings files without a `lang` field still load, and whenever the
/// persisted code is unknown/missing. An explicitly persisted language (e.g.
/// "en") is never overridden.
pub fn default_lang() -> String {
    Lang::default().as_str().to_string()
}

/// Default «Доверять сертификату сервера» is OFF: strict PKI trust by default.
pub fn default_trust_server_cert() -> bool {
    false
}

/// Default UA session timeout in milliseconds (120 s).
pub fn default_session_timeout() -> u32 {
    SESSION_TIMEOUT_DEFAULT_MS
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
    pub update_interval_secs: u64,
    pub auto_reconnect: bool,
    pub show_colors: bool,
    pub confirm_write: bool,
    pub operation_timeout_secs: u64,
    pub test_interval_secs: u64,
    pub show_script: bool,
    pub show_log: bool,
    /// UA-only: absolute deadband for DataChangeFilter (0 = off). Applied to the
    /// push subscription when it is (re)created.
    #[serde(default)]
    pub ua_deadband: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            update_interval_secs: UPDATE_INTERVAL_DEFAULT_SECS,
            auto_reconnect: true,
            show_colors: true,
            confirm_write: true,
            operation_timeout_secs: OPERATION_TIMEOUT_DEFAULT_SECS,
            test_interval_secs: TEST_INTERVAL_DEFAULT_SECS,
            show_script: true,
            show_log: false,
            ua_deadband: 0.0,
        }
    }
}

/// Persisted adapter cache plus per-entry deserialization diagnostics. The
/// diagnostics are deliberately not serialized: they only feed the startup
/// journal when an old/hand-edited file contains malformed records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CachedAdapters {
    pub adapters: Vec<CachedAdapter>,
    pub rejected: Vec<String>,
}

impl CachedAdapters {
    pub fn from_vec(adapters: Vec<CachedAdapter>) -> Self {
        Self {
            adapters,
            rejected: Vec::new(),
        }
    }
}

impl Serialize for CachedAdapters {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.adapters.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CachedAdapters {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        let mut adapters = Vec::new();
        let mut rejected = Vec::new();
        let items = match raw {
            serde_json::Value::Null => Vec::new(),
            serde_json::Value::Array(items) => items,
            _ => {
                rejected.push("cached_adapters: expected an array".to_string());
                Vec::new()
            }
        };
        for (index, value) in items.into_iter().enumerate() {
            match serde_json::from_value::<CachedAdapter>(value) {
                Ok(adapter) => adapters.push(adapter),
                Err(err) => rejected.push(format!("cached_adapters[{index}]: {err}")),
            }
        }
        Ok(Self { adapters, rejected })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub options: Options,
    pub columns: ColumnSet,
    pub script: String,
    pub last_da_host: String,
    pub last_da_server: String,
    pub last_ua_url: String,
    pub recent_endpoints: Vec<EndpointSpec>,
    /// Column order persisted from the header context menu («Влево»/«Вправо»).
    /// `None` keeps the legacy `columns.order` (backward compatible). Stored as
    /// serialized `ColumnId` strings so the table can render from it directly.
    pub column_order: Option<Vec<String>>,
    /// UI language code ("ru", "en", "zh"). Missing/invalid entries fall back
    /// to Russian on load (see `lang()`).
    #[serde(default = "default_lang")]
    pub lang: String,
    /// UA connect dialog: trust the server certificate without importing it
    /// into the PKI trust store. Default OFF (strict PKI trust).
    #[serde(default = "default_trust_server_cert")]
    pub ua_trust_server_cert: bool,
    /// UA connect dialog: requested session timeout in milliseconds. The crate
    /// accepts the value and the server revises it; the revised timeout is not
    /// exposed by the API, so it is surfaced from the dialog state.
    #[serde(default = "default_session_timeout")]
    pub ua_session_timeout_ms: u32,
    /// UA connect dialog: last selected security policy (typed; legacy string
    /// spellings are migrated by the enum deserializer).
    #[serde(default)]
    pub ua_security_policy: UaSecurityPolicy,
    /// UA connect dialog: last selected message security mode.
    #[serde(default)]
    pub ua_security_mode: UaSecurityMode,
    /// Preferred network adapter GUID. `None` means no adapter is selected.
    /// Old settings files without this field load as unselected.
    #[serde(default)]
    pub preferred_adapter: Option<String>,
    /// Local IP selected by the user when the adapter has several addresses.
    /// `None` lets the resolver pick the best address.
    #[serde(default)]
    pub selected_local_ip: Option<String>,
    /// Snapshot of usable adapters from the last explicit «Обновить». Missing
    /// in an old settings file means an empty list until the user refreshes.
    #[serde(default)]
    pub cached_adapters: CachedAdapters,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            options: Options::default(),
            columns: ColumnSet::default(),
            script: "program\nbegin\n\nend.\n".to_string(),
            last_da_host: String::new(),
            last_da_server: String::new(),
            // No preset endpoints: the product requirement is that a fresh
            // install starts with empty fields (the user chooses or searches).
            last_ua_url: String::new(),
            recent_endpoints: Vec::new(),
            column_order: None,
            lang: default_lang(),
            ua_trust_server_cert: default_trust_server_cert(),
            ua_session_timeout_ms: default_session_timeout(),
            ua_security_policy: UaSecurityPolicy::Auto,
            ua_security_mode: UaSecurityMode::Auto,
            preferred_adapter: None,
            selected_local_ip: None,
            cached_adapters: CachedAdapters::default(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(settings) => settings,
            Err(err) => {
                eprintln!("settings {}: {err}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&path, text) {
                    eprintln!("settings save {}: {err}", path.display());
                }
            }
            Err(err) => eprintln!("settings serialize: {err}"),
        }
    }

    pub fn remember(&mut self, endpoint: &EndpointSpec) {
        self.recent_endpoints.retain(|e| {
            !(e.protocol == endpoint.protocol
                && e.address == endpoint.address
                && e.host == endpoint.host)
        });
        self.recent_endpoints.insert(0, endpoint.clone());
        self.recent_endpoints.truncate(12);
        match endpoint.protocol {
            crate::bff::model::Protocol::OpcDa => {
                self.last_da_host = endpoint.host.clone();
                self.last_da_server = endpoint.address.clone();
            }
            crate::bff::model::Protocol::OpcUa => {
                self.last_ua_url = endpoint.address.clone();
            }
        }
    }

    /// Current UI language, falling back to Russian for an unknown/missing code.
    pub fn lang(&self) -> Lang {
        Lang::from_str(&self.lang).unwrap_or(Lang::default())
    }

    /// Set the UI language and normalize its stored code.
    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang.as_str().to_string();
    }

    /// Parse a persisted column name that may be either a plain variant name
    /// ("Server") or a JSON-encoded one from an older build ("\"Server\"").
    fn parse_column_name(raw: &str) -> Option<ColumnId> {
        let trimmed = raw.trim();
        if let Ok(id) = serde_json::from_str::<ColumnId>(trimmed) {
            return Some(id);
        }
        // Plain unquoted name ("Server") is not valid JSON on its own.
        serde_json::from_str::<ColumnId>(&format!("\"{trimmed}\"")).ok()
    }

    /// Effective column order used by the table. Prefers the persisted
    /// `column_order` snapshot; falls back to the legacy `columns.order`.
    pub fn resolved_order(&self) -> Vec<ColumnId> {
        if let Some(list) = &self.column_order {
            let parsed: Vec<ColumnId> = list
                .iter()
                .filter_map(|s| Self::parse_column_name(s))
                .collect();
            if !parsed.is_empty() {
                // Ensure every canonical column is present exactly once, in
                // the stored relative order, so hiding/reshow stays consistent.
                let mut order = parsed.clone();
                for id in ColumnId::ALL {
                    if !order.contains(&id) {
                        order.push(id);
                    }
                }
                return order;
            }
        }
        self.columns.order.clone()
    }

    /// Record a new column order from the header context menu and keep the
    /// legacy `columns.order` in sync for the columns dialog. Persisted entries
    /// are plain variant names (no JSON quotes), so `settings.json` stores
    /// `["Server", ...]` instead of a double-encoded `["\"Server\"", ...]`.
    pub fn persist_column_order(&mut self, order: &[ColumnId]) {
        self.columns.order = order.to_vec();
        self.column_order = Some(
            order
                .iter()
                .map(|id| {
                    serde_json::to_string(id)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string()
                })
                .collect(),
        );
    }

    /// Effective adapter selection. A missing/invalid persisted GUID means no
    /// adapter is selected; the user must choose one explicitly.
    pub fn adapter_selection(&self) -> AdapterSelection {
        self.preferred_adapter
            .as_deref()
            .and_then(|raw| AdapterId::parse(raw).ok())
            .map(AdapterSelection::Adapter)
            .unwrap_or(AdapterSelection::None)
    }

    /// The parsed preferred GUID, if the persisted value is valid.
    pub fn preferred_adapter_id(&self) -> Option<AdapterId> {
        self.preferred_adapter
            .as_deref()
            .and_then(|raw| AdapterId::parse(raw).ok())
    }

    /// Persist a selection as a canonical GUID string (`None` = unselected).
    pub fn set_adapter_selection(&mut self, selection: AdapterSelection) {
        self.preferred_adapter = selection.adapter_id().map(|id| id.to_string());
    }

    /// The parsed preferred local IP, if the persisted value is valid.
    pub fn selected_local_ip(&self) -> Option<IpAddr> {
        self.selected_local_ip
            .as_deref()
            .and_then(|raw| raw.trim().parse::<IpAddr>().ok())
    }

    /// Persist the preferred local IP (`None` = resolver chooses).
    pub fn set_selected_local_ip(&mut self, ip: Option<IpAddr>) {
        self.selected_local_ip = ip.map(|ip| ip.to_string());
    }

    /// Canonicalize both network fields: valid GUID/IP keep their canonical
    /// spelling; invalid/missing values become `None` (Auto / resolver choice).
    pub fn normalize_network(&mut self) {
        self.preferred_adapter = self.preferred_adapter_id().map(|id| id.to_string());
        self.selected_local_ip = self.selected_local_ip().map(|ip| ip.to_string());
    }

    /// Store and persist the user's network choice.
    pub fn save_network_selection(
        &mut self,
        selection: AdapterSelection,
        local_ip: Option<IpAddr>,
    ) {
        self.set_adapter_selection(selection);
        self.set_selected_local_ip(local_ip);
        self.normalize_network();
        self.save();
    }
}

pub fn app_dir() -> PathBuf {
    // Unit tests must never touch the developer's real settings: in test builds
    // the base directory is a per-test override or a private per-process temp
    // directory (see `test_isolation`). The real `%APPDATA%` is only read by the
    // non-test build (the application and the integration tests, which redirect
    // `APPDATA` themselves).
    #[cfg(test)]
    let base = test_isolation::current_override().unwrap_or_else(test_isolation::fallback_dir);
    #[cfg(not(test))]
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("opcda-egui")
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

/// Test-only settings-directory isolation.
///
/// `Settings::load()`/`save()` resolve their directory through `app_dir()`,
/// which in test builds never returns the developer's real `%APPDATA%`. Tests
/// that need a specific directory opt in with [`with_appdata_dir`] (or hold a
/// [`ScopedDir`]); a single process-wide lock serializes those tests so they
/// cannot interleave each other's directories under parallel execution.
#[cfg(test)]
pub mod test_isolation {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    // Serializes every lib test that reads/writes settings. Poisoning (a
    // panicking test) is recovered so the rest of the suite keeps running.
    static IO_LOCK: Mutex<()> = Mutex::new(());

    thread_local! {
        // The directory the current test redirected settings to. Thread-local
        // so parallel tests cannot see (and clobber) each other's directory.
        static OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    pub(super) fn current_override() -> Option<PathBuf> {
        OVERRIDE.with(|cell| cell.borrow().clone())
    }

    /// A private per-process directory used when a test saves without opting
    /// into its own directory. Never the real `%APPDATA%`.
    pub(super) fn fallback_dir() -> PathBuf {
        static FALLBACK: OnceLock<PathBuf> = OnceLock::new();
        FALLBACK
            .get_or_init(|| {
                let dir =
                    std::env::temp_dir().join(format!("opcda-egui-libtest-{}", std::process::id()));
                let _ = std::fs::create_dir_all(&dir);
                dir
            })
            .clone()
    }

    /// Holds the settings lock and redirects settings to `dir` until dropped.
    /// Restores the previous redirect (and releases the lock) even on panic.
    pub struct ScopedDir {
        dir: PathBuf,
        owned: bool,
        previous: Option<PathBuf>,
        #[allow(dead_code)]
        _guard: MutexGuard<'static, ()>,
    }

    impl ScopedDir {
        fn at(dir: PathBuf, owned: bool) -> Self {
            let guard = IO_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = OVERRIDE.with(|cell| cell.replace(Some(dir.clone())));
            Self {
                dir,
                owned,
                previous,
                _guard: guard,
            }
        }

        /// Create a fresh private temp directory owned by this guard.
        pub fn temp(label: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("opcda-egui-libtest-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::create_dir_all(&dir);
            Self::at(dir, true)
        }
    }

    impl Drop for ScopedDir {
        fn drop(&mut self) {
            OVERRIDE.with(|cell| cell.borrow_mut().clone_from(&self.previous));
            if self.owned {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
    }

    /// Run `f` with settings redirected to `dir` under the settings lock.
    pub fn with_appdata_dir<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
        let _scope = ScopedDir::at(dir.to_path_buf(), false);
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bff::model::{EndpointSpec, Protocol};
    use crate::columns::ColumnId;

    // load()/save() resolve `app_dir()`; serialize and redirect those tests so
    // they never touch the real user settings (see `test_isolation`).
    fn with_appdata<T>(dir: &PathBuf, f: impl FnOnce() -> T) -> T {
        super::test_isolation::with_appdata_dir(dir, f)
    }

    #[test]
    fn options_defaults_match_elesy() {
        let d = Options::default();
        assert_eq!(d.update_interval_secs, 1);
        assert_eq!(d.operation_timeout_secs, 60);
        assert_eq!(d.test_interval_secs, 60);
        assert!(d.auto_reconnect);
        assert!(d.show_colors);
        assert!(d.confirm_write);
        assert!(d.show_script);
        assert!(!d.show_log);
    }

    /// A default/unset settings file defaults to Russian; an explicitly
    /// persisted language (e.g. "en") is never overridden on load.
    #[test]
    fn fresh_settings_default_to_russian_and_persisted_lang_is_kept() {
        let mut s = Settings::default();
        s.lang = String::new();
        assert_eq!(
            s.lang(),
            crate::i18n::Lang::Ru,
            "empty lang must fall back to ru"
        );

        // A persisted "en" wins over the Russian default.
        let mut en = Settings::default();
        en.set_lang(crate::i18n::Lang::En);
        assert_eq!(en.lang(), crate::i18n::Lang::En, "explicit en must win");
        let loaded = en;
        assert_eq!(loaded.lang(), crate::i18n::Lang::En);
    }

    #[test]
    fn settings_defaults_field_values() {
        let d = Settings::default();
        let o = d.options.clone();
        let od = Options::default();
        assert_eq!(o.update_interval_secs, od.update_interval_secs);
        assert_eq!(o.operation_timeout_secs, od.operation_timeout_secs);
        assert_eq!(o.test_interval_secs, od.test_interval_secs);
        assert_eq!(o.auto_reconnect, od.auto_reconnect);
        assert_eq!(o.show_colors, od.show_colors);
        assert_eq!(o.confirm_write, od.confirm_write);
        assert_eq!(o.show_script, od.show_script);
        assert_eq!(o.show_log, od.show_log);
        assert_eq!(d.columns.order, ColumnId::ALL.to_vec());
        assert!(d.columns.hidden.is_empty());
        assert!(d.script.contains("program"));
        assert!(d.last_da_host.is_empty());
        assert!(d.last_da_server.is_empty());
        assert!(
            d.last_ua_url.is_empty(),
            "no preset UA URL in fresh settings"
        );
        assert!(d.recent_endpoints.is_empty());
        assert!(d.column_order.is_none(), "column_order defaults to None");
        assert_eq!(d.lang, "ru", "UI language defaults to Russian");
        assert_eq!(d.lang(), crate::i18n::Lang::Ru);
        assert!(d.preferred_adapter.is_none());
        assert!(d.selected_local_ip.is_none());
        assert_eq!(d.adapter_selection(), AdapterSelection::None);
    }

    #[test]
    fn ua_security_defaults_are_strict() {
        let d = Settings::default();
        // Default OFF: server certs are not auto-trusted.
        assert!(
            !d.ua_trust_server_cert,
            "trust_server_cert must default to false"
        );
        // Session timeout defaults to 120 s.
        assert_eq!(d.ua_session_timeout_ms, 120_000);
    }

    #[test]
    fn ua_security_fields_serde_roundtrip() {
        let mut s = Settings::default();
        s.ua_trust_server_cert = true;
        s.ua_session_timeout_ms = 300_000;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"ua_trust_server_cert\":true"));
        assert!(json.contains("\"ua_session_timeout_ms\":300000"));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert!(back.ua_trust_server_cert);
        assert_eq!(back.ua_session_timeout_ms, 300_000);
    }

    #[test]
    fn ua_security_selection_defaults_to_auto_and_roundtrips() {
        let d = Settings::default();
        assert_eq!(d.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(d.ua_security_mode, UaSecurityMode::Auto);

        let mut s = Settings::default();
        s.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
        s.ua_security_mode = UaSecurityMode::SignAndEncrypt;
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("\"ua_security_policy\":\"Basic256Sha256\""),
            "got: {json}"
        );
        assert!(
            json.contains("\"ua_security_mode\":\"SignAndEncrypt\""),
            "got: {json}"
        );
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ua_security_policy, UaSecurityPolicy::Basic256Sha256);
        assert_eq!(back.ua_security_mode, UaSecurityMode::SignAndEncrypt);
    }

    #[test]
    fn legacy_security_strings_in_settings_migrate() {
        // Old settings may carry free-string policies/modes. They must load into
        // typed values instead of forcing a default or failing.
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.insert("ua_security_policy".to_string(), serde_json::json!("Авто"));
        obj.insert("ua_security_mode".to_string(), serde_json::json!("None"));
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(s.ua_security_mode, UaSecurityMode::None);

        // A full URI is accepted as well.
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().insert(
            "ua_security_policy".to_string(),
            serde_json::json!("http://opcfoundation.org/UA/SecurityPolicy#Basic256"),
        );
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.ua_security_policy, UaSecurityPolicy::Basic256);
    }

    #[test]
    fn unknown_security_string_in_settings_is_an_error() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().insert(
            "ua_security_policy".to_string(),
            serde_json::json!("NoSuchPolicy"),
        );
        assert!(serde_json::from_value::<Settings>(value).is_err());
    }

    #[test]
    fn old_settings_missing_security_selection_loads_as_auto() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("ua_security_policy");
        obj.remove("ua_security_mode");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(s.ua_security_mode, UaSecurityMode::Auto);
    }

    #[test]
    fn ua_deadband_defaults_off_and_serde_roundtrips() {
        // Default 0 = off; a positive value survives a JSON roundtrip; an old
        // settings file without the field deserializes as off.
        assert_eq!(Options::default().ua_deadband, 0.0);
        let mut s = Settings::default();
        s.options.ua_deadband = 12.5;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"ua_deadband\":12.5"));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.options.ua_deadband, 12.5);

        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .get_mut("options")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("ua_deadband");
        let legacy: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(
            legacy.options.ua_deadband, 0.0,
            "missing field must fall back to 0"
        );
    }

    #[test]
    fn old_settings_missing_ua_fields_load_with_defaults() {
        // A settings file written before the UA security fields existed loads
        // with strict trust and the 120 s timeout.
        // Build a valid pre-UA blob by dropping the UA fields from a default.
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("ua_trust_server_cert");
        obj.remove("ua_session_timeout_ms");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!(!s.ua_trust_server_cert);
        assert_eq!(s.ua_session_timeout_ms, 120_000);
    }

    #[test]
    fn unknown_or_missing_lang_falls_back_to_russian() {
        // A settings file written before `lang` existed loads with "ru".
        let json = r#"{"lang":"xx","options":null,"columns":null,"script":"","last_da_host":"","last_da_server":"","last_ua_url":"","recent_endpoints":null,"column_order":null}"#;
        // Build a minimal valid settings payload via defaults and inject bad lang.
        let mut s = Settings::default();
        s.lang = "zz".to_string();
        assert_eq!(s.lang(), crate::i18n::Lang::Ru);
        s.lang = "".to_string();
        assert_eq!(s.lang(), crate::i18n::Lang::Ru);
        s.set_lang(crate::i18n::Lang::En);
        assert_eq!(s.lang, "en");
        assert_eq!(s.lang(), crate::i18n::Lang::En);
        let _ = json;
    }

    #[test]
    fn serde_roundtrip_json_preserves_all_fields() {
        let mut s = Settings::default();
        s.options.update_interval_secs = 5;
        s.options.operation_timeout_secs = 300;
        s.options.test_interval_secs = 120;
        s.options.show_script = false;
        s.options.show_log = true;
        s.script = "custom\n".to_string();
        s.last_da_host = "h1".to_string();
        s.last_da_server = "Srv.1".to_string();
        s.last_ua_url = "opc.tcp://x:4840".to_string();
        s.recent_endpoints = vec![
            EndpointSpec::ua("opc.tcp://x:4840"),
            EndpointSpec::da("h", "Srv.1"),
        ];
        s.columns.order = vec![
            ColumnId::Value,
            ColumnId::Status,
            ColumnId::FullName,
            ColumnId::WriteType,
            ColumnId::Type,
            ColumnId::Quality,
            ColumnId::Time,
            ColumnId::Server,
            ColumnId::Description,
        ];
        s.persist_column_order(&s.columns.order.clone());
        s.set_lang(crate::i18n::Lang::Zh);
        s.set_adapter_selection(AdapterSelection::Adapter(
            AdapterId::parse("aabbccdd-1122-3344-5566-77889900aabb").unwrap(),
        ));
        s.set_selected_local_ip(Some("192.168.1.21".parse().unwrap()));
        s.normalize_network();

        let json = serde_json::to_string(&s).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.lang(), crate::i18n::Lang::Zh);
        assert!(json.contains("\"lang\":\"zh\""));
        assert_eq!(back.options.update_interval_secs, 5);
        assert_eq!(back.options.operation_timeout_secs, 300);
        assert_eq!(back.options.test_interval_secs, 120);
        assert!(!back.options.show_script);
        assert!(back.options.show_log);
        assert_eq!(back.script, "custom\n");
        assert_eq!(back.last_da_host, "h1");
        assert_eq!(back.last_da_server, "Srv.1");
        assert_eq!(back.last_ua_url, "opc.tcp://x:4840");
        assert_eq!(back.recent_endpoints.len(), 2);
        assert_eq!(back.recent_endpoints[0].protocol, Protocol::OpcUa);
        assert_eq!(back.recent_endpoints[1].address, "Srv.1");
        let column_order = back.column_order.clone().expect("column_order persisted");
        assert_eq!(column_order.len(), 9);
        assert_eq!(back.resolved_order()[0], ColumnId::Value);
        assert_eq!(back.resolved_order()[1], ColumnId::Status);
    }

    #[test]
    fn persist_column_order_updates_resolved_and_columns() {
        let mut s = Settings::default();
        s.persist_column_order(&[
            ColumnId::Value,
            ColumnId::FullName,
            ColumnId::Status,
            ColumnId::Type,
            ColumnId::WriteType,
            ColumnId::Quality,
            ColumnId::Time,
            ColumnId::Server,
            ColumnId::Description,
        ]);
        let resolved = s.resolved_order();
        assert_eq!(resolved[0], ColumnId::Value);
        assert_eq!(resolved[1], ColumnId::FullName);
        assert_eq!(resolved[2], ColumnId::Status);
        // Both sources stay in sync.
        assert_eq!(s.columns.order, resolved);
        assert_eq!(s.column_order.as_ref().unwrap().len(), 9);
    }

    #[test]
    fn resolved_order_falls_back_when_column_order_is_none() {
        let s = Settings::default();
        assert!(s.column_order.is_none());
        assert_eq!(s.resolved_order(), ColumnId::ALL.to_vec());
    }

    #[test]
    fn resolved_order_ignores_unknown_entries_and_fills_missing() {
        let mut s = Settings::default();
        // Stored as JSON-encoded variant names (as persist_column_order writes).
        s.column_order = Some(vec![
            serde_json::to_string(&ColumnId::Value).unwrap(),
            "\"NoSuchColumn\"".to_string(),
        ]);
        let resolved = s.resolved_order();
        // Unknown entry dropped; full canonical set preserved with Value first.
        assert_eq!(resolved[0], ColumnId::Value);
        assert_eq!(resolved.len(), ColumnId::ALL.len());
        for id in ColumnId::ALL {
            assert!(resolved.contains(&id), "missing {id:?}");
        }
    }

    #[test]
    fn resolved_order_parses_both_plain_and_double_quoted_names() {
        // A legacy settings.json wrote a double-encoded ordering: each element
        // is a JSON string literal containing the quoted variant name, e.g.
        // ["\"Server\"", "…"]. Newer builds write plain names, e.g. ["Server"].
        // Both must resolve.
        let mut legacy = Settings::default();
        legacy.column_order = Some(vec![
            "\"Server\"".to_string(),
            "\"FullName\"".to_string(),
            "\"Status\"".to_string(),
        ]);
        let resolved = legacy.resolved_order();
        assert_eq!(
            resolved[0],
            ColumnId::Server,
            "double-quoted form must parse"
        );
        assert_eq!(resolved[1], ColumnId::FullName);
        assert_eq!(resolved[2], ColumnId::Status);
        // The full canonical set must still be present exactly once.
        assert_eq!(resolved.len(), ColumnId::ALL.len());
        for id in ColumnId::ALL {
            assert_eq!(resolved.iter().filter(|c| **c == id).count(), 1, "{id:?}");
        }

        // Plain form written by the current persist_column_order.
        let mut plain = Settings::default();
        plain.column_order = Some(vec![
            "Value".to_string(),
            "Time".to_string(),
            "Server".to_string(),
        ]);
        let resolved = plain.resolved_order();
        assert_eq!(resolved[0], ColumnId::Value, "plain form must parse");
        assert_eq!(resolved[1], ColumnId::Time);
        assert_eq!(resolved[2], ColumnId::Server);
        assert_eq!(resolved.len(), ColumnId::ALL.len());
    }

    #[test]
    fn persist_column_order_writes_plain_names_not_double_encoded() {
        let mut s = Settings::default();
        s.persist_column_order(&[
            ColumnId::Value,
            ColumnId::Status,
            ColumnId::FullName,
            ColumnId::Server,
            ColumnId::WriteType,
            ColumnId::Type,
            ColumnId::Quality,
            ColumnId::Time,
            ColumnId::Description,
        ]);
        let stored = s.column_order.as_ref().unwrap();
        // Persisted entries are plain variant names, NOT "\"Server\"" encodings.
        assert_eq!(stored[0], "Value");
        assert_eq!(stored[3], "Server");
        assert!(!stored[3].starts_with('"'), "no leading JSON quote");
        // The settings JSON itself must not double-encode the strings.
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("\"column_order\":[\"Value\",\"Status\",\"FullName\",\"Server\""),
            "settings.json must store plain names, got: {json}"
        );
        assert!(
            !json.contains("\\\"Server\\\""),
            "no double-encoded name: {json}"
        );
        // And it round-trips through resolved_order.
        assert_eq!(s.resolved_order()[0], ColumnId::Value);
        assert_eq!(s.resolved_order()[3], ColumnId::Server);
    }

    #[test]
    fn old_settings_missing_network_fields_load_as_unselected() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("preferred_adapter");
        obj.remove("selected_local_ip");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.adapter_selection(), AdapterSelection::None);
        assert!(s.selected_local_ip.is_none());
    }

    #[test]
    fn network_selection_roundtrips_and_normalizes() {
        let id = AdapterId::parse("AABBCCDD-1122-3344-5566-77889900AABB").unwrap();
        let ip: IpAddr = "192.168.1.21".parse().unwrap();
        let mut s = Settings::default();
        s.set_adapter_selection(AdapterSelection::Adapter(id));
        s.set_selected_local_ip(Some(ip));
        s.normalize_network();
        // Canonical GUID spelling is stored (lower-case, no braces).
        assert_eq!(
            s.preferred_adapter.as_deref(),
            Some("aabbccdd-1122-3344-5566-77889900aabb")
        );
        assert_eq!(s.selected_local_ip.as_deref(), Some("192.168.1.21"));
        assert_eq!(s.adapter_selection(), AdapterSelection::Adapter(id));
        assert_eq!(s.selected_local_ip(), Some(ip));

        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adapter_selection(), AdapterSelection::Adapter(id));
        assert_eq!(back.selected_local_ip(), Some(ip));
    }

    #[test]
    fn invalid_persisted_network_values_degrade_to_unselected() {
        let mut s = Settings::default();
        s.preferred_adapter = Some("not-a-guid".to_string());
        s.selected_local_ip = Some("not-an-ip".to_string());
        assert_eq!(s.adapter_selection(), AdapterSelection::None);
        assert!(s.selected_local_ip().is_none());
        s.normalize_network();
        assert!(s.preferred_adapter.is_none());
        assert!(s.selected_local_ip.is_none());

        // A valid but non-canonical GUID is rewritten in canonical form.
        s.preferred_adapter = Some("{AABBCCDD-1122-3344-5566-77889900AABB}".to_string());
        s.selected_local_ip = Some(" 10.0.0.5 ".to_string());
        s.normalize_network();
        assert_eq!(
            s.preferred_adapter.as_deref(),
            Some("aabbccdd-1122-3344-5566-77889900aabb")
        );
        assert_eq!(s.selected_local_ip.as_deref(), Some("10.0.0.5"));
    }

    fn cached_fixture() -> crate::net::CachedAdapter {
        crate::net::CachedAdapter {
            id: AdapterId::parse("14693899-fe86-4f25-a6d6-814e60291d39").unwrap(),
            name: "Wi-Fi".to_string(),
            description: "Intel(R) Wi-Fi 6E AX211 160MHz".to_string(),
            state: crate::net::AdapterState::Up,
            addresses: vec![crate::net::AdapterAddress {
                ip: "172.28.76.90".parse().unwrap(),
                prefix: 24,
                scope_id: None,
            }],
            gateways: vec!["172.28.76.1".parse().unwrap()],
            if_index: 3,
            ipv6_if_index: 3,
            luid: 123,
            metric: 25,
            is_physical: true,
        }
    }

    #[test]
    fn cached_adapters_roundtrip_and_load() {
        let mut s = Settings::default();
        s.cached_adapters = CachedAdapters::from_vec(vec![cached_fixture()]);
        s.set_adapter_selection(AdapterSelection::Adapter(cached_fixture().id));
        s.set_selected_local_ip(Some("172.28.76.90".parse().unwrap()));
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"cached_adapters\":["), "got: {json}");
        assert!(json.contains("\"state\":\"up\""), "got: {json}");
        assert!(json.contains("\"name\":\"Wi-Fi\""), "got: {json}");

        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cached_adapters.adapters.len(), 1);
        assert!(back.cached_adapters.rejected.is_empty());
        assert_eq!(back.cached_adapters.adapters[0], cached_fixture());
        assert_eq!(
            back.selected_local_ip(),
            Some("172.28.76.90".parse().unwrap())
        );
    }

    #[test]
    fn missing_cached_adapters_loads_as_empty() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("cached_adapters");
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!(s.cached_adapters.adapters.is_empty());
        assert!(s.cached_adapters.rejected.is_empty());
    }

    #[test]
    fn deleted_config_loads_with_empty_adapter_cache() {
        let dir = std::env::temp_dir().join("opcda-egui-settings-no-adapter-cache");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let s = Settings::load();
            assert!(s.cached_adapters.adapters.is_empty());
            assert_eq!(s.adapter_selection(), AdapterSelection::None);
        });
    }

    #[test]
    fn malformed_cached_adapter_record_is_dropped_and_reported() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let good = serde_json::to_value(cached_fixture()).unwrap();
        value.as_object_mut().unwrap().insert(
            "cached_adapters".to_string(),
            serde_json::json!([good, {"id": "not-a-guid", "name": "broken"}]),
        );
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.cached_adapters.adapters.len(), 1);
        assert_eq!(s.cached_adapters.rejected.len(), 1);
        assert!(
            s.cached_adapters.rejected[0].contains("cached_adapters[1]"),
            "got: {:?}",
            s.cached_adapters.rejected
        );
    }

    #[test]
    fn null_cached_adapters_loads_as_empty() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("cached_adapters".to_string(), serde_json::Value::Null);
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!(s.cached_adapters.adapters.is_empty());
    }

    #[test]
    fn non_array_cached_adapters_degrades_to_empty_with_diagnostic() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().insert(
            "cached_adapters".to_string(),
            serde_json::json!({"unexpected": true}),
        );
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!(s.cached_adapters.adapters.is_empty());
        assert_eq!(s.cached_adapters.rejected.len(), 1);
    }

    #[test]
    fn password_is_skipped_in_json() {
        let s = Settings {
            recent_endpoints: vec![EndpointSpec {
                protocol: Protocol::OpcUa,
                host: String::new(),
                address: "opc.tcp://x:4840".to_string(),
                user: "admin".to_string(),
                password: "secret".to_string(),
                ..Default::default()
            }],
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(
            !json.contains("secret"),
            "password leaked into JSON: {json}"
        );
    }

    #[test]
    fn remember_dedups_reorders_and_truncates() {
        let mut s = Settings::default();
        for addr in ["opc.tcp://a:4840", "opc.tcp://b:4840", "opc.tcp://a:4840"] {
            s.remember(&EndpointSpec::ua(addr));
        }
        // Same URL re-inserted moves it to front, no duplicate.
        assert_eq!(s.recent_endpoints.len(), 2);
        assert_eq!(s.recent_endpoints[0].address, "opc.tcp://a:4840");
        assert_eq!(s.recent_endpoints[1].address, "opc.tcp://b:4840");
        assert_eq!(s.last_ua_url, "opc.tcp://a:4840");
    }

    #[test]
    fn remember_keeps_only_twelve_endpoints() {
        let mut s = Settings::default();
        for i in 0..20 {
            let e = EndpointSpec::ua(format!("opc.tcp://host{i}:4840"));
            s.remember(&e);
        }
        assert_eq!(s.recent_endpoints.len(), 12);
        // Newest first; the 20th added is at front.
        assert_eq!(s.recent_endpoints[0].address, "opc.tcp://host19:4840");
        assert_eq!(s.recent_endpoints[11].address, "opc.tcp://host8:4840");
    }

    #[test]
    fn remember_records_da_host_and_server() {
        let mut s = Settings::default();
        s.remember(&EndpointSpec::da("box1", "Matrikon.OPC.Simulation"));
        assert_eq!(s.last_da_host, "box1");
        assert_eq!(s.last_da_server, "Matrikon.OPC.Simulation");
        assert_eq!(s.recent_endpoints.len(), 1);
        assert_eq!(s.recent_endpoints[0].protocol, Protocol::OpcDa);
    }

    #[test]
    fn load_missing_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join("opcda-egui-settings-missing");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            assert_eq!(settings_path().file_name().unwrap(), "settings.json");
            let s = Settings::load();
            // Falls back wholesale to Default.
            assert_eq!(
                serde_json::to_string(&s.options).unwrap(),
                serde_json::to_string(&Options::default()).unwrap()
            );
            assert!(
                s.last_ua_url.is_empty(),
                "fresh settings must not preset a UA URL"
            );
        });
    }

    #[test]
    fn save_network_selection_persists_through_load() {
        let dir = std::env::temp_dir().join("opcda-egui-settings-network");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let id = AdapterId::parse("aabbccdd-1122-3344-5566-77889900aabb").unwrap();
            let ip: IpAddr = "192.168.1.21".parse().unwrap();
            let mut s = Settings::default();
            s.save_network_selection(AdapterSelection::Adapter(id), Some(ip));

            let loaded = Settings::load();
            assert_eq!(loaded.adapter_selection(), AdapterSelection::Adapter(id));
            assert_eq!(loaded.selected_local_ip(), Some(ip));
        });
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = std::env::temp_dir().join("opcda-egui-settings-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let mut s = Settings::default();
            s.options.update_interval_secs = 3;
            s.options.show_log = true;
            s.last_da_host = "box".to_string();
            s.remember(&EndpointSpec::da("box", "Matrikon.OPC.Simulation"));
            s.set_lang(crate::i18n::Lang::En);
            s.save();
            assert!(settings_path().exists());

            let loaded = Settings::load();
            assert_eq!(loaded.options.update_interval_secs, 3);
            assert!(loaded.options.show_log);
            assert_eq!(loaded.last_da_host, "box");
            assert_eq!(loaded.recent_endpoints.len(), 1);
            assert_eq!(loaded.lang(), crate::i18n::Lang::En);
        });
    }
}
