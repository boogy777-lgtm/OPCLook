use super::super::colors::{red, yellow};
use super::dialog_spacing;
use super::network::network_settings_view;
use crate::app::App;
use crate::bff::model::{Protocol, UaSecurityMode, UaSecurityPolicy};
use crate::constants::{
    DEFAULT_UA_PORT, MAX_UA_SESSIONS, SESSION_TIMEOUT_MAX_MS, SESSION_TIMEOUT_MIN_MS,
};
use crate::i18n::{Lang, Tr, fmt, tr};
use crate::net::AdapterSelection;

/// Localized label for a typed security policy. Exhaustive: a new enum variant
/// cannot be added without a UI label.
pub(super) fn security_policy_label(lang: Lang, policy: UaSecurityPolicy) -> &'static str {
    match policy {
        UaSecurityPolicy::Auto => tr(lang, Tr::AutoLabel),
        UaSecurityPolicy::None => tr(lang, Tr::SecPolicyNone),
        UaSecurityPolicy::Basic128Rsa15 => tr(lang, Tr::SecPolicyBasic128Rsa15),
        UaSecurityPolicy::Basic256 => tr(lang, Tr::SecPolicyBasic256),
        UaSecurityPolicy::Basic256Sha256 => tr(lang, Tr::SecPolicyBasic256Sha256),
    }
}

/// Localized label for a typed security mode. Exhaustive like the policy one.
pub(super) fn security_mode_label(lang: Lang, mode: UaSecurityMode) -> &'static str {
    match mode {
        UaSecurityMode::Auto => tr(lang, Tr::AutoLabel),
        UaSecurityMode::None => tr(lang, Tr::SecModeNone),
        UaSecurityMode::Sign => tr(lang, Tr::SecModeSign),
        UaSecurityMode::SignAndEncrypt => tr(lang, Tr::SecModeSignAndEncrypt),
    }
}
pub(super) fn connect_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.connect.open {
        return;
    }
    let lang = app.lang();
    let mut refresh: Option<String> = None;
    let mut connect: Option<Protocol> = None;
    let mut quick_connect: Option<crate::bff::model::EndpointSpec> = None;
    let mut close = false;
    let mut app_search = false;
    let mut app_network_search = false;
    let mut app_subnet_search = false;
    let mut app_subnet_cancel = false;
    let mut app_subnet_confirm = false;
    let mut security_changed = false;
    let network_adapters = app.network.adapters.clone();
    let network_refresh_failed = app.network.refresh_failed;
    let ua_session_count = app.ua_session_count();
    let preferred_selection = app.settings.adapter_selection();
    let preferred_ip = app.settings.selected_local_ip();
    let mut network_refresh = false;
    let mut network_action: Option<(AdapterSelection, Option<std::net::IpAddr>)> = None;

    {
        let dialog = &mut app.dialogs.connect;
        let recent = app.settings.recent_endpoints.clone();
        egui::Window::new(tr(lang, Tr::ConnectToServer))
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(&ctx, |ui| {
                dialog_spacing(ui);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                let opening_frame = dialog.focus_pending;
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            !dialog.network_tab && dialog.protocol == Protocol::OpcDa,
                            tr(lang, Tr::DaTab),
                        )
                        .clicked()
                    {
                        let scan_active =
                            dialog.ua_subnet_searching || dialog.ua_scan_confirm.is_some();
                        dialog.protocol = Protocol::OpcDa;
                        dialog.network_tab = false;
                        if scan_active {
                            app_subnet_cancel = true;
                        }
                    }
                    if ui
                        .selectable_label(
                            !dialog.network_tab && dialog.protocol == Protocol::OpcUa,
                            tr(lang, Tr::UaTab),
                        )
                        .clicked()
                    {
                        let leaving_network = dialog.network_tab;
                        let previous_protocol = dialog.protocol;
                        dialog.protocol = Protocol::OpcUa;
                        dialog.network_tab = false;
                        // Clicking the already active UA tab must not stop its
                        // scan; leaving UA (or a protocol switch) does.
                        if (leaving_network || previous_protocol != Protocol::OpcUa)
                            && (dialog.ua_subnet_searching || dialog.ua_scan_confirm.is_some())
                        {
                            app_subnet_cancel = true;
                        }
                    }
                    if ui
                        .selectable_label(dialog.network_tab, tr(lang, Tr::NetworkTab))
                        .clicked()
                    {
                        dialog.network_tab = true;
                        if dialog.ua_subnet_searching || dialog.ua_scan_confirm.is_some() {
                            app_subnet_cancel = true;
                        }
                    }
                });
                ui.separator();

                let is_network = dialog.network_tab;
                let is_da = !is_network && dialog.protocol == Protocol::OpcDa;
                let is_ua = !is_network && dialog.protocol == Protocol::OpcUa;
                let ua_busy = dialog.ua_searching
                    || dialog.ua_network_searching
                    || dialog.ua_subnet_searching
                    || dialog.ua_scan_confirm.is_some();

                if is_network {
                    network_settings_view(
                        ui,
                        lang,
                        &network_adapters,
                        network_refresh_failed,
                        &preferred_selection,
                        preferred_ip,
                        &mut network_refresh,
                        &mut network_action,
                    );
                    if esc && !opening_frame {
                        close = true;
                    }
                    ui.separator();
                    if ui.button(tr(lang, Tr::Close)).clicked() {
                        close = true;
                    }
                } else {
                    // 1. List of servers (DA) / search results (UA).
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(if is_da {
                                tr(lang, Tr::DaServersList)
                            } else {
                                tr(lang, Tr::FoundServers)
                            })
                            .strong(),
                        );
                        if !is_da {
                            ui.separator();
                            ui.label(fmt(
                                lang,
                                Tr::UaSessionCounter,
                                &[&ua_session_count.to_string(), &MAX_UA_SESSIONS.to_string()],
                            ));
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt(if is_da {
                            "server_list"
                        } else {
                            "ua_server_list"
                        })
                        .max_height(150.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if is_da {
                                if dialog.da_pending {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(tr(lang, Tr::PollingServers));
                                    });
                                } else if dialog.da_servers.is_empty() {
                                    ui.label(tr(lang, Tr::ListEmptyManual));
                                }
                                for server in &dialog.da_servers {
                                    let selected =
                                        dialog.da_selected.as_deref() == Some(server.id.as_str());
                                    let label = if server.description.is_empty() {
                                        server.id.clone()
                                    } else {
                                        format!("{} — {}", server.id, server.description)
                                    };
                                    let hint = format!(
                                        "{}\n{}",
                                        server.id,
                                        tr(lang, Tr::DoubleClickServerHint)
                                    );
                                    let response =
                                        ui.selectable_label(selected, label).on_hover_text(&hint);
                                    if response.double_clicked() {
                                        let host = dialog.da_host.clone();
                                        let mut ep = crate::bff::model::EndpointSpec::da(
                                            host,
                                            server.id.clone(),
                                        );
                                        ep = ep.with_advertised_name(da_advertised_name(server));
                                        quick_connect = Some(ep);
                                    } else if response.clicked() {
                                        dialog.da_selected = Some(server.id.clone());
                                        dialog.da_manual = server.id.clone();
                                    }
                                }
                            } else if dialog.ua_searching {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(tr(lang, Tr::SearchingUa));
                                });
                            } else if dialog.ua_search_results.is_empty() {
                                ui.label(tr(lang, Tr::ListEmptyUrl));
                            }
                            if !is_da {
                                let identity_ready = dialog.ua_identity_index != 1
                                    || !dialog.ua_user.trim().is_empty();
                                let mut identity_error = false;
                                for server in &dialog.ua_search_results {
                                    let title = if server.application_name.is_empty() {
                                        server.application_uri.clone()
                                    } else {
                                        format!(
                                            "{} — {}",
                                            server.application_name, server.application_uri
                                        )
                                    };
                                    for endpoint in &server.endpoints {
                                        let label = if endpoint.url.is_empty() {
                                            title.clone()
                                        } else {
                                            let sec = if endpoint.security_policy.is_empty() {
                                                String::new()
                                            } else {
                                                format!(
                                                    " | {} / {}",
                                                    endpoint.security_policy,
                                                    endpoint.security_mode
                                                )
                                            };
                                            format!("{}{sec}", endpoint.url)
                                        };
                                        let hint = format!(
                                            "{}\n{}",
                                            endpoint.url,
                                            tr(lang, Tr::DoubleClickServerHint)
                                        );
                                        let response = ui
                                            .selectable_label(dialog.ua_url == endpoint.url, label)
                                            .on_hover_text(&hint);
                                        if response.double_clicked() {
                                            if identity_ready {
                                                let mut ep = crate::bff::model::EndpointSpec::ua(
                                                    endpoint.url.clone(),
                                                );
                                                ep = ep.with_advertised_name(Some(
                                                    server.application_name.clone(),
                                                ));
                                                ep.ua_security_policy =
                                                    Some(dialog.ua_security_policy);
                                                ep.ua_security_mode = Some(dialog.ua_security_mode);
                                                ep.ua_trust_server_cert =
                                                    dialog.ua_trust_server_cert;
                                                ep.ua_session_timeout =
                                                    Some(dialog.ua_session_timeout);
                                                if dialog.ua_identity_index == 1 {
                                                    ep.user = dialog.ua_user.trim().to_string();
                                                    ep.password = dialog.ua_password.clone();
                                                } else if dialog.ua_identity_index == 2 {
                                                    ep.ua_use_certificate = true;
                                                }
                                                quick_connect = Some(ep);
                                            } else {
                                                identity_error = true;
                                            }
                                        } else if response.clicked() {
                                            dialog.ua_url = endpoint.url.clone();
                                        }
                                    }
                                }
                                if identity_error {
                                    dialog.error =
                                        Some(tr(lang, Tr::UaUserNameRequired).to_string());
                                }
                            }
                        });

                    ui.add_space(4.0);

                    // 2. Address (host) + 3. Port (UA only) + 4. Search button.
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(if is_da {
                                tr(lang, Tr::NetworkAddr)
                            } else {
                                tr(lang, Tr::AddressLabel)
                            })
                            .weak(),
                        );
                        let host = ui.add(
                            egui::TextEdit::singleline(if is_da {
                                &mut dialog.da_host
                            } else {
                                &mut dialog.ua_search_host
                            })
                            .hint_text(if is_da {
                                tr(lang, Tr::HintLocalAddr)
                            } else {
                                tr(lang, Tr::UaHostHint)
                            })
                            .desired_width(160.0),
                        );
                        if dialog.focus_pending && is_da {
                            host.request_focus();
                        }
                        if !is_da {
                            ui.label(egui::RichText::new(tr(lang, Tr::PortLabel)).weak());
                            ui.add(
                                egui::TextEdit::singleline(&mut dialog.ua_search_port)
                                    .hint_text(tr(lang, Tr::UaPortHint))
                                    .desired_width(60.0),
                            );
                        }
                        let searching = if is_da {
                            dialog.da_pending
                        } else {
                            dialog.ua_searching
                        };
                        let search_label = if is_da {
                            tr(lang, Tr::SearchLan)
                        } else {
                            tr(lang, Tr::Search)
                        };
                        if ui
                            .add_enabled(
                                !searching && !(is_ua && ua_busy),
                                egui::Button::new(search_label),
                            )
                            .clicked()
                        {
                            if is_da {
                                refresh = Some(dialog.da_host.clone());
                            } else {
                                app_search = true;
                            }
                        }
                        if searching {
                            ui.spinner();
                        }
                    });

                    // LDS/mDNS and subnet scan are separate actions; keeping
                    // them on their own row avoids overflowing the 560 px
                    // non-resizable connect window.
                    if !is_da {
                        ui.horizontal(|ui| {
                            // «Поиск через LDS/mDNS» (`FindServersOnNetwork` + DNS-SD).
                            let net_searching = dialog.ua_network_searching;
                            if ui
                                .add_enabled(
                                    !ua_busy,
                                    egui::Button::new(tr(lang, Tr::SearchLdsMdns)),
                                )
                                .clicked()
                            {
                                app_network_search = true;
                            }
                            if net_searching {
                                ui.spinner();
                            }
                            // «Поиск в сети» — N+1 subnet scanner (loopback first).
                            if ui
                                .add_enabled(
                                    !ua_busy,
                                    egui::Button::new(tr(lang, Tr::SearchUaSubnet)),
                                )
                                .clicked()
                            {
                                app_subnet_search = true;
                            }
                        });
                    }

                    if !is_da {
                        if dialog.ua_subnet_searching {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(fmt(
                                    lang,
                                    Tr::UaScanProgress,
                                    &[
                                        &dialog.ua_scan_checked.to_string(),
                                        &dialog.ua_scan_total.to_string(),
                                        &dialog.ua_scan_found.to_string(),
                                    ],
                                ));
                                if ui.button(tr(lang, Tr::UaScanStop)).clicked() {
                                    app_subnet_cancel = true;
                                }
                            });
                        }
                        if let Some(count) = dialog.ua_scan_confirm {
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    yellow(),
                                    fmt(
                                        lang,
                                        Tr::UaScanConfirmText,
                                        &[
                                            &count.to_string(),
                                            &crate::constants::SCAN_CONFIRM_THRESHOLD.to_string(),
                                        ],
                                    ),
                                );
                                if ui.button(tr(lang, Tr::Yes)).clicked() {
                                    app_subnet_confirm = true;
                                }
                                if ui.button(tr(lang, Tr::Cancel)).clicked() {
                                    app_subnet_cancel = true;
                                }
                            });
                        }
                        if dialog.ua_scan_confirm.is_none() {
                            if let Some(error) = &dialog.ua_scan_error {
                                ui.colored_label(red(), error);
                            }
                        }
                    }

                    // Reference hint: an empty address searches this computer + the
                    // local network (both protocols).
                    let empty_host = if is_da {
                        dialog.da_host.trim().is_empty()
                    } else {
                        dialog.ua_search_host.trim().is_empty()
                    };
                    if empty_host {
                        ui.label(egui::RichText::new(tr(lang, Tr::EmptySearchHint)).weak());
                    }

                    ui.add_space(4.0);

                    // 5. DA credentials. UA credentials are shown only for the
                    // UserName identity, after the identity selector below.
                    if is_da {
                        user_password_ui(ui, lang, &mut dialog.ua_user, &mut dialog.ua_password);
                    }

                    if !is_da {
                        ui.add_space(4.0);
                        // UA security controls: policy + mode combos, trust checkbox,
                        // identity combo and session timeout.
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(tr(lang, Tr::SecPolicyLabel)).weak());
                            let before = dialog.ua_security_policy;
                            egui::ComboBox::from_id_salt("ua_sec_policy")
                                .selected_text(security_policy_label(lang, before))
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    for p in UaSecurityPolicy::ALL {
                                        ui.selectable_value(
                                            &mut dialog.ua_security_policy,
                                            p,
                                            security_policy_label(lang, p),
                                        );
                                    }
                                });
                            if dialog.ua_security_policy != before {
                                security_changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(tr(lang, Tr::SecModeLabel)).weak());
                            let before = dialog.ua_security_mode;
                            egui::ComboBox::from_id_salt("ua_sec_mode")
                                .selected_text(security_mode_label(lang, before))
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    for m in UaSecurityMode::ALL {
                                        ui.selectable_value(
                                            &mut dialog.ua_security_mode,
                                            m,
                                            security_mode_label(lang, m),
                                        );
                                    }
                                });
                            if dialog.ua_security_mode != before {
                                security_changed = true;
                            }
                        });
                        ui.checkbox(
                            &mut dialog.ua_trust_server_cert,
                            tr(lang, Tr::TrustServerCertCheck),
                        );
                        let cert_supported = {
                            let d = &*dialog;
                            d.ua_identity_index == 2
                                || d.ua_discovered_endpoints.iter().any(|e| {
                                    !matches!(
                                        UaSecurityMode::from_advertised(&e.security_mode),
                                        Some(UaSecurityMode::None)
                                    )
                                })
                        };
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(tr(lang, Tr::IdentityLabel)).weak());
                            let mut identity = dialog.ua_identity_index;
                            if !cert_supported && identity >= 2 {
                                identity = 1;
                            }
                            egui::ComboBox::from_id_salt("ua_identity")
                                .selected_text(match identity {
                                    0 => tr(lang, Tr::IdentityAnonymous),
                                    1 => tr(lang, Tr::IdentityUserName),
                                    _ => tr(lang, Tr::IdentityCertificate),
                                })
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut identity,
                                        0,
                                        tr(lang, Tr::IdentityAnonymous),
                                    );
                                    ui.selectable_value(
                                        &mut identity,
                                        1,
                                        tr(lang, Tr::IdentityUserName),
                                    );
                                    ui.add_enabled_ui(cert_supported, |ui| {
                                        ui.selectable_value(
                                            &mut identity,
                                            2,
                                            tr(lang, Tr::IdentityCertificate),
                                        );
                                    });
                                });
                            dialog.ua_identity_index = identity;
                        });
                        if !cert_supported && dialog.ua_identity_index >= 2 {
                            dialog.ua_identity_index = 1;
                        }
                        if dialog.ua_identity_index == 1 {
                            user_password_ui(
                                ui,
                                lang,
                                &mut dialog.ua_user,
                                &mut dialog.ua_password,
                            );
                        }
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(tr(lang, Tr::SessionTimeoutLabel)).weak());
                            ui.add(
                                egui::DragValue::new(&mut dialog.ua_session_timeout)
                                    .range(SESSION_TIMEOUT_MIN_MS..=SESSION_TIMEOUT_MAX_MS),
                            );
                        });
                        ui.label(egui::RichText::new(tr(lang, Tr::SessionTimeoutInfo)).weak());
                        ui.add_space(4.0);
                    }

                    // 6. URL / строка подключения.
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr(lang, Tr::UrlLabel)).weak());
                        let url = ui.add(
                            egui::TextEdit::singleline(if is_da {
                                &mut dialog.da_manual
                            } else {
                                &mut dialog.ua_url
                            })
                            .hint_text(if is_da {
                                tr(lang, Tr::HintProgId)
                            } else {
                                tr(lang, Tr::HintUaUrl)
                            })
                            .desired_width(380.0),
                        );
                        if dialog.focus_pending && !is_da {
                            url.request_focus();
                        }
                    });

                    if dialog.focus_pending {
                        dialog.focus_pending = false;
                    }
                    if !recent.is_empty() {
                        ui.collapsing(tr(lang, Tr::Recent), |ui| {
                            for endpoint in recent.iter().filter(|e| {
                                if is_da {
                                    e.protocol == Protocol::OpcDa
                                } else {
                                    e.protocol == Protocol::OpcUa
                                }
                            }) {
                                if ui.button(endpoint.display_name()).clicked() {
                                    match endpoint.protocol {
                                        Protocol::OpcDa => {
                                            dialog.da_host = endpoint.host.clone();
                                            dialog.da_manual = endpoint.address.clone();
                                        }
                                        Protocol::OpcUa => {
                                            dialog.ua_url = endpoint.address.clone();
                                        }
                                    }
                                }
                            }
                        });
                    }

                    if let Some(error) = &dialog.error {
                        ui.colored_label(red(), error);
                    }
                    ui.separator();
                    let can_connect_dialog = {
                        let dialog = &dialog;
                        match dialog.protocol {
                            Protocol::OpcDa => {
                                !dialog.da_manual.trim().is_empty() || dialog.da_selected.is_some()
                            }
                            Protocol::OpcUa => !dialog.ua_url.trim().is_empty(),
                        }
                    };
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                can_connect_dialog,
                                egui::Button::new(tr(lang, Tr::Connect)),
                            )
                            .clicked()
                        {
                            connect = Some(dialog.protocol);
                        }
                        if ui.button(tr(lang, Tr::Cancel)).clicked() {
                            close = true;
                        }
                    });
                    if enter && can_connect_dialog && !opening_frame {
                        connect = Some(dialog.protocol);
                    } else if esc && !opening_frame {
                        close = true;
                    }
                }
            });
    }

    if network_refresh {
        app.refresh_network();
        ctx.request_repaint();
    }
    if let Some((selection, local_ip)) = network_action {
        app.apply_network_selection(selection, local_ip);
        ctx.request_repaint();
    }

    if security_changed {
        // Typed combos are the source of truth: persist the selection and
        // re-validate against the already-discovered endpoints (when any).
        // An invalid pair is reported immediately instead of only at connect.
        app.settings.ua_security_policy = app.dialogs.connect.ua_security_policy;
        app.settings.ua_security_mode = app.dialogs.connect.ua_security_mode;
        app.settings.save();
        app.validate_ua_security();
    }
    if let Some(host) = refresh {
        if host.trim().is_empty() {
            app.discover_da_all();
        } else {
            app.discover_da(host);
        }
    }
    if app_search {
        // Auto-compose the editable URL from the search host:port.
        let host = app.dialogs.connect.ua_search_host.trim().to_string();
        let port = app.dialogs.connect.ua_search_port.trim().to_string();
        if !host.is_empty() {
            app.dialogs.connect.ua_url = ua_search_url(&host, &port);
        }
        app.search_ua_servers();
    }
    if app_network_search {
        app.search_ua_network();
    }
    if app_subnet_cancel {
        app.cancel_ua_subnet_scan();
    }
    if app_subnet_confirm {
        app.confirm_ua_subnet_scan();
    }
    if app_subnet_search {
        app.start_ua_subnet_scan();
    }
    if close {
        app.close_connect_dialog();
    }
    if let Some(protocol) = connect {
        if protocol == Protocol::OpcUa
            && (!app.validate_ua_security() || !app.validate_ua_identity())
        {
            // Invalid security or a UserName identity without a user name:
            // keep the dialog open with the localized error instead of silently
            // connecting with different credentials.
            return;
        }
        let endpoint = match protocol {
            Protocol::OpcDa => {
                let dialog = &app.dialogs.connect;
                let address = if !dialog.da_manual.trim().is_empty() {
                    dialog.da_manual.trim().to_string()
                } else {
                    dialog.da_selected.clone().unwrap_or_default()
                };
                // Advertised name (description + ProgID) when the user picked a
                // discovered server; None for a manually typed ProgID.
                let advertised = dialog
                    .da_servers
                    .iter()
                    .find(|s| {
                        s.id == address || dialog.da_selected.as_deref() == Some(s.id.as_str())
                    })
                    .and_then(da_advertised_name);
                let mut ep =
                    crate::bff::model::EndpointSpec::da(dialog.da_host.trim().to_string(), address);
                ep = ep.with_advertised_name(advertised);
                Some(ep)
            }
            Protocol::OpcUa => {
                let dialog = &app.dialogs.connect;
                if dialog.ua_url.trim().is_empty() {
                    None
                } else {
                    let mut endpoint =
                        crate::bff::model::EndpointSpec::ua(dialog.ua_url.trim().to_string());
                    // Advertised application name when a discovered endpoint was
                    // selected; None for a manually typed URL (host:port is used).
                    let advertised = dialog
                        .ua_search_results
                        .iter()
                        .find(|s| s.endpoints.iter().any(|e| e.url == endpoint.address))
                        .and_then(|s| {
                            if s.application_name.trim().is_empty() {
                                None
                            } else {
                                Some(s.application_name.clone())
                            }
                        });
                    endpoint = endpoint.with_advertised_name(advertised);
                    endpoint.ua_security_policy = Some(dialog.ua_security_policy);
                    endpoint.ua_security_mode = Some(dialog.ua_security_mode);
                    endpoint.ua_trust_server_cert = dialog.ua_trust_server_cert;
                    // Identity selector: 0 = Anonymous, 1 = UserName, 2 = X509.
                    // Anonymous must not inherit credentials typed earlier.
                    if dialog.ua_identity_index == 1 {
                        endpoint.user = dialog.ua_user.trim().to_string();
                        endpoint.password = dialog.ua_password.clone();
                    } else if dialog.ua_identity_index == 2 {
                        endpoint.ua_use_certificate = true;
                    }
                    endpoint.ua_session_timeout = Some(dialog.ua_session_timeout);
                    Some(endpoint)
                }
            }
        };
        if let Some(endpoint) = endpoint {
            if !endpoint.address.trim().is_empty() {
                if endpoint.protocol == Protocol::OpcUa {
                    // Persist the dialog's security/trust/timeout choices so
                    // they survive restarts.
                    app.settings.ua_security_policy =
                        endpoint.ua_security_policy.unwrap_or_default();
                    app.settings.ua_security_mode = endpoint.ua_security_mode.unwrap_or_default();
                    app.settings.ua_trust_server_cert = endpoint.ua_trust_server_cert;
                    app.settings.ua_session_timeout_ms = endpoint
                        .ua_session_timeout
                        .unwrap_or_else(crate::settings::default_session_timeout);
                    app.settings.save();
                }
                app.connect_endpoint(endpoint);
                app.close_connect_dialog();
            } else {
                app.dialogs.connect.error = Some(tr(lang, Tr::ValueNotServer).to_string());
            }
        }
    }
    // Double-click on a server row or UA result: connect (if not yet connected)
    // AND open/activate the tab. Reusing an existing container reactivates it
    // without creating a duplicate session.
    if let Some(endpoint) = quick_connect {
        if !endpoint.address.trim().is_empty() {
            if endpoint.protocol == Protocol::OpcUa
                && (!app.validate_ua_security() || !app.validate_ua_identity())
            {
                return;
            }
            app.open_tab_for_endpoint(endpoint);
            app.close_connect_dialog();
        }
    }
}
/// Build a valid OPC UA URL from the search fields. An empty/invalid port
/// falls back to 4840 (the same value the search command uses), and a bare
/// IPv6 literal is bracketed.
fn ua_search_url(host: &str, port_text: &str) -> String {
    let port = port_text
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .unwrap_or(DEFAULT_UA_PORT);
    let host = host.trim();
    let host = if host.starts_with('[') || !host.contains(':') {
        host.to_string()
    } else {
        format!("[{host}]")
    };
    format!("opc.tcp://{host}:{port}")
}

fn user_password_ui(ui: &mut egui::Ui, lang: Lang, user: &mut String, password: &mut String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::UserLabel)).weak());
        ui.add(egui::TextEdit::singleline(user).desired_width(180.0));
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::PasswordLabel)).weak());
        ui.add(
            egui::TextEdit::singleline(password)
                .password(true)
                .desired_width(180.0),
        );
    });
}

/// Compose the advertised display name for a discovered OPC DA server:
/// the ProgID with a description suffix (when the server provides one).
pub(super) fn da_advertised_name(server: &crate::bff::model::ServerEntry) -> Option<String> {
    if server.id.trim().is_empty() {
        return None;
    }
    if server.description.trim().is_empty() {
        Some(server.id.clone())
    } else {
        Some(format!("{} — {}", server.id, server.description))
    }
}

#[cfg(test)]
mod tests {
    use super::ua_search_url;

    #[test]
    fn search_url_uses_default_port_when_empty_or_invalid() {
        assert_eq!(ua_search_url("127.0.0.1", ""), "opc.tcp://127.0.0.1:4840");
        assert_eq!(
            ua_search_url("127.0.0.1", "not-a-port"),
            "opc.tcp://127.0.0.1:4840"
        );
        assert_eq!(
            ua_search_url("127.0.0.1", "5000"),
            "opc.tcp://127.0.0.1:5000"
        );
    }

    #[test]
    fn search_url_brackets_bare_ipv6() {
        assert_eq!(ua_search_url("::1", ""), "opc.tcp://[::1]:4840");
        assert_eq!(ua_search_url("[::1]", ""), "opc.tcp://[::1]:4840");
    }
}
