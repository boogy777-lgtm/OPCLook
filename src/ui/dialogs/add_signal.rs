use super::super::colors::red;
use super::dialog_spacing;
use crate::app::App;
use crate::i18n::{Tr, tr};

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
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
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
                            ui.selectable_value(&mut dialog.session, Some(*id), label);
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
            });
            ui.horizontal(|ui| {
                ui.label(tr(lang, Tr::DisplayName));
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.display_name)
                        .hint_text(tr(lang, Tr::Optional))
                        .desired_width(300.0),
                );
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
                    .clicked()
                {
                    submit = true;
                }
                if resolving {
                    ui.spinner();
                    ui.label(tr(lang, Tr::ResolvingName));
                }
                if ui.button(tr(lang, Tr::Cancel)).clicked() {
                    cancel = true;
                }
            });
            if enter && session_selected && !resolving && !opening_frame {
                submit = true;
            } else if esc && !opening_frame {
                cancel = true;
            }
        });
    if cancel {
        app.close_add_signal_dialog();
    }
    if submit {
        app.add_by_name();
    }
}
