use super::*;

pub(super) fn server_results(
    ui: &mut egui::Ui,
    lang: Lang,
    dialog: &mut crate::app::ConnectDialog,
    is_da: bool,
    ua_session_count: usize,
    connect: &mut Option<Protocol>,
    quick_connect: &mut Option<crate::bff::model::EndpointSpec>,
) {
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
        // Adapt the nested results pane; the outer dialog scrolls on short screens.
        .max_height(if is_da {
            150.0
        } else {
            (ui.ctx().content_rect().height() * 0.15).clamp(80.0, 240.0)
        })
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
                    let selected = dialog.da_selected.as_ref().is_some_and(|selected| {
                        selected.server.id == server.id && selected.host == *host
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
                    let hint = format!("{}\n{}", server.id, tr(lang, Tr::DoubleClickServerHint));
                    let response = ui.selectable_label(selected, label).on_hover_text(&hint);
                    if response.double_clicked() {
                        let mut ep =
                            crate::bff::model::EndpointSpec::da(host.clone(), server.id.clone());
                        ep.acquisition = dialog.da_acquisition.clone();
                        ep = ep.with_advertised_name(da_advertised_name(server));
                        *quick_connect = Some(ep);
                    } else if response.clicked() {
                        dialog.da_selected = Some(hit.clone());
                        dialog.da_host.clone_from(host);
                        dialog.da_manual.clone_from(&server.id);
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
                if let Some(pick) = super::super::ua_tree::ua_server_tree(
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
                        *connect = Some(Protocol::OpcUa);
                    }
                }
            }
        });
}
