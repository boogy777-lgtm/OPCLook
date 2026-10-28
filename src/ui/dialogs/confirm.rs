use super::dialog_spacing;
use crate::app::App;
use crate::i18n::{Tr, tr};

pub(super) fn confirm_dialog(app: &mut App, ui: &mut egui::Ui, was_open: bool) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.confirm.open {
        return;
    }
    let lang = app.lang();
    let title = if app.dialogs.confirm.title.is_empty() {
        tr(lang, Tr::ConfirmTitle).to_string()
    } else {
        app.dialogs.confirm.title.clone()
    };
    let text = app.dialogs.confirm.text.clone();
    let mut yes = false;
    let mut no = false;
    egui::Window::new(&title)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let enter = was_open && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = was_open && ui.input(|i| i.key_pressed(egui::Key::Escape));
            ui.label(text);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr(lang, Tr::Yes)).clicked() {
                    yes = true;
                }
                if ui.button(tr(lang, Tr::No)).clicked() {
                    no = true;
                }
            });
            if enter {
                yes = true;
            } else if esc {
                no = true;
            }
        });
    if yes {
        app.confirm_yes();
    }
    if no {
        app.confirm_no();
    }
}
