//! OPC UA discovery results rendered as a three-level tree:
//!
//! ```text
//! ▼ Server (application name)          URLs: 2 · security options: 3
//!    ▼ opc.tcp://10.0.0.5:4840          <- click: Auto (or the only option)
//!         None                           <- click: exact policy/mode
//!         Basic256Sha256 · Sign
//!         Aes512_Sha999_Hypothetical · … <- disabled: unsupported by client
//!      opc.tcp://127.0.0.1:4840          <- no advertised security: Auto
//! ```
//!
//! Grouping is pure (`group_endpoints`) so it is unit-tested without a UI. The
//! renderer never mutates dialog state; it returns a [`UaTreePick`] that the
//! connect dialog applies.

use super::connect::{security_mode_label, security_policy_label};
use crate::bff::model::{
    UaEndpointInfo, UaSecurityMode, UaSecurityPolicy, UaServerInfo, validate_security_pair,
};
use crate::i18n::{Lang, Tr, fmt, tr};

/// One security configuration advertised for a URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UaSecurityOption {
    /// A pair this client can request.
    Supported {
        policy: UaSecurityPolicy,
        mode: UaSecurityMode,
    },
    /// Advertised by the server but unusable here (e.g. an Aes* policy or an
    /// `Invalid` mode). Shown disabled so the user sees why it is missing.
    Unsupported { policy: String, mode: String },
}

impl UaSecurityOption {
    fn sort_key(&self) -> (u8, usize, usize) {
        match self {
            UaSecurityOption::Supported { policy, mode } => (
                0,
                UaSecurityPolicy::ALL
                    .iter()
                    .position(|p| p == policy)
                    .unwrap_or(usize::MAX),
                UaSecurityMode::ALL
                    .iter()
                    .position(|m| m == mode)
                    .unwrap_or(usize::MAX),
            ),
            UaSecurityOption::Unsupported { .. } => (1, 0, 0),
        }
    }
}

/// One endpoint URL of a server with every security option advertised for it.
/// `options` is empty when the URL came from LDS/mDNS/`DiscoveryUrls` and the
/// server's endpoint descriptions were never fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UaUrlNode {
    pub url: String,
    pub options: Vec<UaSecurityOption>,
}

impl UaUrlNode {
    /// The single supported option, when the URL offers exactly one.
    fn only_supported(&self) -> Option<(UaSecurityPolicy, UaSecurityMode)> {
        let mut supported = self.options.iter().filter_map(|option| match option {
            UaSecurityOption::Supported { policy, mode } => Some((*policy, *mode)),
            UaSecurityOption::Unsupported { .. } => None,
        });
        let first = supported.next()?;
        supported.next().is_none().then_some(first)
    }

    fn offers(&self, policy: UaSecurityPolicy, mode: UaSecurityMode) -> bool {
        self.options
            .iter()
            .any(|option| *option == UaSecurityOption::Supported { policy, mode })
    }
}

/// What the user picked in the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UaTreePick {
    pub url: String,
    pub policy: UaSecurityPolicy,
    pub mode: UaSecurityMode,
    /// Double-click: connect right away with this selection.
    pub connect: bool,
}

/// Case/trailing-slash-insensitive URL identity used for grouping/selection.
pub(crate) fn url_key(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Classify one advertised (policy, mode) pair. `None` means "not advertised"
/// (both strings empty: the URL is known but its security was never fetched).
pub(crate) fn classify_security(endpoint: &UaEndpointInfo) -> Option<UaSecurityOption> {
    let policy_text = endpoint.security_policy.trim();
    let mode_text = endpoint.security_mode.trim();
    if policy_text.is_empty() && mode_text.is_empty() {
        return None;
    }
    let short_policy = policy_text
        .rsplit('#')
        .next()
        .unwrap_or(policy_text)
        .to_string();
    let parsed = if policy_text.is_empty() || mode_text.is_empty() {
        None
    } else {
        UaSecurityPolicy::from_advertised(policy_text)
            .zip(UaSecurityMode::from_advertised(mode_text))
            .filter(|(policy, mode)| {
                *policy != UaSecurityPolicy::Auto
                    && *mode != UaSecurityMode::Auto
                    && validate_security_pair(*policy, *mode).is_ok()
            })
    };
    Some(match parsed {
        Some((policy, mode)) => UaSecurityOption::Supported { policy, mode },
        None => UaSecurityOption::Unsupported {
            policy: short_policy,
            mode: mode_text.to_string(),
        },
    })
}

/// Group a server's flat endpoint list by URL (first-seen order and spelling)
/// and collect the distinct security options of each URL, sorted from the
/// weakest supported option to the strongest with unsupported ones last.
pub(crate) fn group_endpoints(endpoints: &[UaEndpointInfo]) -> Vec<UaUrlNode> {
    let mut nodes: Vec<UaUrlNode> = Vec::new();
    for endpoint in endpoints {
        let url = endpoint.url.trim();
        if url.is_empty() {
            continue;
        }
        let key = url_key(url);
        let index = match nodes.iter().position(|node| url_key(&node.url) == key) {
            Some(index) => index,
            None => {
                nodes.push(UaUrlNode {
                    url: url.to_string(),
                    options: Vec::new(),
                });
                nodes.len() - 1
            }
        };
        if let Some(option) = classify_security(endpoint) {
            if !nodes[index].options.contains(&option) {
                nodes[index].options.push(option);
            }
        }
    }
    for node in &mut nodes {
        node.options.sort_by_key(UaSecurityOption::sort_key);
    }
    nodes
}

/// Selection produced by a click on a URL row: the only supported option when
/// there is exactly one, otherwise `Auto/Auto` (resolved against the server's
/// endpoints at connect time). Never inherits another server's security.
pub(crate) fn pick_for_url(node: &UaUrlNode, connect: bool) -> UaTreePick {
    let (policy, mode) = node
        .only_supported()
        .unwrap_or((UaSecurityPolicy::Auto, UaSecurityMode::Auto));
    UaTreePick {
        url: node.url.clone(),
        policy,
        mode,
        connect,
    }
}

/// Server title: application name, falling back to the URI, then the first URL.
fn server_title(server: &UaServerInfo) -> String {
    let name = server.application_name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let uri = server.application_uri.trim();
    if !uri.is_empty() {
        return uri.to_string();
    }
    server
        .endpoints
        .iter()
        .map(|endpoint| endpoint.url.trim())
        .find(|url| !url.is_empty())
        .unwrap_or("?")
        .to_string()
}

fn option_label(lang: Lang, option: &UaSecurityOption) -> String {
    match option {
        UaSecurityOption::Supported {
            policy: UaSecurityPolicy::None,
            mode,
        } => security_mode_label(lang, *mode).to_string(),
        UaSecurityOption::Supported { policy, mode } => format!(
            "{} · {}",
            security_policy_label(lang, *policy),
            security_mode_label(lang, *mode)
        ),
        UaSecurityOption::Unsupported { policy, mode } => {
            format!("{policy} · {mode} — {}", tr(lang, Tr::UaTreeUnsupported))
        }
    }
}

/// Render the discovery results as a server → URL → security tree. Returns the
/// user's pick for this frame, if any. `epoch` changes on every new search so
/// collapse state never leaks between result sets; tree ids use the row index
/// so servers with an empty or duplicate application URI never clash.
pub(crate) fn ua_server_tree(
    ui: &mut egui::Ui,
    lang: Lang,
    servers: &[UaServerInfo],
    epoch: u64,
    selected_url: &str,
    selected_policy: UaSecurityPolicy,
    selected_mode: UaSecurityMode,
) -> Option<UaTreePick> {
    let mut pick: Option<UaTreePick> = None;
    let selected_key = url_key(selected_url);
    let hint = tr(lang, Tr::DoubleClickServerHint);
    // Few servers: open them all. Many servers: start collapsed so the list
    // stays scannable.
    let open_servers = servers.len() <= 3;

    for (server_index, server) in servers.iter().enumerate() {
        let nodes = group_endpoints(&server.endpoints);
        let option_count: usize = nodes.iter().map(|node| node.options.len()).sum();
        let summary = fmt(
            lang,
            Tr::UaTreeSummary,
            &[&nodes.len().to_string(), &option_count.to_string()],
        );
        let holds_selection = nodes.iter().any(|node| url_key(&node.url) == selected_key);
        let title = egui::RichText::new(server_title(server)).strong();
        let header = egui::CollapsingHeader::new(title)
            .id_salt(("ua_tree_server", epoch, server_index))
            .default_open(open_servers || holds_selection)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&summary).weak().small());
                if nodes.is_empty() {
                    ui.label(egui::RichText::new(tr(lang, Tr::UaTreeNoEndpoints)).weak());
                    return;
                }
                let open_urls = nodes.len() <= 2;
                for (url_index, node) in nodes.iter().enumerate() {
                    let same_url = url_key(&node.url) == selected_key;
                    let exact_leaf = same_url && node.offers(selected_policy, selected_mode);
                    // The URL row is selected when its URL is current and no
                    // leaf of it matches the current policy/mode (Auto etc.).
                    let url_selected = same_url && !exact_leaf;

                    if node.options.is_empty() {
                        // No toggle arrow: indent by the arrow column so every
                        // URL of the server starts in the same column.
                        let response = ui
                            .horizontal(|ui| {
                                ui.add_space(ui.spacing().indent);
                                ui.selectable_label(url_selected, &node.url)
                            })
                            .inner
                            .on_hover_text(format!(
                                "{}\n{}\n{}",
                                node.url,
                                tr(lang, Tr::UaTreeSecurityOnConnect),
                                hint
                            ));
                        if response.double_clicked() {
                            pick = Some(pick_for_url(node, true));
                        } else if response.clicked() {
                            pick = Some(pick_for_url(node, false));
                        }
                        continue;
                    }

                    let id = ui.make_persistent_id(("ua_tree_url", epoch, server_index, url_index));
                    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        id,
                        open_urls || same_url,
                    );
                    let header = state.show_header(ui, |ui| {
                        let response = ui
                            .selectable_label(url_selected, &node.url)
                            .on_hover_text(format!("{}\n{}", node.url, hint));
                        if response.double_clicked() {
                            pick = Some(pick_for_url(node, true));
                        } else if response.clicked() {
                            pick = Some(pick_for_url(node, false));
                        }
                    });
                    header.body(|ui| {
                        for option in &node.options {
                            let label = option_label(lang, option);
                            match option {
                                UaSecurityOption::Supported { policy, mode } => {
                                    let leaf_selected = same_url
                                        && *policy == selected_policy
                                        && *mode == selected_mode;
                                    let response = ui
                                        .selectable_label(leaf_selected, label)
                                        .on_hover_text(format!(
                                            "{}\n{} / {}\n{}",
                                            node.url,
                                            policy.as_str(),
                                            mode.as_str(),
                                            hint
                                        ));
                                    let chosen = UaTreePick {
                                        url: node.url.clone(),
                                        policy: *policy,
                                        mode: *mode,
                                        connect: false,
                                    };
                                    if response.double_clicked() {
                                        pick = Some(UaTreePick {
                                            connect: true,
                                            ..chosen
                                        });
                                    } else if response.clicked() {
                                        pick = Some(chosen);
                                    }
                                }
                                UaSecurityOption::Unsupported { .. } => {
                                    ui.add_enabled(false, egui::Button::selectable(false, label));
                                }
                            }
                        }
                    });
                }
            });
        header
            .header_response
            .on_hover_text(if server.application_uri.trim().is_empty() {
                summary.clone()
            } else {
                format!("{}\n{}", server.application_uri.trim(), summary)
            });
    }
    pick
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(url: &str, policy: &str, mode: &str) -> UaEndpointInfo {
        UaEndpointInfo {
            url: url.to_string(),
            security_policy: policy.to_string(),
            security_mode: mode.to_string(),
        }
    }

    const B256: &str = "http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256";
    const NONE: &str = "http://opcfoundation.org/UA/SecurityPolicy#None";
    const AES: &str = "http://opcfoundation.org/UA/SecurityPolicy#Aes256_Sha256_RsaPss";
    // A policy this client does not implement (the list of supported ones grows:
    // Aes256_Sha256_RsaPss used to stand here and is supported now).
    const FUTURE: &str = "http://opcfoundation.org/UA/SecurityPolicy#Aes512_Sha999_Hypothetical";

    #[test]
    fn groups_by_url_and_keeps_first_spelling() {
        let nodes = group_endpoints(&[
            ep("opc.tcp://Host:4840/", B256, "SignAndEncrypt"),
            ep("opc.tcp://10.0.0.5:4840", NONE, "None"),
            ep("opc.tcp://host:4840", NONE, "None"),
            ep("opc.tcp://host:4840", B256, "Sign"),
        ]);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].url, "opc.tcp://Host:4840/");
        assert_eq!(
            nodes[0].options,
            vec![
                UaSecurityOption::Supported {
                    policy: UaSecurityPolicy::None,
                    mode: UaSecurityMode::None
                },
                UaSecurityOption::Supported {
                    policy: UaSecurityPolicy::Basic256Sha256,
                    mode: UaSecurityMode::Sign
                },
                UaSecurityOption::Supported {
                    policy: UaSecurityPolicy::Basic256Sha256,
                    mode: UaSecurityMode::SignAndEncrypt
                },
            ]
        );
        assert_eq!(nodes[1].url, "opc.tcp://10.0.0.5:4840");
    }

    #[test]
    fn empty_security_is_not_a_none_option() {
        // LDS/mDNS URLs carry no security info. They must not be presented
        // (or picked) as an insecure None/None endpoint.
        let nodes = group_endpoints(&[ep("opc.tcp://lds-only:4840", "", "")]);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].options.is_empty());
        let pick = pick_for_url(&nodes[0], false);
        assert_eq!(pick.policy, UaSecurityPolicy::Auto);
        assert_eq!(pick.mode, UaSecurityMode::Auto);
    }

    #[test]
    fn unknown_url_entry_merges_with_known_options() {
        let nodes = group_endpoints(&[
            ep("opc.tcp://srv:4840", "", ""),
            ep("opc.tcp://srv:4840", B256, "Sign"),
        ]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].options.len(), 1);
    }

    #[test]
    fn unsupported_policies_are_kept_but_never_picked() {
        let nodes = group_endpoints(&[
            ep("opc.tcp://srv:4840", FUTURE, "SignAndEncrypt"),
            ep("opc.tcp://srv:4840", B256, "SignAndEncrypt"),
            ep("opc.tcp://srv:4840", NONE, "Sign"),
            ep("opc.tcp://srv:4840", B256, "Invalid"),
        ]);
        assert_eq!(nodes[0].options.len(), 4);
        assert!(matches!(
            nodes[0].options[0],
            UaSecurityOption::Supported { .. }
        ));
        assert!(
            nodes[0].options[1..]
                .iter()
                .all(|o| matches!(o, UaSecurityOption::Unsupported { .. }))
        );
        // Exactly one usable option: clicking the URL selects it.
        let pick = pick_for_url(&nodes[0], true);
        assert_eq!(pick.policy, UaSecurityPolicy::Basic256Sha256);
        assert_eq!(pick.mode, UaSecurityMode::SignAndEncrypt);
        assert!(pick.connect);
    }

    #[test]
    fn aes_policies_are_offered_as_supported_options() {
        let nodes = group_endpoints(&[
            ep("opc.tcp://srv:4840", AES, "SignAndEncrypt"),
            ep("opc.tcp://srv:4840", FUTURE, "SignAndEncrypt"),
        ]);
        assert_eq!(
            nodes[0].options[0],
            UaSecurityOption::Supported {
                policy: UaSecurityPolicy::Aes256Sha256RsaPss,
                mode: UaSecurityMode::SignAndEncrypt
            }
        );
        assert!(matches!(
            nodes[0].options[1],
            UaSecurityOption::Unsupported { .. }
        ));
    }

    #[test]
    fn url_with_several_options_picks_auto() {
        let nodes = group_endpoints(&[
            ep("opc.tcp://srv:4840", NONE, "None"),
            ep("opc.tcp://srv:4840", B256, "Sign"),
        ]);
        let pick = pick_for_url(&nodes[0], false);
        assert_eq!(pick.url, "opc.tcp://srv:4840");
        assert_eq!(pick.policy, UaSecurityPolicy::Auto);
        assert_eq!(pick.mode, UaSecurityMode::Auto);
    }

    #[test]
    fn short_policy_names_are_accepted() {
        let option = classify_security(&ep("opc.tcp://srv", "None", "None"));
        assert_eq!(
            option,
            Some(UaSecurityOption::Supported {
                policy: UaSecurityPolicy::None,
                mode: UaSecurityMode::None
            })
        );
    }

    #[test]
    fn server_title_falls_back_to_uri_then_url() {
        let mut server = UaServerInfo {
            application_name: String::new(),
            application_uri: "urn:x".to_string(),
            endpoints: vec![ep("opc.tcp://a:1", "", "")],
        };
        assert_eq!(server_title(&server), "urn:x");
        server.application_uri.clear();
        assert_eq!(server_title(&server), "opc.tcp://a:1");
    }
}
