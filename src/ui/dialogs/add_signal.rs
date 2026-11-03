use super::super::colors::red;
use super::dialog_spacing;
use crate::app::App;
use crate::i18n::{Tr, tr};
use crate::ui::ImeSafeClick;

pub(super) fn add_signal_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.add_signal.open {
        return;
    }
    let sessions = app.active_sessions();
    let mut submit = false;
    let mut cancel = false;
    let lang = app.lang();
    let dialog = &mut app.dialogs.add_signal;
    egui::Window::new(tr(lang, Tr::AddSignal))
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let enter = crate::ui::key_pressed_once(ui.ctx(), egui::Key::Enter)
                && ui.input(|input| input.events.iter().any(|event| matches!(event,
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, repeat: false, modifiers, .. } if modifiers.is_none()
                )));
            let mut text_owns_enter = false;
            let esc = !crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| i.key_pressed(egui::Key::Escape));
            let opening_frame = dialog.focus_pending;
            let session_selected = dialog.session.is_some();
            ui.horizontal(|ui| {
                ui.label(tr(lang, Tr::ServerLabel));
                let current = dialog
                    .session
                    .and_then(|id| sessions.iter().find(|(sid, _)| *sid == id))
                    .map(|(_, label)| label.clone())
                    .unwrap_or_else(|| tr(lang, Tr::NotSelected).to_string());
                egui::ComboBox::from_id_salt("add_signal_session")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (id, label) in &sessions {
                            super::super::components::selectable_value(ui, &mut dialog.session, Some(*id), label.clone());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(tr(lang, Tr::SignalName));
                let name = ui.add(
                    egui::TextEdit::singleline(&mut dialog.item_id)
                        .hint_text(tr(lang, Tr::HintItemId))
                        .desired_width(300.0),
                );
                if dialog.focus_pending {
                    name.request_focus();
                    dialog.focus_pending = false;
                }
                text_owns_enter |= name.has_focus()
                    || (name.lost_focus() && ui.memory(|memory| memory.focused().is_none()));
            });
            ui.horizontal(|ui| {
                ui.label(tr(lang, Tr::DisplayName));
                let display = ui.add(
                    egui::TextEdit::singleline(&mut dialog.display_name)
                        .hint_text(tr(lang, Tr::Optional))
                        .desired_width(300.0),
                );
                text_owns_enter |= display.has_focus()
                    || (display.lost_focus() && ui.memory(|memory| memory.focused().is_none()));
            });
            if let Some(error) = &dialog.error {
                ui.colored_label(red(), error);
            }
            ui.separator();
            let resolving = dialog.resolving.is_some();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        session_selected && !resolving,
                        egui::Button::new(tr(lang, Tr::Add)),
                    )
                    .ime_safe_clicked()
                {
                    submit = true;
                }
                if resolving {
                    ui.spinner();
                    ui.label(tr(lang, Tr::ResolvingName));
                }
                if ui.button(tr(lang, Tr::Cancel)).ime_safe_clicked() {
                    cancel = true;
                }
            });
            if enter && text_owns_enter && session_selected && !resolving && !opening_frame
                && !ui.input(|input| input.events.iter().any(|event| matches!(event,
                    egui::Event::PointerButton { pressed: true, .. }
                ))) {
                submit = true;
            } else if esc && !opening_frame {
                cancel = true;
            }
        });
    if cancel {
        app.close_add_signal_dialog();
    } else if submit {
        app.add_by_name();
    }
}
