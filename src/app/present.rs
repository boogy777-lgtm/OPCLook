use crate::bff::model::{EndpointSpec, Operation, Protocol};
use crate::i18n::{Lang, Tr, fmt, tr};

/// Compose the human-readable server name shown in the servers list / tab strip.
///
/// Name source per protocol:
/// - If the server advertised a name (set when the user picks a discovered
///   server: UA application name, DA description + ProgID), that name is used.
/// - OPC UA fallback: the `host:port` of the endpoint URL.
/// - OPC DA fallback: the ProgID (the address).
///
/// A remote host (not 127.0.0.1 / localhost / empty-local) appends
/// « — <host>» so two servers on different PCs are distinguishable. For UA the
/// suffix is only appended when an advertised name is used (the host:port
/// fallback already contains the host).
pub fn compose_server_name(endpoint: &EndpointSpec) -> String {
    let advertised = endpoint.display_name.as_deref();
    let base = match advertised {
        Some(name) => name.to_string(),
        None => match endpoint.protocol {
            Protocol::OpcUa => ua_host_port(&endpoint.address),
            Protocol::OpcDa => {
                if endpoint.address.is_empty() {
                    "?".to_string()
                } else {
                    endpoint.address.clone()
                }
            }
        },
    };
    let remote_host = match endpoint.protocol {
        Protocol::OpcDa => remote_host_opt(&endpoint.host),
        Protocol::OpcUa => {
            if advertised.is_some() {
                ua_url_host(&endpoint.address).filter(|h| is_remote_host(h))
            } else {
                None
            }
        }
    };
    match remote_host {
        Some(host) => format!("{base} — {host}"),
        None => base,
    }
}

/// The trim-able `host` for the DA suffix, `None` for local/empty hosts.
fn remote_host_opt(host: &str) -> Option<String> {
    let h = host.trim();
    if is_remote_host(h) {
        Some(h.to_string())
    } else {
        None
    }
}

/// True when `host` refers to a machine other than this one (not a loopback
/// address / localhost / empty local DA host).
fn is_remote_host(host: &str) -> bool {
    let h = host.trim();
    !h.is_empty() && !is_loopback_host(h)
}

/// The `host:port` portion of an `opc.tcp://host:port` URL (or the host when no
/// port is present). IPv6 literals are bracketed again: `[::1]:4840`.
fn ua_host_port(url: &str) -> String {
    match ua_url_host(url) {
        Some(host) if !host.is_empty() => {
            let shown = if host.contains(':') {
                format!("[{host}]")
            } else {
                host
            };
            match ua_url_port(url) {
                Some(port) => format!("{shown}:{port}"),
                None => shown,
            }
        }
        _ => url.to_string(),
    }
}

/// Host of a validated OPC UA URL; IPv6 brackets are removed.
pub(super) fn ua_url_host(url: &str) -> Option<String> {
    url.parse::<crate::bff::model::UaUrl>()
        .ok()
        .map(|url| url.host().to_owned())
}

fn ua_url_port(url: &str) -> Option<String> {
    let parsed = url.parse::<crate::bff::model::UaUrl>().ok()?;
    parsed.explicit_port().then(|| parsed.port().to_string())
}

pub(super) fn operation_label(lang: Lang, operation: Operation) -> String {
    use Tr::*;
    let key = match operation {
        Operation::Connect => OpConnect,
        Operation::Disconnect => OpDisconnect,
        Operation::Browse => OpBrowse,
        Operation::BrowseViews => OpBrowseViews,
        Operation::ResolveBrowsePath => OpResolvePath,
        Operation::AddItems | Operation::Add => OpAdd,
        Operation::RemoveItems | Operation::Remove => OpRemove,
        Operation::Read => OpRead,
        Operation::Write => OpWrite,
        Operation::Test => OpTest,
        Operation::Discover => OpDiscover,
        Operation::SearchServers => OpSearchServers,
        Operation::SearchNetwork => OpSearchNetwork,
        Operation::Subscription => OpSubscription,
        Operation::UaRuntime => OpUaRuntime,
    };
    tr(lang, key).to_string()
}

/// Map a scanner error token to a localized message. Unknown details keep the
/// technical text wrapped in the localized "scan failed" template.
pub(super) fn localize_scan_error(lang: Lang, detail: &str) -> String {
    if let Some(count) = detail
        .strip_prefix(crate::bff::ua::SCAN_SUBNET_TOO_LARGE)
        .and_then(|rest| rest.strip_prefix(':'))
    {
        return fmt(lang, Tr::UaScanTooLarge, &[&count]);
    }
    fmt(lang, Tr::UaScanFailed, &[&detail])
}

/// Hosts that must be discovered through loopback, never through the selected
/// physical adapter's local address: `localhost`, the whole `127.0.0.0/8`
/// block and `::1` (also in brackets or as an IPv4-mapped address).
pub(super) fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.to_canonical().is_loopback())
}

/// A font file plus the face to use inside it (`.ttc` collections hold several).
pub(super) struct FontFile {
    pub path: std::path::PathBuf,
    pub index: u32,
}

/// Environment variable naming a user-chosen CJK font file (optionally
/// `path#face-index`); consulted before the built-in candidate list.
pub const CJK_FONT_ENV: &str = "OPCLOOK_CJK_FONT";

/// CJK fallback candidates, best first: the fonts every Windows install ships
/// (Microsoft YaHei, SimSun, Microsoft JhengHei, Yu Gothic, Malgun), then the
/// usual Linux/macOS locations (used when the UI is checked on those hosts).
fn cjk_candidates() -> Vec<FontFile> {
    let from_env = std::env::var_os(CJK_FONT_ENV).map(|raw| raw.to_string_lossy().into_owned());
    cjk_candidates_with(from_env)
}

/// [`cjk_candidates`] with the override already read (testable without
/// touching the process environment).
pub(super) fn cjk_candidates_with(from_env: Option<String>) -> Vec<FontFile> {
    let mut list = Vec::new();
    if let Some(raw) = from_env {
        let (path, index) = match raw.rsplit_once('#') {
            Some((path, face)) => (path.to_string(), face.parse().unwrap_or(0)),
            None => (raw, 0),
        };
        list.push(FontFile {
            path: path.into(),
            index,
        });
    }
    let fixed: &[(&str, u32)] = &[
        ("C:\\Windows\\Fonts\\msyh.ttc", 0),
        ("C:\\Windows\\Fonts\\msyh.ttf", 0),
        ("C:\\Windows\\Fonts\\simsun.ttc", 0),
        ("C:\\Windows\\Fonts\\msjh.ttc", 0),
        ("C:\\Windows\\Fonts\\YuGothR.ttc", 0),
        ("C:\\Windows\\Fonts\\malgun.ttf", 0),
        ("C:\\Windows\\Fonts\\simhei.ttf", 0),
        // Noto Sans CJK collections list JP, KR, SC, TC, HK: face 2 is Simplified.
        ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
        ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2),
        ("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 2),
        ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
        ("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", 0),
        (
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            0,
        ),
        ("/System/Library/Fonts/PingFang.ttc", 0),
        ("/System/Library/Fonts/STHeiti Light.ttc", 0),
    ];
    list.extend(fixed.iter().map(|(path, index)| FontFile {
        path: (*path).into(),
        index: *index,
    }));
    list
}

/// Build the font set: Segoe UI / Consolas when present (Windows look), plus
/// the first readable CJK font appended as a FALLBACK to both families, so
/// Latin/Cyrillic text keeps its look and only glyphs missing there (the
/// Chinese UI language, «中文» in the menu) come from the CJK face (A1).
/// Returns the definitions and the path of the CJK font that was added.
pub(super) fn build_fonts(
    latin: &[std::path::PathBuf],
    mono: &[std::path::PathBuf],
    cjk: &[FontFile],
) -> (egui::FontDefinitions, Option<String>) {
    let mut fonts = egui::FontDefinitions::default();
    for path in latin {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "segoe".to_string(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "segoe".to_string());
            }
            break;
        }
    }
    for path in mono {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "consolas".to_string(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "consolas".to_string());
            break;
        }
    }
    let mut loaded = None;
    for candidate in cjk {
        let Ok(bytes) = std::fs::read(&candidate.path) else {
            continue;
        };
        fonts.font_data.insert(
            "cjk".to_string(),
            std::sync::Arc::new(egui::FontData::from_blob(
                std::sync::Arc::new(bytes),
                candidate.index,
            )),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_string());
        }
        loaded = Some(candidate.path.display().to_string());
        break;
    }
    (fonts, loaded)
}

/// Install the application fonts. Returns the CJK fallback font that was
/// found, or `None` when the machine has none (Chinese text then shows as
/// boxes; the caller reports it in the journal).
pub fn install_fonts(ctx: &egui::Context) -> Option<String> {
    let latin = [
        std::path::PathBuf::from("C:\\Windows\\Fonts\\segoeui.ttf"),
        std::path::PathBuf::from("C:\\Windows\\Fonts\\SegoeUI.ttf"),
    ];
    let mono = [std::path::PathBuf::from("C:\\Windows\\Fonts\\consola.ttf")];
    let (fonts, cjk) = build_fonts(&latin, &mono, &cjk_candidates());
    ctx.set_fonts(fonts);
    cjk
}
