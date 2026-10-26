use super::super::colors::red;
use crate::i18n::{Lang, Tr, tr};
use crate::net::{AdapterInfo, AdapterSelection, selectable_adapters};

pub(super) fn adapter_type_label(lang: Lang, adapter: &AdapterInfo) -> &'static str {
    if adapter.is_loopback {
        tr(lang, Tr::NetworkTypeLoopback)
    } else if adapter.is_physical {
        tr(lang, Tr::NetworkTypePhysical)
    } else {
        tr(lang, Tr::NetworkTypeVirtual)
    }
}

pub(super) fn adapter_display_name(lang: Lang, adapter: &AdapterInfo) -> String {
    if adapter.name.trim().is_empty() {
        if adapter.is_loopback {
            tr(lang, Tr::NetworkLoopbackName).to_string()
        } else {
            adapter.id.to_string()
        }
    } else {
        adapter.name.clone()
    }
}

fn adapter_choice_label(lang: Lang, adapter: &AdapterInfo, selected: bool) -> String {
    let label = format!(
        "{} ({})",
        adapter_display_name(lang, adapter),
        adapter_type_label(lang, adapter)
    );
    // An explicit marker makes the single selected row visible even when the
    // theme's highlight color does not render distinctly.
    if selected {
        format!("● {label}")
    } else {
        label
    }
}

/// Minimal network form: adapter, «Обновить», local IP, short status/hint and
/// «Забыть» for a stale saved GUID. Diagnostics go to the journal instead.
pub(super) fn network_settings_view(
    ui: &mut egui::Ui,
    lang: Lang,
    adapters: &[AdapterInfo],
    refresh_failed: bool,
    preferred: &AdapterSelection,
    preferred_ip: Option<std::net::IpAddr>,
    refresh: &mut bool,
    action: &mut Option<(AdapterSelection, Option<std::net::IpAddr>)>,
) {
    let preferred_id = preferred.adapter_id();
    let rows = selectable_adapters(adapters);
    let selected = rows
        .iter()
        .copied()
        .find(|adapter| preferred_id.is_some() && preferred_id == Some(adapter.id));
    // Reserve room for the label and Refresh control in a 560px popup.
    let max_width = (ui.available_width() - 230.0).clamp(160.0, 330.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkAdapterCombo)).strong());
        let mut chosen = preferred_id;
        egui::ComboBox::from_id_salt("network_adapter_combo")
            .selected_text(match selected {
                Some(adapter) => adapter_display_name(lang, adapter),
                None => tr(lang, Tr::NetworkNotSelected).to_string(),
            })
            .width(max_width)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, tr(lang, Tr::NetworkNotSelected));
                for adapter in &rows {
                    let is_selected = preferred_id.is_some() && preferred_id == Some(adapter.id);
                    ui.selectable_value(
                        &mut chosen,
                        Some(adapter.id),
                        adapter_choice_label(lang, adapter, is_selected),
                    );
                }
            });
        if chosen != preferred_id {
            *action = Some(match chosen {
                Some(id) => (AdapterSelection::Adapter(id), None),
                None => (AdapterSelection::None, None),
            });
        }
        if ui.button(tr(lang, Tr::NetworkRefresh)).clicked() {
            *refresh = true;
        }
    });

    if refresh_failed {
        ui.colored_label(red(), tr(lang, Tr::NetworkRefreshFailed));
    }

    if let Some(adapter) = selected {
        let current = adapter.select_address(preferred_ip);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr(lang, Tr::NetworkAddressChoice)).weak());
            match current {
                None => {
                    ui.colored_label(red(), tr(lang, Tr::NetworkNoIp));
                }
                Some(address) if adapter.addresses.len() == 1 => {
                    ui.label(address.display_with_prefix());
                }
                Some(address) => {
                    let mut chosen = Some(address.ip);
                    egui::ComboBox::from_id_salt(("network_ip", adapter.id.to_string()))
                        .selected_text(address.display_with_prefix())
                        .width(max_width)
                        .show_ui(ui, |ui| {
                            for candidate in &adapter.addresses {
                                ui.selectable_value(
                                    &mut chosen,
                                    Some(candidate.ip),
                                    candidate.display_with_prefix(),
                                );
                            }
                        });
                    if chosen != Some(address.ip) {
                        *action = Some((AdapterSelection::Adapter(adapter.id), chosen));
                    }
                }
            }
        });
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkApplyHint)).weak());
    } else if preferred_id.is_some() {
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkStaleAdapter)).weak());
        if ui.button(tr(lang, Tr::NetworkForget)).clicked() {
            *action = Some((AdapterSelection::None, None));
        }
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkApplyHint)).weak());
    } else if rows.is_empty() {
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkEmptyList)).weak());
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkApplyHint)).weak());
    } else {
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkSelectAdapterHint)).weak());
        ui.label(egui::RichText::new(tr(lang, Tr::NetworkApplyHint)).weak());
    }
}
