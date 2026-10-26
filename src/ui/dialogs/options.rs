use super::super::colors::yellow;
use super::dialog_spacing;
use crate::app::App;
use crate::constants::{
    OPERATION_TIMEOUT_MAX_SECS, OPERATION_TIMEOUT_MIN_SECS, TEST_INTERVAL_MAX_SECS,
    TEST_INTERVAL_MIN_SECS, UPDATE_INTERVAL_MAX_SECS, UPDATE_INTERVAL_MIN_SECS,
};
use crate::i18n::{Tr, tr};

pub(super) fn options_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.options.open {
        return;
    }
    let mut apply = false;
    let mut close = false;
    let lang = app.lang();
    {
        let draft = &mut app.dialogs.options.draft;
        egui::Window::new(tr(lang, Tr::OptionsTitle))
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(&ctx, |ui| {
                dialog_spacing(ui);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                let opening_frame = app.dialogs.options.focus_pending;
                ui.heading(tr(lang, Tr::UpdateInterval));
                let slider = ui.add(
                    egui::Slider::new(
                        &mut draft.update_interval_secs,
                        UPDATE_INTERVAL_MIN_SECS..=UPDATE_INTERVAL_MAX_SECS,
                    )
                    .text(tr(lang, Tr::Seconds))
                    .integer(),
                );
                if app.dialogs.options.focus_pending {
                    slider.request_focus();
                    app.dialogs.options.focus_pending = false;
                }
                ui.label(egui::RichText::new(tr(lang, Tr::RecommendedOneSec)).weak());
                ui.separator();
                ui.heading(tr(lang, Tr::AutoReconnectTitle));
                ui.checkbox(&mut draft.auto_reconnect, tr(lang, Tr::AutoReconnectCheck));
                ui.label(egui::RichText::new(tr(lang, Tr::AutoReconnectWarn)).color(yellow()));
                ui.separator();
                ui.heading(tr(lang, Tr::DisplayOptions));
                ui.checkbox(&mut draft.show_colors, tr(lang, Tr::ShowColorsCheck));
                ui.checkbox(&mut draft.confirm_write, tr(lang, Tr::ConfirmWriteCheck));
                ui.separator();
                ui.heading(tr(lang, Tr::AdvancedTitle));
                ui.add(
                    egui::DragValue::new(&mut draft.operation_timeout_secs)
                        .range(OPERATION_TIMEOUT_MIN_SECS..=OPERATION_TIMEOUT_MAX_SECS)
                        .suffix(tr(lang, Tr::SecondsShortSuffix)),
                );
                ui.label(tr(lang, Tr::OpTimeoutLabel));
                ui.add(
                    egui::DragValue::new(&mut draft.test_interval_secs)
                        .range(TEST_INTERVAL_MIN_SECS..=TEST_INTERVAL_MAX_SECS)
                        .suffix(tr(lang, Tr::SecondsShortSuffix)),
                );
                ui.label(tr(lang, Tr::TestIntervalLabel));
                ui.add(
                    egui::DragValue::new(&mut draft.ua_deadband)
                        .range(0.0..=1_000_000.0)
                        .speed(0.1),
                );
                ui.label(tr(lang, Tr::DeadbandLabel));
                ui.label(egui::RichText::new(tr(lang, Tr::DeadbandHint)).weak());
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(tr(lang, Tr::Save)).clicked() {
                        apply = true;
                    }
                    if ui.button(tr(lang, Tr::Cancel)).clicked() {
                        close = true;
                    }
                });
                if enter && !opening_frame {
                    apply = true;
                } else if esc && !opening_frame {
                    close = true;
                }
            });
    }
    if apply {
        app.apply_options();
        app.dialogs.options.open = false;
        app.settings.save();
    }
    if close {
        app.dialogs.options.open = false;
    }
}
