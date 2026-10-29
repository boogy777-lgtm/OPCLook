use super::super::colors::red;
use super::dialog_spacing;
use crate::app::{App, write_type_choices};
use crate::bff::model::Protocol;
use crate::i18n::{Tr, fmt, tr};
use crate::ui::ImeSafeClick;

pub(super) fn write_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.write.open {
        return;
    }
    let lang = app.lang();
    let first = app
        .dialogs
        .write
        .keys
        .first()
        .and_then(|key| app.rows().iter().find(|r| r.key == *key));
    let (protocol, name, current, canonical, data_type, writable) = match first {
        Some(row) => {
            let protocol = app
                .sessions
                .iter()
                .find(|s| s.id == row.session)
                .map(|s| s.protocol())
                .unwrap_or(Protocol::OpcDa);
            (
                protocol,
                row.display_name.clone(),
                row.value.to_string(),
                row.canonical_vt,
                row.data_type.clone(),
                row.writable,
            )
        }
        None => {
            app.dialogs.write.open = false;
            return;
        }
    };
    let unavailable = first.and_then(|row| app.write_unavailable(row.session, protocol));
    let choices = write_type_choices(protocol);
    let mut submit = false;
    let mut close = false;
    // When a confirm dialog sits on top, it owns Esc/Enter; the write dialog
    // must remain inert to those keys so only the topmost dialog responds.
    let confirm_open = app.dialogs.confirm.is_some();
    {
        let dialog = &mut app.dialogs.write;
        egui::Window::new(tr(lang, Tr::ChangeSignalValue))
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(&ctx, |ui| {
                dialog_spacing(ui);
                let enter = crate::ui::key_pressed_once(ui.ctx(), egui::Key::Enter)
                    && ui.input(|input| input.events.iter().any(|event| matches!(event,
                        egui::Event::Key { key: egui::Key::Enter, pressed: true, repeat: false, modifiers, .. } if modifiers.is_none()
                    )));
                let modified_enter = ui.input(|input| input.events.iter().any(|event| matches!(event,
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } if !modifiers.is_none()
                )));
                let esc = !crate::ui::ime_blocks_keys(ui.ctx())
                    && ui.input(|i| i.key_pressed(egui::Key::Escape));
                let opening_frame = dialog.focus_pending;
                ui.label(fmt(lang, Tr::SignalLabel, &[&name]));
                ui.label(fmt(lang, Tr::CurrentValue, &[&current]));
                ui.label(fmt(lang, Tr::ServerTypeLabel, &[&protocol.to_string()]));
                if !writable {
                    ui.colored_label(red(), tr(lang, Tr::ReadOnly));
                }
                if let Some(vt) = canonical {
                    ui.label(fmt(
                        lang,
                        Tr::CanonicalDaType,
                        &[&crate::bff::da_variant::vt_name(vt)],
                    ));
                } else if let Some(dt) = &data_type {
                    ui.label(fmt(lang, Tr::ServerDataType, &[dt]));
                }
                ui.horizontal(|ui| {
                    ui.label(tr(lang, Tr::WriteTypeLabel));
                    let selected = choices
                        .get(dialog.type_index)
                        .map(|c| c.label)
                        .unwrap_or("Auto");
                    egui::ComboBox::from_id_salt("write_type")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for (index, choice) in choices.iter().enumerate() {
                                let previous = dialog.type_index;
                                let response = ui.selectable_value(&mut dialog.type_index, index, choice.label);
                                if crate::ui::ime_blocks_keys(ui.ctx()) && response.changed() && !response.ime_safe_clicked() {
                                    dialog.type_index = previous;
                                }
                            }
                        });
                });
                ui.label(tr(lang, Tr::ValueLabel));
                let value_out = egui::TextEdit::singleline(&mut dialog.text)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr(lang, Tr::HintWriteValue))
                    .show(ui);
                if dialog.focus_pending {
                    // K3: the field opens focused with the current value selected,
                    // so typing replaces it and Enter writes: no mouse needed.
                    let id = value_out.response.response.id;
                    value_out.response.response.request_focus();
                    let mut state = value_out.state;
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::select_all_str(
                            &dialog.text,
                        )));
                    state.store(ui.ctx(), id);
                    dialog.focus_pending = false;
                }
                // Enter belongs to the value field, not to a type selection or other control.
                if enter && !modified_enter
                    && (value_out.response.response.has_focus()
                        || (value_out.response.response.lost_focus()
                            && ui.memory(|memory| memory.focused().is_none())))
                    && !ui.input(|input| input.events.iter().any(|event| matches!(event,
                        egui::Event::PointerButton { pressed: true, .. }
                    )))
                    && writable && unavailable.is_none() && !opening_frame && !confirm_open {
                    submit = true;
                } else if esc && !opening_frame && !confirm_open {
                    close = true;
                }
                if let Some(error) = &dialog.error {
                    ui.colored_label(red(), error);
                }
                {
                    let confirm_on = app.settings.view.options.confirm_write;
                    if confirm_on {
                        ui.label(egui::RichText::new(tr(lang, Tr::ConfirmAfterWrite)).weak());
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let response = ui.add_enabled(
                        writable && unavailable.is_none() && !confirm_open,
                        egui::Button::new(tr(lang, Tr::Write)),
                    );
                    let response = if let Some(reason) = unavailable {
                        let text = tr(lang, reason);
                        response.on_hover_text(text).on_disabled_hover_text(text)
                    } else {
                        response
                    };
                    if (response.clicked_by(egui::PointerButton::Primary)
                        || (response.ime_safe_clicked() && !crate::ui::ime_blocks_keys(ui.ctx()) && !modified_enter))
                        && !crate::ui::key_repeat(ui.ctx(), egui::Key::Enter)
                        && !crate::ui::key_repeat(ui.ctx(), egui::Key::Space)
                    {
                        submit = true;
                    }
                    if ui
                        .add_enabled(!confirm_open, egui::Button::new(tr(lang, Tr::Cancel)))
                        .ime_safe_clicked()
                    {
                        close = true;
                    }
                });
            });
    }
    // An Enter on Cancel can also trigger the dialog default. Cancellation wins.
    if close {
        app.dialogs.write.open = false;
    } else if submit {
        app.apply_write();
    }
}
