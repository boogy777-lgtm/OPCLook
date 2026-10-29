use super::dialog_spacing;
use crate::app::App;
use crate::i18n::{Tr, tr};
use crate::ui::ImeSafeClick;

pub(super) fn confirm_dialog(app: &mut App, ui: &mut egui::Ui, was_open: bool) {
    let ctx = ui.ctx().clone();
    let Some(request) = app.dialogs.confirm.as_ref() else {
        return;
    };
    let lang = app.lang();
    let title = if request.title.is_empty() {
        tr(lang, Tr::ConfirmTitle).to_string()
    } else {
        request.title.clone()
    };
    let text = request.text.clone();
    let mut yes = false;
    let mut no = false;
    egui::Window::new(&title)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let (plain_enter, modified_enter) = ui.input(|input| {
                input
                    .events
                    .iter()
                    .fold((false, false), |(plain, modified), event| match event {
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            repeat: false,
                            modifiers,
                            ..
                        } => (
                            plain || modifiers.is_none(),
                            modified || !modifiers.is_none(),
                        ),
                        _ => (plain, modified),
                    })
            });
            let enter = was_open && !crate::ui::ime_blocks_keys(ui.ctx()) && plain_enter;
            let esc = was_open
                && (!crate::ui::ime_blocks_keys(ui.ctx())
                    && ui.input(|i| i.key_pressed(egui::Key::Escape)));
            ui.label(text);
            ui.separator();
            ui.horizontal(|ui| {
                let yes_button = ui.button(tr(lang, Tr::Yes));
                if (yes_button.clicked_by(egui::PointerButton::Primary)
                    || (yes_button.ime_safe_clicked() && !modified_enter))
                    && !crate::ui::key_repeat(ui.ctx(), egui::Key::Enter)
                    && !crate::ui::key_repeat(ui.ctx(), egui::Key::Space)
                {
                    yes = true;
                }
                if ui.button(tr(lang, Tr::No)).ime_safe_clicked() {
                    no = true;
                }
            });
            if enter {
                yes = true;
            } else if esc {
                no = true;
            }
        });
    // Enter can activate the focused No button and the default Yes together.
    // Cancellation wins even if both were raised in the same frame.
    if no {
        app.confirm_no();
    } else if yes {
        app.confirm_yes();
    }
}
