use crate::bff::model::{EndpointSpec, UaSecurityMode, UaSecurityPolicy};
use crate::columns::{ColumnId, ColumnSet};
use crate::constants::{
    OPERATION_TIMEOUT_DEFAULT_SECS, OPERATION_TIMEOUT_MAX_SECS, OPERATION_TIMEOUT_MIN_SECS,
    SESSION_TIMEOUT_DEFAULT_MS, TEST_INTERVAL_DEFAULT_SECS, TEST_INTERVAL_MAX_SECS,
    TEST_INTERVAL_MIN_SECS, UPDATE_INTERVAL_DEFAULT_SECS, UPDATE_INTERVAL_MAX_SECS,
    UPDATE_INTERVAL_MIN_SECS,
};
use crate::i18n::Lang;
use crate::net::{AdapterId, AdapterSelection, CachedAdapter};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::net::IpAddr;
use std::path::PathBuf;

/// Default «Доверять сертификату сервера» is OFF: strict PKI trust by default.
pub fn default_trust_server_cert() -> bool {
    false
}

/// Default UA session timeout in milliseconds (120 s).
pub fn default_session_timeout() -> u32 {
    SESSION_TIMEOUT_DEFAULT_MS
}

/// UI colour theme (UI9). `System` follows the OS light/dark preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
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
    /// K2: journal every request/reply pair with its latency (off by default).
    #[serde(default)]
    pub diagnostic_trace: bool,
    /// UI9: light / dark / follow the system. Settings files written before
    /// this field existed load as `System`.
    #[serde(default)]
    pub theme: Theme,
}

impl Options {
    pub fn clamped(mut self) -> Self {
        self.update_interval_secs = self
            .update_interval_secs
            .clamp(UPDATE_INTERVAL_MIN_SECS, UPDATE_INTERVAL_MAX_SECS);
        self.operation_timeout_secs = self
            .operation_timeout_secs
            .clamp(OPERATION_TIMEOUT_MIN_SECS, OPERATION_TIMEOUT_MAX_SECS);
        self.test_interval_secs = self
            .test_interval_secs
            .clamp(TEST_INTERVAL_MIN_SECS, TEST_INTERVAL_MAX_SECS);
        self.ua_deadband = if self.ua_deadband.is_finite() {
            self.ua_deadband.clamp(0.0, 1_000_000.0)
        } else {
            0.0
        };
        self
    }
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
            // UI7: both panels exist, the journal tab is the one shown first.
            show_script: true,
            show_log: true,
            ua_deadband: 0.0,
            diagnostic_trace: false,
            theme: Theme::System,
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

/// A load or save problem to be reported by the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsNotice {
    /// The file exists but could not be read.
    ReadFailed { path: String, error: String },
    /// The file was read but is not valid settings. `backup` is the copy the
    /// damaged file was saved to (`Err` carries why that copy failed).
    ParseFailed {
        path: String,
        error: String,
        backup: Result<String, String>,
    },
    /// Writing the settings failed (or was refused to protect a damaged file).
    SaveFailed { path: String, error: String },
    /// Saving would overwrite the only copy of unreadable settings.
    SaveBlocked { path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPrefs {
    pub options: Options,
    pub columns: ColumnSet,
    pub script: String,
    #[serde(default, deserialize_with = "deserialize_column_order")]
    pub column_order: Option<Vec<ColumnId>>,
    #[serde(default, deserialize_with = "deserialize_lang")]
    pub lang: Lang,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UaPrefs {
    #[serde(default = "default_trust_server_cert")]
    pub ua_trust_server_cert: bool,
    #[serde(default = "default_session_timeout")]
    pub ua_session_timeout_ms: u32,
    pub ua_security_policy: UaSecurityPolicy,
    pub ua_security_mode: UaSecurityMode,
    pub ua_lds_host: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkPrefs {
    #[serde(default, deserialize_with = "deserialize_adapter_id")]
    pub preferred_adapter: Option<AdapterId>,
    #[serde(default, deserialize_with = "deserialize_ip_address")]
    pub selected_local_ip: Option<IpAddr>,
    pub cached_adapters: CachedAdapters,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(flatten)]
    pub view: ViewPrefs,
    #[serde(flatten)]
    pub ua: UaPrefs,
    #[serde(flatten)]
    pub network: NetworkPrefs,
    pub last_da_host: String,
    pub last_da_server: String,
    pub last_ua_url: String,
    pub recent_endpoints: Vec<EndpointSpec>,
    /// Prevent overwriting an unreadable file or a failed backup.
    #[serde(skip)]
    pub save_blocked: bool,
}

fn deserialize_adapter_id<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<AdapterId>, D::Error> {
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.and_then(|value| AdapterId::parse(&value).ok()))
}

fn deserialize_ip_address<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<IpAddr>, D::Error> {
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.and_then(|value| value.trim().parse().ok()))
}

fn deserialize_lang<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Lang, D::Error> {
    let code = String::deserialize(deserializer)?;
    Ok(Lang::from_str(&code).unwrap_or_default())
}

fn deserialize_column_order<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<ColumnId>>, D::Error> {
    let raw = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut order = Vec::with_capacity(ColumnId::ALL.len());
    for value in raw {
        let Some(name) = value.as_str() else {
            continue;
        };
        let name = name.trim();
        let decoded = serde_json::from_str::<String>(name).unwrap_or_else(|_| name.to_owned());
        if let Ok(id) = serde_json::from_value::<ColumnId>(serde_json::Value::String(decoded)) {
            if !order.contains(&id) {
                order.push(id);
            }
        }
    }
    if order.is_empty() {
        return Ok(None);
    }
    for id in ColumnId::ALL {
        if !order.contains(&id) {
            order.push(id);
        }
    }
    Ok(Some(order))
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            options: Options::default(),
            columns: ColumnSet::default(),
            script: "program\nbegin\n\nend.\n".to_string(),
            column_order: None,
            lang: Lang::default(),
        }
    }
}

impl Default for UaPrefs {
    fn default() -> Self {
        Self {
            ua_trust_server_cert: default_trust_server_cert(),
            ua_session_timeout_ms: default_session_timeout(),
            ua_security_policy: UaSecurityPolicy::Auto,
            ua_security_mode: UaSecurityMode::Auto,
            ua_lds_host: String::new(),
        }
    }
}

#[derive(Default)]
pub struct Loaded {
    pub settings: Settings,
    pub notices: Vec<SettingsNotice>,
}

impl Settings {
    /// Load `settings.json`. A missing file is a normal first start. Any other
    /// problem (unreadable, malformed, unknown enum value) never resets the
    /// settings silently: the damaged file is copied to
    /// `settings.json.bad-<time>` before defaults are used, and a
    /// [`SettingsNotice`] is returned for the application to report.
    pub fn load() -> Loaded {
        let path = settings_path();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Loaded::default(),
            Err(err) => {
                let settings = Self {
                    save_blocked: true,
                    ..Self::default()
                };
                return Loaded {
                    settings,
                    notices: vec![SettingsNotice::ReadFailed {
                        path: path.display().to_string(),
                        error: err.to_string(),
                    }],
                };
            }
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(mut settings) => {
                settings.view.options = settings.view.options.clamped();
                Loaded {
                    settings,
                    notices: Vec::new(),
                }
            }
            Err(err) => {
                let backup = backup_damaged_file(&path);
                let settings = Self {
                    save_blocked: backup.is_err(),
                    ..Self::default()
                };
                Loaded {
                    settings,
                    notices: vec![SettingsNotice::ParseFailed {
                        path: path.display().to_string(),
                        error: err.to_string(),
                        backup,
                    }],
                }
            }
        }
    }

    /// Persist atomically, returning a diagnostic to the application on failure.
    pub fn save(&self) -> Result<(), SettingsNotice> {
        let path = settings_path();
        let shown = path.display().to_string();
        if self.save_blocked {
            return Err(SettingsNotice::SaveBlocked { path: shown });
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|err| SettingsNotice::SaveFailed {
                path: shown.clone(),
                error: err.to_string(),
            })?;
        write_atomic(&path, text.as_bytes()).map_err(|err| SettingsNotice::SaveFailed {
            path: shown,
            error: err.to_string(),
        })
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
                self.last_da_host.clone_from(&endpoint.host);
                self.last_da_server.clone_from(&endpoint.address);
            }
            crate::bff::model::Protocol::OpcUa => {
                self.last_ua_url.clone_from(&endpoint.address);
            }
        }
    }

    /// Current UI language, falling back to Russian for an unknown/missing code.
    pub fn lang(&self) -> Lang {
        self.view.lang
    }

    /// Remember the LDS host of the connect dialog; saves only on a change.
    pub fn set_ua_lds_host(&mut self, text: &str) -> Result<(), SettingsNotice> {
        let text = text.trim();
        if self.ua.ua_lds_host != text {
            self.ua.ua_lds_host = text.to_string();
            self.save()?;
        }
        Ok(())
    }

    /// Set the UI language and normalize its stored code.
    pub fn set_lang(&mut self, lang: Lang) {
        self.view.lang = lang;
    }

    /// The normalized order for table headers, the setup dialog and DnD.
    pub fn resolved_order(&self) -> Vec<ColumnId> {
        let saved = self
            .view
            .column_order
            .as_ref()
            .filter(|order| !order.is_empty())
            .unwrap_or(&self.view.columns.order);
        let mut order = Vec::with_capacity(ColumnId::ALL.len());
        for id in saved.iter().copied().chain(ColumnId::ALL) {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        order
    }

    /// Translate a gap in the visible header to the persisted full order.
    pub fn move_visible_column_to_insert(&mut self, id: ColumnId, before: usize) -> bool {
        let order = self.resolved_order();
        let visible: Vec<_> = order
            .iter()
            .copied()
            .filter(|column| self.view.columns.is_visible(*column))
            .collect();
        let full_gap = if let Some(target) = visible.get(before) {
            order
                .iter()
                .position(|column| column == target)
                .unwrap_or(order.len())
        } else if let Some(last) = visible.last() {
            order
                .iter()
                .position(|column| column == last)
                .map_or(order.len(), |index| index + 1)
        } else {
            return false;
        };
        self.move_column_to_insert(id, full_gap)
    }

    /// Apply the same move semantics to the effective order no matter whether
    /// it originated in the header menu, dialog, or legacy settings.
    pub fn move_column_to_insert(&mut self, id: ColumnId, before: usize) -> bool {
        let mut columns = self.view.columns.clone();
        columns.order = self.resolved_order();
        if !columns.move_to_insert(id, before) {
            return false;
        }
        self.persist_column_order(&columns.order);
        true
    }

    /// Keep the legacy order and the persisted header order identical.
    /// A malformed caller-provided order is repaired using the canonical ids.
    pub fn persist_column_order(&mut self, order: &[ColumnId]) {
        let mut normalized = Vec::with_capacity(ColumnId::ALL.len());
        for id in order.iter().copied().chain(ColumnId::ALL) {
            if !normalized.contains(&id) {
                normalized.push(id);
            }
        }
        self.view.columns.order.clone_from(&normalized);
        self.view.column_order = Some(normalized);
    }

    /// Effective adapter selection. Invalid persisted GUIDs load unselected.
    pub fn adapter_selection(&self) -> AdapterSelection {
        self.network
            .preferred_adapter
            .map(AdapterSelection::Adapter)
            .unwrap_or(AdapterSelection::None)
    }

    pub fn preferred_adapter_id(&self) -> Option<AdapterId> {
        self.network.preferred_adapter
    }

    pub fn set_adapter_selection(&mut self, selection: AdapterSelection) {
        self.network.preferred_adapter = selection.adapter_id();
    }

    pub fn selected_local_ip(&self) -> Option<IpAddr> {
        self.network.selected_local_ip
    }

    pub fn set_selected_local_ip(&mut self, ip: Option<IpAddr>) {
        self.network.selected_local_ip = ip;
    }

    /// Store and persist the user's network choice.
    pub fn save_network_selection(
        &mut self,
        selection: AdapterSelection,
        local_ip: Option<IpAddr>,
    ) -> Result<(), SettingsNotice> {
        self.set_adapter_selection(selection);
        self.set_selected_local_ip(local_ip);
        self.save()
    }
}

/// Write `bytes` to `path` through a sibling temporary file and a rename.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(err) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Copy a damaged settings file to `settings.json.bad-<YYYYMMDD-HHMMSS>` (a
/// numeric suffix keeps repeated failures within one second apart). Returns
/// the backup path.
fn backup_damaged_file(path: &std::path::Path) -> Result<String, String> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    for attempt in 0..100u32 {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".bad-{stamp}"));
        if attempt > 0 {
            name.push(format!("-{attempt}"));
        }
        let target = PathBuf::from(name);
        if target.exists() {
            continue;
        }
        return std::fs::copy(path, &target)
            .map(|_| target.display().to_string())
            .map_err(|err| err.to_string());
    }
    Err("no free backup file name".to_string())
}

/// Per-thread redirect of the settings directory for headless tests of the
/// application (`tests/*.rs`), which run in the same process and in parallel.
///
/// Redirecting through the process-wide `APPDATA` variable made such tests
/// interleave: one test's `save()` landed in another test's directory, and
/// `set_var` itself races with every other thread reading the environment. A
/// thread-local directory cannot be seen by other tests. The application never
/// installs one; without it `app_dir()` reads `%APPDATA%` as before.
pub mod thread_dir {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    thread_local! {
        static DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
        static OWNED: RefCell<Option<Owned>> = const { RefCell::new(None) };
    }

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// A private directory created for this thread; removed when the thread ends.
    struct Owned(PathBuf);

    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub(super) fn current() -> Option<PathBuf> {
        DIR.with(|dir| dir.borrow().clone())
    }

    /// Redirect this thread's settings to `dir` until the guard is dropped.
    pub fn redirect(dir: &Path) -> Redirect {
        let previous = DIR.with(|cell| cell.borrow_mut().replace(dir.to_path_buf()));
        Redirect { previous }
    }

    /// Restores the previous redirect of the thread.
    pub struct Redirect {
        previous: Option<PathBuf>,
    }

    impl Drop for Redirect {
        fn drop(&mut self) {
            let previous = self.previous.take();
            DIR.with(|cell| *cell.borrow_mut() = previous);
        }
    }

    /// Give this thread a private temporary settings directory unless it has a
    /// redirect already, so a headless test that saves settings never writes to
    /// the real `%APPDATA%` or into another test's directory.
    pub fn ensure_private() {
        if current().is_some() {
            return;
        }
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opclook-thread-settings-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        DIR.with(|cell| *cell.borrow_mut() = Some(dir.clone()));
        OWNED.with(|cell| *cell.borrow_mut() = Some(Owned(dir)));
    }
}

pub fn app_dir() -> PathBuf {
    // Unit tests must never touch the developer's real settings: in test builds
    // the base directory is a per-test override or a private per-process temp
    // directory (see `test_isolation`). Headless tests of the application
    // redirect per thread (see `thread_dir`). The real `%APPDATA%` is read only
    // by the application itself.
    #[cfg(test)]
    if let Some(dir) = test_isolation::current_override() {
        return dir.join("opclook");
    }
    if let Some(dir) = thread_dir::current() {
        return dir.join("opclook");
    }
    #[cfg(test)]
    let base = test_isolation::fallback_dir();
    #[cfg(not(test))]
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("opclook")
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
                    std::env::temp_dir().join(format!("opclook-libtest-{}", std::process::id()));
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
                .join(format!("opclook-libtest-{label}-{}", std::process::id()));
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
    fn options_defaults_match_the_spec() {
        let d = Options::default();
        assert_eq!(d.update_interval_secs, 1);
        assert_eq!(d.operation_timeout_secs, 60);
        assert_eq!(d.test_interval_secs, 60);
        assert!(d.auto_reconnect);
        assert!(d.show_colors);
        assert!(d.confirm_write);
        assert!(d.show_script);
        // UI7: the journal is available by default (shown first).
        assert!(d.show_log);
    }

    /// A default/unset settings file defaults to Russian; an explicitly
    /// persisted language (e.g. "en") is never overridden on load.
    #[test]
    fn fresh_settings_default_to_russian_and_persisted_lang_is_kept() {
        let s: Settings = serde_json::from_str(r#"{"lang":""}"#).unwrap();
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
    fn grouped_preferences_keep_the_legacy_flat_json_schema() {
        let value = serde_json::to_value(Settings::default()).unwrap();
        let keys: std::collections::BTreeSet<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let expected = [
            "options",
            "columns",
            "script",
            "column_order",
            "lang",
            "ua_trust_server_cert",
            "ua_session_timeout_ms",
            "ua_security_policy",
            "ua_security_mode",
            "ua_lds_host",
            "preferred_adapter",
            "selected_local_ip",
            "cached_adapters",
            "last_da_host",
            "last_da_server",
            "last_ua_url",
            "recent_endpoints",
        ]
        .into_iter()
        .collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn settings_defaults_field_values() {
        let d = Settings::default();
        let o = d.view.options.clone();
        let od = Options::default();
        assert_eq!(o.update_interval_secs, od.update_interval_secs);
        assert_eq!(o.operation_timeout_secs, od.operation_timeout_secs);
        assert_eq!(o.test_interval_secs, od.test_interval_secs);
        assert_eq!(o.auto_reconnect, od.auto_reconnect);
        assert_eq!(o.show_colors, od.show_colors);
        assert_eq!(o.confirm_write, od.confirm_write);
        assert_eq!(o.show_script, od.show_script);
        assert_eq!(o.show_log, od.show_log);
        assert_eq!(d.view.columns.order, ColumnId::ALL.to_vec());
        assert!(d.view.columns.hidden.is_empty());
        assert!(d.view.script.contains("program"));
        assert!(d.last_da_host.is_empty());
        assert!(d.last_da_server.is_empty());
        assert!(
            d.last_ua_url.is_empty(),
            "no preset UA URL in fresh settings"
        );
        assert!(d.recent_endpoints.is_empty());
        assert!(
            d.view.column_order.is_none(),
            "column_order defaults to None"
        );
        assert_eq!(d.view.lang, Lang::Ru, "UI language defaults to Russian");
        assert_eq!(d.lang(), crate::i18n::Lang::Ru);
        assert!(d.network.preferred_adapter.is_none());
        assert!(d.network.selected_local_ip.is_none());
        assert_eq!(d.adapter_selection(), AdapterSelection::None);
    }

    #[test]
    fn ua_security_defaults_are_strict() {
        let d = Settings::default();
        // Default OFF: server certs are not auto-trusted.
        assert!(
            !d.ua.ua_trust_server_cert,
            "trust_server_cert must default to false"
        );
        // Session timeout defaults to 120 s.
        assert_eq!(d.ua.ua_session_timeout_ms, 120_000);
    }

    #[test]
    fn ua_security_fields_serde_roundtrip() {
        let mut s = Settings::default();
        s.ua.ua_trust_server_cert = true;
        s.ua.ua_session_timeout_ms = 300_000;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"ua_trust_server_cert\":true"));
        assert!(json.contains("\"ua_session_timeout_ms\":300000"));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert!(back.ua.ua_trust_server_cert);
        assert_eq!(back.ua.ua_session_timeout_ms, 300_000);
    }

    #[test]
    fn ua_security_selection_defaults_to_auto_and_roundtrips() {
        let d = Settings::default();
        assert_eq!(d.ua.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(d.ua.ua_security_mode, UaSecurityMode::Auto);

        let mut s = Settings::default();
        s.ua.ua_security_policy = UaSecurityPolicy::Basic256Sha256;
        s.ua.ua_security_mode = UaSecurityMode::SignAndEncrypt;
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
        assert_eq!(back.ua.ua_security_policy, UaSecurityPolicy::Basic256Sha256);
        assert_eq!(back.ua.ua_security_mode, UaSecurityMode::SignAndEncrypt);
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
        assert_eq!(s.ua.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(s.ua.ua_security_mode, UaSecurityMode::None);

        // A full URI is accepted as well.
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().insert(
            "ua_security_policy".to_string(),
            serde_json::json!("http://opcfoundation.org/UA/SecurityPolicy#Basic256"),
        );
        let s: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(s.ua.ua_security_policy, UaSecurityPolicy::Basic256);
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
        assert_eq!(s.ua.ua_security_policy, UaSecurityPolicy::Auto);
        assert_eq!(s.ua.ua_security_mode, UaSecurityMode::Auto);
    }

    #[test]
    fn k2_diagnostic_trace_defaults_off_and_survives_old_files() {
        assert!(!Options::default().diagnostic_trace);
        let mut s = Settings::default();
        s.view.options.diagnostic_trace = true;
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            serde_json::from_str::<Settings>(&json)
                .unwrap()
                .view
                .options
                .diagnostic_trace
        );
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value["options"]
            .as_object_mut()
            .unwrap()
            .remove("diagnostic_trace");
        let legacy: Settings = serde_json::from_value(value).unwrap();
        assert!(!legacy.view.options.diagnostic_trace);
    }

    #[test]
    fn ua_deadband_defaults_off_and_serde_roundtrips() {
        // Default 0 = off; a positive value survives a JSON roundtrip; an old
        // settings file without the field deserializes as off.
        assert_eq!(Options::default().ua_deadband, 0.0);
        let mut s = Settings::default();
        s.view.options.ua_deadband = 12.5;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"ua_deadband\":12.5"));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.view.options.ua_deadband, 12.5);

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
            legacy.view.options.ua_deadband, 0.0,
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
        assert!(!s.ua.ua_trust_server_cert);
        assert_eq!(s.ua.ua_session_timeout_ms, 120_000);
    }

    #[test]
    fn unknown_or_missing_lang_falls_back_to_russian() {
        for code in ["zz", ""] {
            let value = serde_json::json!({"lang": code});
            let s: Settings = serde_json::from_value(value).unwrap();
            assert_eq!(s.lang(), Lang::Ru);
        }
        let missing: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.lang(), Lang::Ru);
        let mut s = Settings::default();
        s.set_lang(Lang::En);
        assert_eq!(s.view.lang, Lang::En);
        assert_eq!(serde_json::to_value(&s).unwrap()["lang"], "en");
        let legacy: Settings = serde_json::from_str(r#"{"lang":"English"}"#).unwrap();
        assert_eq!(legacy.lang(), Lang::En);
    }

    #[test]
    fn serde_roundtrip_json_preserves_all_fields() {
        let mut s = Settings::default();
        s.view.options.update_interval_secs = 5;
        s.view.options.operation_timeout_secs = 300;
        s.view.options.test_interval_secs = 120;
        s.view.options.show_script = false;
        s.view.options.show_log = true;
        s.view.script = "custom\n".to_string();
        s.last_da_host = "h1".to_string();
        s.last_da_server = "Srv.1".to_string();
        s.last_ua_url = "opc.tcp://x:4840".to_string();
        s.recent_endpoints = vec![
            EndpointSpec::ua("opc.tcp://x:4840"),
            EndpointSpec::da("h", "Srv.1"),
        ];
        s.view.columns.order = vec![
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
        s.persist_column_order(&s.view.columns.order.clone());
        s.set_lang(crate::i18n::Lang::Zh);
        s.set_adapter_selection(AdapterSelection::Adapter(
            AdapterId::parse("aabbccdd-1122-3344-5566-77889900aabb").unwrap(),
        ));
        s.set_selected_local_ip(Some("192.168.1.21".parse().unwrap()));

        let json = serde_json::to_string(&s).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.lang(), crate::i18n::Lang::Zh);
        assert!(json.contains("\"lang\":\"zh\""));
        assert_eq!(back.view.options.update_interval_secs, 5);
        assert_eq!(back.view.options.operation_timeout_secs, 300);
        assert_eq!(back.view.options.test_interval_secs, 120);
        assert!(!back.view.options.show_script);
        assert!(back.view.options.show_log);
        assert_eq!(back.view.script, "custom\n");
        assert_eq!(back.last_da_host, "h1");
        assert_eq!(back.last_da_server, "Srv.1");
        assert_eq!(back.last_ua_url, "opc.tcp://x:4840");
        assert_eq!(back.recent_endpoints.len(), 2);
        assert_eq!(back.recent_endpoints[0].protocol, Protocol::OpcUa);
        assert_eq!(back.recent_endpoints[1].address, "Srv.1");
        let column_order = back
            .view
            .column_order
            .clone()
            .expect("column_order persisted");
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
        assert_eq!(s.view.columns.order, resolved);
        assert_eq!(s.view.column_order.as_ref().unwrap().len(), 9);
    }

    #[test]
    fn resolved_order_falls_back_when_column_order_is_none() {
        let s = Settings::default();
        assert!(s.view.column_order.is_none());
        assert_eq!(s.resolved_order(), ColumnId::ALL.to_vec());
    }

    #[test]
    fn resolved_order_ignores_unknown_entries_and_fills_missing() {
        let s: Settings =
            serde_json::from_str(r#"{"column_order":["\"Value\"","\"NoSuchColumn\""]}"#).unwrap();
        let resolved = s.resolved_order();
        // Unknown entry dropped; full canonical set preserved with Value first.
        assert_eq!(resolved[0], ColumnId::Value);
        assert_eq!(resolved.len(), ColumnId::ALL.len());
        for id in ColumnId::ALL {
            assert!(resolved.contains(&id), "missing {id:?}");
        }
    }

    #[test]
    fn column_order_deduplicates_and_repairs_legacy_entries() {
        let mut s: Settings =
            serde_json::from_str(r#"{"column_order":["Value","Value","Status","Unknown"]}"#)
                .unwrap();
        s.view.columns.order = vec![ColumnId::Value, ColumnId::Value];
        let resolved = s.resolved_order();
        assert_eq!(resolved.len(), ColumnId::ALL.len());
        assert_eq!(&resolved[..2], &[ColumnId::Value, ColumnId::Status]);
        s.persist_column_order(&[ColumnId::Status, ColumnId::Status]);
        assert_eq!(s.view.columns.order.len(), ColumnId::ALL.len());
        assert_eq!(
            s.view.column_order.as_ref().unwrap().len(),
            ColumnId::ALL.len()
        );
    }

    #[test]
    fn visible_column_drag_accounts_for_hidden_columns() {
        let mut settings = Settings::default();
        settings.view.columns.set_visible(ColumnId::FullName, false);
        assert!(settings.move_visible_column_to_insert(ColumnId::Quality, 1));
        let visible: Vec<_> = settings
            .resolved_order()
            .into_iter()
            .filter(|id| settings.view.columns.is_visible(*id))
            .collect();
        assert_eq!(&visible[..2], &[ColumnId::Status, ColumnId::Quality]);
    }

    #[test]
    fn column_drag_uses_resolved_order_and_persists() {
        let mut s = Settings::default();
        s.view.column_order = Some(vec![ColumnId::Value, ColumnId::Status]);
        assert!(s.move_column_to_insert(ColumnId::Status, 0));
        assert_eq!(s.resolved_order()[0], ColumnId::Status);
        assert_eq!(s.resolved_order()[1], ColumnId::Value);
        assert_eq!(s.view.columns.order, s.resolved_order());
        assert!(!s.move_column_to_insert(ColumnId::Status, 0));
    }

    #[test]
    fn resolved_order_parses_both_plain_and_double_quoted_names() {
        // A legacy settings.json wrote a double-encoded ordering: each element
        // is a JSON string literal containing the quoted variant name, e.g.
        // ["\"Server\"", "…"]. Newer builds write plain names, e.g. ["Server"].
        // Both must resolve.
        let legacy: Settings =
            serde_json::from_str(r#"{"column_order":["\"Server\"","\"FullName\"","\"Status\""]}"#)
                .unwrap();
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
        let plain: Settings =
            serde_json::from_str(r#"{"column_order":["Value","Time","Server"]}"#).unwrap();
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
        let stored = s.view.column_order.as_ref().unwrap();
        // Persisted entries are plain variant names, NOT "\"Server\"" encodings.
        assert_eq!(stored[0], ColumnId::Value);
        assert_eq!(stored[3], ColumnId::Server);
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
        assert!(s.network.selected_local_ip.is_none());
    }

    #[test]
    fn network_selection_roundtrips_and_normalizes() {
        let id = AdapterId::parse("AABBCCDD-1122-3344-5566-77889900AABB").unwrap();
        let ip: IpAddr = "192.168.1.21".parse().unwrap();
        let mut s = Settings::default();
        s.set_adapter_selection(AdapterSelection::Adapter(id));
        s.set_selected_local_ip(Some(ip));
        assert_eq!(s.network.preferred_adapter, Some(id));
        assert_eq!(s.network.selected_local_ip, Some(ip));
        assert_eq!(s.adapter_selection(), AdapterSelection::Adapter(id));
        assert_eq!(s.selected_local_ip(), Some(ip));

        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adapter_selection(), AdapterSelection::Adapter(id));
        assert_eq!(back.selected_local_ip(), Some(ip));
    }

    #[test]
    fn invalid_persisted_network_values_degrade_to_unselected() {
        let invalid: Settings = serde_json::from_str(
            r#"{"preferred_adapter":"not-a-guid","selected_local_ip":"not-an-ip"}"#,
        )
        .unwrap();
        assert_eq!(invalid.adapter_selection(), AdapterSelection::None);
        assert!(invalid.selected_local_ip().is_none());

        let valid: Settings = serde_json::from_str(r#"{"preferred_adapter":"{AABBCCDD-1122-3344-5566-77889900AABB}","selected_local_ip":" 10.0.0.5 "}"#).unwrap();
        let json = serde_json::to_value(valid).unwrap();
        assert_eq!(
            json["preferred_adapter"],
            "aabbccdd-1122-3344-5566-77889900aabb"
        );
        assert_eq!(json["selected_local_ip"], "10.0.0.5");
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
            if_index: std::num::NonZeroU32::new(3),
            ipv6_if_index: std::num::NonZeroU32::new(3),
            luid: 123,
            metric: 25,
            is_physical: true,
        }
    }

    #[test]
    fn cached_adapters_roundtrip_and_load() {
        let mut s = Settings::default();
        s.network.cached_adapters = CachedAdapters::from_vec(vec![cached_fixture()]);
        s.set_adapter_selection(AdapterSelection::Adapter(cached_fixture().id));
        s.set_selected_local_ip(Some("172.28.76.90".parse().unwrap()));
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"cached_adapters\":["), "got: {json}");
        assert!(json.contains("\"state\":\"up\""), "got: {json}");
        assert!(json.contains("\"name\":\"Wi-Fi\""), "got: {json}");

        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.network.cached_adapters.adapters.len(), 1);
        assert!(back.network.cached_adapters.rejected.is_empty());
        assert_eq!(back.network.cached_adapters.adapters[0], cached_fixture());
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
        assert!(s.network.cached_adapters.adapters.is_empty());
        assert!(s.network.cached_adapters.rejected.is_empty());
    }

    #[test]
    fn deleted_config_loads_with_empty_adapter_cache() {
        let dir = std::env::temp_dir().join("opclook-settings-no-adapter-cache");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let s = Settings::load().settings;
            assert!(s.network.cached_adapters.adapters.is_empty());
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
        assert_eq!(s.network.cached_adapters.adapters.len(), 1);
        assert_eq!(s.network.cached_adapters.rejected.len(), 1);
        assert!(
            s.network.cached_adapters.rejected[0].contains("cached_adapters[1]"),
            "got: {:?}",
            s.network.cached_adapters.rejected
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
        assert!(s.network.cached_adapters.adapters.is_empty());
    }

    #[test]
    fn non_array_cached_adapters_degrades_to_empty_with_diagnostic() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().insert(
            "cached_adapters".to_string(),
            serde_json::json!({"unexpected": true}),
        );
        let s: Settings = serde_json::from_value(value).unwrap();
        assert!(s.network.cached_adapters.adapters.is_empty());
        assert_eq!(s.network.cached_adapters.rejected.len(), 1);
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
        let dir = std::env::temp_dir().join("opclook-settings-missing");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            assert_eq!(settings_path().file_name().unwrap(), "settings.json");
            let s = Settings::load().settings;
            // Falls back wholesale to Default.
            assert_eq!(
                serde_json::to_string(&s.view.options).unwrap(),
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
        let dir = std::env::temp_dir().join("opclook-settings-network");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let id = AdapterId::parse("aabbccdd-1122-3344-5566-77889900aabb").unwrap();
            let ip: IpAddr = "192.168.1.21".parse().unwrap();
            let mut s = Settings::default();
            s.save_network_selection(AdapterSelection::Adapter(id), Some(ip))
                .unwrap();

            let loaded = Settings::load().settings;
            assert_eq!(loaded.adapter_selection(), AdapterSelection::Adapter(id));
            assert_eq!(loaded.selected_local_ip(), Some(ip));
        });
    }

    #[test]
    fn u15_lds_host_persists_and_old_files_load_without_it() {
        let dir = std::env::temp_dir().join("opclook-settings-lds-host");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let mut s = Settings::default();
            s.set_ua_lds_host("  lds.plant:4850 ").unwrap();
            assert_eq!(Settings::load().settings.ua.ua_lds_host, "lds.plant:4850");
            s.set_ua_lds_host("").unwrap();
            assert_eq!(Settings::load().settings.ua.ua_lds_host, "");
        });
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("ua_lds_host");
        let old: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(old.ua.ua_lds_host, "");
    }

    #[test]
    fn load_partial_legacy_file_preserves_fields_and_clamps_options() {
        let dir = std::env::temp_dir().join("opclook-settings-partial-legacy");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let path = settings_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, r#"{"last_da_host":"legacy-host","options":{"update_interval_secs":18446744073709551615}}"#).unwrap();
            let loaded_result = Settings::load();
            assert!(loaded_result.notices.is_empty());
            let loaded = loaded_result.settings;
            assert_eq!(loaded.last_da_host, "legacy-host");
            assert_eq!(
                loaded.view.options.update_interval_secs,
                UPDATE_INTERVAL_MAX_SECS
            );
            assert_eq!(
                loaded.view.options.auto_reconnect,
                Options::default().auto_reconnect
            );
            assert_eq!(loaded.last_ua_url, Settings::default().last_ua_url);
        });
    }

    #[test]
    fn load_clamps_hostile_timing_options() {
        let dir = std::env::temp_dir().join("opclook-settings-extreme-interval");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let mut settings = Settings::default();
            settings.view.options.update_interval_secs = u64::MAX;
            settings.view.options.test_interval_secs = u64::MAX;
            settings.view.options.operation_timeout_secs = u64::MAX;
            settings.save().unwrap();
            let loaded = Settings::load().settings;
            assert_eq!(
                loaded.view.options.update_interval_secs,
                UPDATE_INTERVAL_MAX_SECS
            );
            assert_eq!(
                loaded.view.options.test_interval_secs,
                TEST_INTERVAL_MAX_SECS
            );
            assert_eq!(
                loaded.view.options.operation_timeout_secs,
                OPERATION_TIMEOUT_MAX_SECS
            );
        });
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = std::env::temp_dir().join("opclook-settings-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        with_appdata(&dir, || {
            let mut s = Settings::default();
            s.view.options.update_interval_secs = 3;
            s.view.options.show_log = true;
            s.last_da_host = "box".to_string();
            s.remember(&EndpointSpec::da("box", "Matrikon.OPC.Simulation"));
            s.set_lang(crate::i18n::Lang::En);
            s.save().unwrap();
            assert!(settings_path().exists());

            let loaded = Settings::load().settings;
            assert_eq!(loaded.view.options.update_interval_secs, 3);
            assert!(loaded.view.options.show_log);
            assert_eq!(loaded.last_da_host, "box");
            assert_eq!(loaded.recent_endpoints.len(), 1);
            assert_eq!(loaded.lang(), crate::i18n::Lang::En);
        });
    }

    // ---- A2: no silent loss, atomic writes ----

    fn fresh_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opclook-settings-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn corrupt_file_is_backed_up_and_reported_not_silently_reset() {
        let dir = fresh_dir("corrupt");
        with_appdata(&dir, || {
            let path = settings_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "{ not json").unwrap();

            let loaded_result = Settings::load();
            let loaded = loaded_result.settings;
            assert_eq!(
                loaded.view.options.update_interval_secs, 1,
                "defaults are used"
            );
            let notices = loaded_result.notices;
            assert_eq!(notices.len(), 1, "{notices:?}");
            let SettingsNotice::ParseFailed { backup, .. } = &notices[0] else {
                panic!("expected ParseFailed, got {notices:?}");
            };
            let backup = backup.as_ref().expect("backup must succeed");
            assert!(backup.contains("settings.json.bad-"), "{backup}");
            assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ not json");
            // The damaged original is still there until the next save.
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
            loaded.save().unwrap();
            assert!(Settings::load().notices.is_empty());
            assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ not json");
        });
    }

    #[test]
    fn unknown_security_policy_is_reported_with_a_backup() {
        let dir = fresh_dir("unknown-policy");
        with_appdata(&dir, || {
            let mut value = serde_json::to_value(Settings::default()).unwrap();
            value["ua_security_policy"] = serde_json::json!("Quantum9000");
            let path = settings_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

            let notices = Settings::load().notices;
            assert!(
                matches!(&notices[..], [SettingsNotice::ParseFailed { error, backup: Ok(_), .. }] if error.contains("Quantum9000")),
                "{notices:?}"
            );
        });
    }

    #[test]
    fn save_replaces_the_file_atomically_and_leaves_no_temp_file() {
        let dir = fresh_dir("atomic");
        with_appdata(&dir, || {
            let mut s = Settings::default();
            s.last_da_host = "first".to_string();
            s.save().unwrap();
            s.last_da_host = "second".to_string();
            s.save().unwrap();
            let tmp = {
                let mut t = settings_path().into_os_string();
                t.push(".tmp");
                PathBuf::from(t)
            };
            assert!(!tmp.exists(), "temporary file must be renamed away");
            assert_eq!(Settings::load().settings.last_da_host, "second");
            assert!(s.save().is_ok());
        });
    }

    #[test]
    fn failed_save_is_reported_and_cleans_up_the_temp_file() {
        let dir = fresh_dir("save-fails");
        with_appdata(&dir, || {
            // A directory squatting on the target makes the final rename fail.
            std::fs::create_dir_all(settings_path()).unwrap();
            let s = Settings::default();
            let notices = vec![s.save().unwrap_err()];
            assert!(
                matches!(&notices[..], [SettingsNotice::SaveFailed { .. }]),
                "{notices:?}"
            );
            let mut tmp = settings_path().into_os_string();
            tmp.push(".tmp");
            assert!(!PathBuf::from(tmp).exists());
        });
    }

    #[test]
    fn unreadable_file_is_reported_and_never_overwritten() {
        let dir = fresh_dir("unreadable");
        with_appdata(&dir, || {
            // Reading a directory fails with a non-NotFound error.
            std::fs::create_dir_all(settings_path()).unwrap();
            let loaded_result = Settings::load();
            let loaded = loaded_result.settings;
            assert!(loaded.save_blocked);
            let notices = loaded_result.notices;
            assert!(
                matches!(&notices[..], [SettingsNotice::ReadFailed { .. }]),
                "{notices:?}"
            );
            let notices = vec![loaded.save().unwrap_err()];
            assert!(
                matches!(&notices[..], [SettingsNotice::SaveBlocked { .. }]),
                "a blocked save must say so: {notices:?}"
            );
            assert!(settings_path().is_dir(), "nothing was overwritten");
        });
    }
}
