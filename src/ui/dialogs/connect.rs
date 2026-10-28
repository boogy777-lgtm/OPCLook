use super::super::colors::{red, yellow};
use super::dialog_spacing;
use super::network::network_settings_view;
use crate::app::App;
use crate::app::{ConnectPane, UaIdentity};
use crate::bff::model::{Protocol, UaSecurityMode, UaSecurityPolicy};
use crate::constants::{
    DEFAULT_UA_PORT, MAX_UA_SESSIONS, SESSION_TIMEOUT_MAX_MS, SESSION_TIMEOUT_MIN_MS,
};
use crate::i18n::{Lang, Tr, fmt, tr};
use crate::net::AdapterSelection;

/// Localized label for a typed security policy. Exhaustive: a new enum variant
/// cannot be added without a UI label.
pub(in crate::ui) fn security_policy_label(lang: Lang, policy: UaSecurityPolicy) -> &'static str {
    match policy {
        UaSecurityPolicy::Auto => tr(lang, Tr::AutoLabel),
        UaSecurityPolicy::None => tr(lang, Tr::SecPolicyNone),
        UaSecurityPolicy::Basic128Rsa15 => tr(lang, Tr::SecPolicyBasic128Rsa15),
        UaSecurityPolicy::Basic256 => tr(lang, Tr::SecPolicyBasic256),
        UaSecurityPolicy::Basic256Sha256 => tr(lang, Tr::SecPolicyBasic256Sha256),
        UaSecurityPolicy::Aes128Sha256RsaOaep => tr(lang, Tr::SecPolicyAes128Sha256RsaOaep),
        UaSecurityPolicy::Aes256Sha256RsaPss => tr(lang, Tr::SecPolicyAes256Sha256RsaPss),
    }
}

/// Localized label for a typed security mode. Exhaustive like the policy one.
pub(in crate::ui) fn security_mode_label(lang: Lang, mode: UaSecurityMode) -> &'static str {
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
                let mut focus_in_search_field = false;
                let mut focus_in_lds_field = false;
                let mut focus_in_url_field = false;
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(dialog.pane == ConnectPane::Da, tr(lang, Tr::DaTab))
                        .clicked()
                    {
                        let scan_active = dialog.ua_search.subnet_active();
                        dialog.pane = ConnectPane::Da;
                        if scan_active {
                            app_subnet_cancel = true;
                        }
                    }
                    if ui
                        .selectable_label(dialog.pane == ConnectPane::Ua, tr(lang, Tr::UaTab))
                        .clicked()
                    {
                        let leaving_network = matches!(dialog.pane, ConnectPane::Network { .. });
                        let previous_protocol = dialog.pane.protocol();
                        dialog.pane = ConnectPane::Ua;
                        // Clicking the already active UA tab must not stop its
                        // scan; leaving UA (or a protocol switch) does.
                        if (leaving_network || previous_protocol != Protocol::OpcUa)
                            && (dialog.ua_search.subnet_active())
                        {
                            app_subnet_cancel = true;
                        }
                    }
                    if ui
                        .selectable_label(
                            matches!(dialog.pane, ConnectPane::Network { .. }),
                            tr(lang, Tr::NetworkTab),
                        )
                        .clicked()
                    {
                        dialog.pane = ConnectPane::Network {
                            previous: dialog.pane.protocol(),
                        };
                        if dialog.ua_search.subnet_active() {
                            app_subnet_cancel = true;
                        }
                    }
                });
                ui.separator();

                let is_network = matches!(dialog.pane, ConnectPane::Network { .. });
                let is_da = dialog.pane == ConnectPane::Da;
                let ua_busy = dialog.ua_search.busy();

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
                        // The UA tree has three levels; give it more room.
                        .max_height(if is_da { 150.0 } else { 240.0 })
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
                                for hit in &dialog.da_servers {
                                    let server = &hit.server;
                                    let host = &hit.host;
                                    let selected =
                                        dialog.da_selected.as_ref().is_some_and(|selected| {
                                            selected.server.id == server.id
                                                && selected.host == *host
                                        });
                                    let label = if server.description.is_empty() {
                                        server.id.clone()
                                    } else {
                                        format!("{} — {}", server.id, server.description)
                                    };
                                    let label = if host.is_empty() {
                                        label
                                    } else {
                                        format!("{label} @ {host}")
                                    };
                                    let hint = format!(
                                        "{}\n{}",
                                        server.id,
                                        tr(lang, Tr::DoubleClickServerHint)
                                    );
                                    let response =
                                        ui.selectable_label(selected, label).on_hover_text(&hint);
                                    if response.double_clicked() {
                                        let mut ep = crate::bff::model::EndpointSpec::da(
                                            host.clone(),
                                            server.id.clone(),
                                        );
                                        ep = ep.with_advertised_name(da_advertised_name(server));
                                        quick_connect = Some(ep);
                                    } else if response.clicked() {
                                        dialog.da_selected = Some(hit.clone());
                                        dialog.da_host = host.clone();
                                        dialog.da_manual = server.id.clone();
                                    }
                                }
                            } else if dialog.ua_search.host_searching() {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(tr(lang, Tr::SearchingUa));
                                });
                            } else if dialog.ua_search_results().is_empty() {
                                ui.label(tr(lang, Tr::ListEmptyUrl));
                            }
                            if !is_da {
                                // Server → URL → security tree. A pick never
                                // inherits another row's security: URLs without
                                // advertised security select Auto/Auto.
                                if let Some(pick) = super::ua_tree::ua_server_tree(
                                    ui,
                                    lang,
                                    dialog.ua_search_results(),
                                    dialog.ua_result_epoch,
                                    dialog.ua_results_cache_id,
                                    dialog.ua_results_revision,
                                    &dialog.ua_url,
                                    dialog.ua_security_policy,
                                    dialog.ua_security_mode,
                                ) {
                                    dialog.ua_url = pick.url;
                                    dialog.ua_security_policy = pick.policy;
                                    dialog.ua_security_mode = pick.mode;
                                    dialog.error = None;
                                    if pick.connect {
                                        // Double-click = select + «Подключиться»:
                                        // the same validation/dedup path as the
                                        // button (identity, security, UA cap).
                                        connect = Some(Protocol::OpcUa);
                                    }
                                }
                            }
                        });

                    ui.add_space(4.0);

                    // 2. Host + 3. UA port; one UA search action below.
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
                        focus_in_search_field = host.has_focus();
                        if is_da && host.changed() {
                            // A selected server belongs to its original host;
                            // never reuse that selection on another PC. Keep a
                            // manually typed ProgID when the host is edited.
                            if dialog
                                .da_selected
                                .as_ref()
                                .is_some_and(|hit| hit.server.id == dialog.da_manual)
                            {
                                dialog.da_manual.clear();
                            }
                            dialog.da_selected = None;
                        }
                        if !is_da {
                            ui.label(egui::RichText::new(tr(lang, Tr::PortLabel)).weak());
                            let port_field = ui.add(
                                egui::TextEdit::singleline(&mut dialog.ua_search_port)
                                    .hint_text(tr(lang, Tr::UaPortHint))
                                    .desired_width(60.0),
                            );
                            focus_in_search_field |= port_field.has_focus();
                        }
                        // OPC UA discovery has one primary action below:
                        // populated host => search host:port; empty host =>
                        // local + subnet. DA retains its separate LAN search.
                        if is_da {
                            if ui
                                .add_enabled(
                                    !dialog.da_pending,
                                    egui::Button::new(tr(lang, Tr::SearchLan)),
                                )
                                .clicked()
                            {
                                refresh = Some(dialog.da_host.clone());
                            }
                            if dialog.da_pending {
                                ui.spinner();
                            }
                        }
                    });

                    // LDS/mDNS and subnet scan are separate actions; keeping
                    // them on their own row avoids overflowing the 560 px
                    // non-resizable connect window.
                    if !is_da {
                        // Optional discovery server: «Поиск через LDS/mDNS» asks
                        // this host for the server list (port defaults to 4840).
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(tr(lang, Tr::LdsHostLabel)).weak());
                            let lds = ui
                                .add(
                                    egui::TextEdit::singleline(&mut dialog.ua_lds_host)
                                        .id_salt("ua_lds_host")
                                        .hint_text(tr(lang, Tr::LdsHostHint))
                                        .desired_width(200.0),
                                )
                                .on_hover_text(tr(lang, Tr::LdsHostTip));
                            focus_in_lds_field = lds.has_focus();
                        });
                        ui.horizontal(|ui| {
                            // «Поиск через LDS/mDNS» (`FindServersOnNetwork` + DNS-SD).
                            let net_searching = dialog.ua_search.network_searching();
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
                        if dialog.ua_search.scan_id().is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                if dialog.ua_scan_total == 0 {
                                    ui.label(tr(lang, Tr::SearchingUaSubnet));
                                } else {
                                    ui.label(fmt(
                                        lang,
                                        Tr::UaScanProgress,
                                        &[
                                            &dialog.ua_scan_checked.to_string(),
                                            &dialog.ua_scan_total.to_string(),
                                            &dialog.ua_scan_found.to_string(),
                                        ],
                                    ));
                                }
                                if ui.button(tr(lang, Tr::UaScanStop)).clicked() {
                                    app_subnet_cancel = true;
                                }
                            });
                        }
                        if let Some(count) = dialog.ua_search.confirmation_count() {
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
                        if dialog.ua_search.confirmation_count().is_none() {
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

                    // OPC DA uses the process's Windows/DCOM identity; the
                    // former username/password fields were not wired to COM.
                    // Never present non-functional credential controls.

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
                        )
                        .on_hover_text(tr(lang, Tr::TrustServerCertTip));
                        // Discovery does not expose user-token policies yet.
                        // Do not silently replace X509 with UserName based on
                        // the endpoint's message security mode. The worker
                        // validates the requested identity at Connect time.
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(tr(lang, Tr::IdentityLabel)).weak());
                            let mut identity = dialog.ua_identity;
                            egui::ComboBox::from_id_salt("ua_identity")
                                .selected_text(match identity {
                                    UaIdentity::Anonymous => tr(lang, Tr::IdentityAnonymous),
                                    UaIdentity::UserName => tr(lang, Tr::IdentityUserName),
                                    UaIdentity::Certificate => tr(lang, Tr::IdentityCertificate),
                                })
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut identity,
                                        UaIdentity::Anonymous,
                                        tr(lang, Tr::IdentityAnonymous),
                                    );
                                    ui.selectable_value(
                                        &mut identity,
                                        UaIdentity::UserName,
                                        tr(lang, Tr::IdentityUserName),
                                    );
                                    ui.selectable_value(
                                        &mut identity,
                                        UaIdentity::Certificate,
                                        tr(lang, Tr::IdentityCertificate),
                                    );
                                });
                            dialog.ua_identity = identity;
                        });
                        if dialog.ua_identity == UaIdentity::UserName {
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
                        focus_in_url_field = url.has_focus();
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
                                            dialog.da_selected = None;
                                        }
                                        Protocol::OpcUa => {
                                            dialog.ua_url = endpoint.address.clone();
                                            dialog.ua_security_policy = endpoint.ua_security_policy;
                                            dialog.ua_security_mode = endpoint.ua_security_mode;
                                            dialog.ua_trust_server_cert =
                                                endpoint.ua_trust_server_cert;
                                            dialog.ua_session_timeout = endpoint
                                                .ua_session_timeout
                                                .unwrap_or(dialog.ua_session_timeout);
                                            dialog.ua_identity = if endpoint.ua_use_certificate {
                                                UaIdentity::Certificate
                                            } else if !endpoint.user.is_empty() {
                                                UaIdentity::UserName
                                            } else {
                                                UaIdentity::Anonymous
                                            };
                                            dialog.ua_user = endpoint.user.clone();
                                            // Passwords are deliberately not persisted.
                                            dialog.ua_password.clear();
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
                        match dialog.pane.protocol() {
                            Protocol::OpcDa => {
                                !dialog.da_manual.trim().is_empty() || dialog.da_selected.is_some()
                            }
                            Protocol::OpcUa => !dialog.ua_url.trim().is_empty(),
                        }
                    };
                    let waiting = dialog.connecting.is_some();
                    ui.horizontal(|ui| {
                        if waiting {
                            // U9: the dialog stays open until this attempt ends.
                            ui.add_enabled(
                                false,
                                egui::Button::new(tr(lang, Tr::ConnectInProgress)),
                            );
                            ui.spinner();
                        } else if ui
                            .add_enabled(
                                can_connect_dialog,
                                egui::Button::new(tr(lang, Tr::Connect)),
                            )
                            .clicked()
                        {
                            connect = Some(dialog.pane.protocol());
                        }
                        if ui.button(tr(lang, Tr::Cancel)).clicked() {
                            close = true;
                        }
                    });
                    // Enter belongs to the focused field, not to an
                    // unconditional popup-wide Connect shortcut.
                    if enter && !opening_frame && focus_in_search_field {
                        if is_da {
                            refresh = Some(dialog.da_host.clone());
                        } else {
                            app_search = true;
                        }
                    } else if enter && !opening_frame && focus_in_lds_field && !ua_busy {
                        app_network_search = true;
                    } else if enter
                        && !opening_frame
                        && !waiting
                        && can_connect_dialog
                        && (focus_in_url_field || ui.memory(|m| m.focused().is_none()))
                    {
                        connect = Some(dialog.pane.protocol());
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
        // The popup owns an uncommitted draft. Cancel must not persist a
        // security choice that was never used for a connection.
        app.validate_ua_security();
    }
    if let Some(host) = refresh {
        if host.trim().is_empty() {
            app.discover_da_all();
        } else {
            app.restart_da_discovery(host);
        }
    }
    if app_subnet_search && !app.dialogs.connect.ua_search_host.trim().is_empty() {
        // The same action searches only the chosen host when it is specified.
        app_subnet_search = false;
        app_search = true;
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
        if app.network.lease.is_some() {
            app.start_ua_subnet_scan();
        } else {
            // A missing network selection must not disable local OPC UA.
            app.search_ua_all();
        }
    }
    if close {
        app.close_connect_dialog();
    }
    if app.dialogs.connect.connecting.is_some() {
        // U9: one attempt at a time; a second click/double-click is ignored.
        connect = None;
        quick_connect = None;
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
                let (host, address) = da_connect_target(dialog);
                // Description may only come from the same computer and ProgID.
                let advertised = dialog
                    .da_servers
                    .iter()
                    .find(|hit| hit.server.id == address && hit.host == host)
                    .and_then(|hit| da_advertised_name(&hit.server));
                let mut ep = crate::bff::model::EndpointSpec::da(host, address);
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
                    let parsed_url = endpoint.address.parse::<crate::bff::model::UaUrl>().ok();
                    let advertised = dialog
                        .ua_search_results()
                        .iter()
                        .find(|s| {
                            s.endpoints.iter().any(|e| {
                                parsed_url
                                    .as_ref()
                                    .is_some_and(|url| e.url.same_endpoint(url))
                            })
                        })
                        .and_then(|s| {
                            if s.application_name.trim().is_empty() {
                                None
                            } else {
                                Some(s.application_name.clone())
                            }
                        });
                    endpoint = endpoint.with_advertised_name(advertised);
                    endpoint.ua_security_policy = dialog.ua_security_policy;
                    endpoint.ua_security_mode = dialog.ua_security_mode;
                    endpoint.ua_trust_server_cert = dialog.ua_trust_server_cert;
                    // Anonymous must not inherit credentials typed earlier.
                    if dialog.ua_identity == UaIdentity::UserName {
                        endpoint.user = dialog.ua_user.trim().to_string();
                        endpoint.password = dialog.ua_password.clone();
                    } else if dialog.ua_identity == UaIdentity::Certificate {
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
                    app.settings.ua_security_policy = endpoint.ua_security_policy;
                    app.settings.ua_security_mode = endpoint.ua_security_mode;
                    // U13: «Доверять сертификату» is a per-connection choice
                    // (kept on the recent endpoint), never a global setting.
                    app.settings.ua_session_timeout_ms = endpoint
                        .ua_session_timeout
                        .unwrap_or_else(crate::settings::default_session_timeout);
                    app.settings.save();
                }
                // U9: the dialog closes on `Connected`, shows `Error` inline.
                app.connect_from_dialog(endpoint);
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
            if endpoint.protocol == Protocol::OpcUa {
                app.settings.ua_security_policy = endpoint.ua_security_policy;
                app.settings.ua_security_mode = endpoint.ua_security_mode;
                app.settings.ua_session_timeout_ms = endpoint
                    .ua_session_timeout
                    .unwrap_or_else(crate::settings::default_session_timeout);
                app.settings.save();
            }
            app.connect_from_dialog(endpoint);
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

/// Resolve the source computer for Connect, including the automatically
/// selected first search result (which has not yet been clicked).
fn da_connect_target(dialog: &crate::app::ConnectDialog) -> (String, String) {
    if !dialog.da_manual.trim().is_empty() {
        (
            dialog.da_host.trim().to_string(),
            dialog.da_manual.trim().to_string(),
        )
    } else {
        dialog
            .da_selected
            .as_ref()
            .map(|hit| (hit.host.trim().to_string(), hit.server.id.clone()))
            .unwrap_or_else(|| (dialog.da_host.trim().to_string(), String::new()))
    }
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
    use super::{da_connect_target, ua_search_url};

    #[test]
    fn automatically_selected_da_server_uses_its_source_host() {
        let mut dialog = crate::app::ConnectDialog::default();
        dialog.da_selected = Some(crate::app::DaHit {
            server: crate::bff::model::ServerEntry {
                id: "Vendor.OPC".into(),
                description: String::new(),
            },
            host: "plant-b".into(),
        });
        assert_eq!(
            da_connect_target(&dialog),
            ("plant-b".to_string(), "Vendor.OPC".to_string())
        );

        dialog.da_host = "manual-pc".to_string();
        dialog.da_manual = "Other.OPC".to_string();
        assert_eq!(
            da_connect_target(&dialog),
            ("manual-pc".to_string(), "Other.OPC".to_string())
        );
    }

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
