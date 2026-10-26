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

/// True when `host` refers to a machine other than this one (not 127.0.0.1 /
/// localhost / empty local DA host).
fn is_remote_host(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    !h.is_empty() && h != "127.0.0.1" && h != "localhost"
}

/// The `host:port` portion of an `opc.tcp://host:port` URL (or the host when no
/// port is present).
fn ua_host_port(url: &str) -> String {
    match ua_url_host(url) {
        Some(host) if !host.is_empty() => {
            let port = ua_url_port(url);
            if let Some(port) = port {
                format!("{host}:{port}")
            } else {
                host
            }
        }
        _ => url.to_string(),
    }
}

pub(super) fn ua_url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("opc.tcp://")
        .or_else(|| url.strip_prefix("opc.tcp:"))
        .unwrap_or(url);
    let host = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    let host = host.trim().to_string();
    if host.is_empty() { None } else { Some(host) }
}

fn ua_url_port(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("opc.tcp://")
        .or_else(|| url.strip_prefix("opc.tcp:"))
        .unwrap_or(url);
    let host_port = rest.split('/').next().unwrap_or("");
    let mut parts = host_port.split(':');
    let host = parts.next();
    let port = parts.next();
    if let (Some(_), Some(p)) = (host, port) {
        Some(p.to_string())
    } else {
        None
    }
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
/// physical adapter's local address.
pub(super) fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]"
}

pub(super) fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\SegoeUI.ttf",
    ] {
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
    if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\consola.ttf") {
        fonts.font_data.insert(
            "consolas".to_string(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "consolas".to_string());
    }
    ctx.set_fonts(fonts);
}
