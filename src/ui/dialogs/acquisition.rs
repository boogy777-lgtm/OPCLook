use crate::app::App;
use crate::bff::model::{PollFreshness, Protocol, UpdateMode};
use crate::i18n::{Tr, tr};
use crate::ui::ImeSafeClick;

pub(super) fn acquisition_dialog(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    let target = app
        .dialogs
        .acquisition
        .session
        .and_then(|id| app.sessions.iter().find(|view| view.id == id));
    let endpoint = target.map(|view| view.endpoint().clone());
    let mut apply = None;
    let mut cancel = false;
    let height = (ui.ctx().content_rect().height() - 32.0).max(100.0);
    egui::Window::new(tr(lang, Tr::AcquisitionEdit))
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_height(height.min(600.0))
        .max_height(height)
        .vscroll(true)
        .show(ui.ctx(), |ui| {
            super::dialog_spacing(ui);
            ui.heading(tr(lang, Tr::AcquisitionActive));
            if let Some(endpoint) = &endpoint {
                if let Some(config) = &endpoint.acquisition {
                    let mode = match config.update_mode {
                        UpdateMode::Poll => Tr::AcquisitionPoll,
                        UpdateMode::Subscription => Tr::AcquisitionSubscription,
                    };
                    let freshness = match config.poll_freshness {
                        PollFreshness::CacheAllowed => Tr::AcquisitionCache,
                        PollFreshness::FreshSource => Tr::AcquisitionSource,
                    };
                    ui.label(format!(
                        "{}: {}",
                        tr(lang, Tr::AcquisitionMode),
                        tr(lang, mode)
                    ));
                    ui.label(format!(
                        "{}: {}",
                        tr(lang, Tr::AcquisitionRequestedInterval),
                        config.interval_ms
                    ));
                    ui.label(format!(
                        "{}: {}",
                        tr(lang, Tr::AcquisitionReadback),
                        tr(
                            lang,
                            if config.read_after_write {
                                Tr::Yes
                            } else {
                                Tr::No
                            }
                        )
                    ));
                    ui.label(format!(
                        "{}: {}",
                        tr(lang, Tr::AcquisitionFreshness),
                        tr(lang, freshness)
                    ));
                } else {
                    ui.label(tr(lang, Tr::AcquisitionLegacy));
                }
                ui.separator();
                ui.heading(tr(lang, Tr::AcquisitionDraft));
                super::connect::acquisition_editor(
                    ui,
                    lang,
                    &mut app.dialogs.acquisition.draft,
                    endpoint.protocol == Protocol::OpcDa,
                    true,
                );
            } else {
                ui.colored_label(egui::Color32::RED, tr(lang, Tr::AcquisitionEpochChanged));
            }
            if let Some(error) = &app.dialogs.acquisition.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if endpoint.is_some() {
                    if ui
                        .button(tr(lang, Tr::AcquisitionApplyNext))
                        .ime_safe_clicked()
                    {
                        apply = Some(false);
                    }
                    if ui
                        .button(tr(lang, Tr::AcquisitionApplyReconnect))
                        .ime_safe_clicked()
                    {
                        apply = Some(true);
                    }
                }
                if ui.button(tr(lang, Tr::Cancel)).ime_safe_clicked() {
                    cancel = true;
                }
            });
            if !crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| i.key_pressed(egui::Key::Escape))
            {
                cancel = true;
            }
        });
    if cancel {
        app.dialogs.acquisition.open = false;
    } else if let Some(reconnect) = apply {
        app.apply_acquisition_editor(reconnect);
    }
}
