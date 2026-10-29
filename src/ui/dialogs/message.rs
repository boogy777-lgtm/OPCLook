use super::dialog_spacing;
use crate::app::App;
use crate::i18n::{Tr, tr};

pub(super) fn message_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let Some(message) = &app.dialogs.message else {
        return;
    };
    let mut close = false;
    egui::Window::new(&message.title)
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let esc = !crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| i.key_pressed(egui::Key::Escape));
            ui.label(&message.text);
            if !message.details.is_empty() {
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&message.details).monospace());
                    });
            }
            ui.separator();
            if ui.button(tr(app.lang(), Tr::Close)).clicked() || esc {
                close = true;
            }
        });
    if close {
        app.dialogs.message = None;
    }
}
