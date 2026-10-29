use super::dialog_spacing;
use crate::app::App;
use crate::i18n::{Tr, tr};
use crate::ui::ImeSafeClick;

pub(super) fn confirm_dialog(app: &mut App, ui: &mut egui::Ui, was_open: bool) {
    let ctx = ui.ctx().clone();
    let lang = app.lang();
    let Some(request) = app.dialogs.confirm.as_mut() else {
        return;
    };
    let title = if request.title.is_empty() {
        tr(lang, Tr::ConfirmTitle).to_string()
    } else {
        request.title.clone()
    };
    let text = request.text.clone();
    let focus_pending = std::mem::take(&mut request.focus_pending);
    let mut yes = false;
    let mut no = false;
    egui::Modal::new(egui::Id::unique("confirm_dialog")).show(&ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 40.0).clamp(120.0, 460.0));
        ui.heading(&title);
        dialog_spacing(ui);
        let modified_enter = ui.input(|input| {
            input.events.iter().any(|event| {
                matches!(event,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            modifiers,
                            ..
                        } if !modifiers.is_none())
            })
        });
        let esc = was_open
            && (!crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| i.key_pressed(egui::Key::Escape)));
        let pointer_press = ui.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerButton { .. }))
        });
        // The two choices form one keyboard region. Keep egui's actual
        // focus as the only source of truth, including mouse/Tab focus.
        let move_left = was_open
            && !focus_pending
            && !crate::ui::ime_blocks_keys(ui.ctx())
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let move_right = was_open
            && !focus_pending
            && !crate::ui::ime_blocks_keys(ui.ctx())
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        if move_left || move_right {
            ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
        ui.label(text);
        ui.separator();
        ui.horizontal(|ui| {
            let yes_button = ui.button(tr(lang, Tr::Yes));
            let no_button = ui.button(tr(lang, Tr::No));
            if focus_pending {
                no_button.request_focus();
            } else if move_left {
                yes_button.request_focus();
            } else if move_right {
                no_button.request_focus();
            }
            if ((was_open && !focus_pending && yes_button.clicked_by(egui::PointerButton::Primary))
                || (was_open
                    && !focus_pending
                    && !move_left
                    && !move_right
                    && !pointer_press
                    && yes_button.ime_safe_clicked()
                    && !modified_enter))
                && !crate::ui::key_repeat(ui.ctx(), egui::Key::Enter)
                && !crate::ui::key_repeat(ui.ctx(), egui::Key::Space)
            {
                yes = true;
            }
            if ((was_open && !focus_pending && no_button.clicked_by(egui::PointerButton::Primary))
                || (was_open
                    && !focus_pending
                    && !move_left
                    && !move_right
                    && !pointer_press
                    && no_button.ime_safe_clicked()
                    && !modified_enter))
                && !crate::ui::key_repeat(ui.ctx(), egui::Key::Enter)
                && !crate::ui::key_repeat(ui.ctx(), egui::Key::Space)
            {
                no = true;
            }
        });
        if esc {
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
