use super::files::{
    load_journal, load_script_file, load_tree_csv, save_journal, save_script_file, save_tree_csv,
};
use super::tree::{open_add_signal, refresh_selected, request_disconnect};
use crate::app::{App, MessageDialog};
use crate::i18n::{Lang, Tr, tr};
use crate::settings::Theme;

/// The theme selector remains available while editing Options: its changes
/// update the draft without exposing other background actions.
pub(super) fn options_theme_only(app: &App) -> bool {
    let d = &app.dialogs;
    d.options.open
        && !d.palette.open
        && !d.connect.open
        && !d.add_signal.open
        && !d.write.open
        && !d.columns.open
        && !d.about.open
        && d.confirm.is_none()
        && d.message.is_none()
        && app.csv_import.is_none()
}

#[derive(Clone, Default)]
struct HeaderFocus {
    selected: usize,
    roots: [Option<egui::Id>; 8],
    popups: [Option<egui::Id>; 8],
    focus_root: bool,
    focus_first_item: bool,
}

fn header_state_id() -> egui::Id {
    egui::Id::unique("opclook_header_keyboard")
}

pub(super) fn theme_popup_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<HeaderFocus>(header_state_id()))
        .and_then(|focus| focus.popups[0])
        .is_some_and(|id| egui::Popup::is_id_open(ctx, id))
}

fn header_menu<R>(
    ui: &mut egui::Ui,
    state: &mut HeaderFocus,
    index: usize,
    label: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) {
    ui.push_id(index, |ui| {
        let result = ui.menu_button(label, |ui| {
            if state.selected == index && state.focus_first_item && !ui.is_sizing_pass() {
                state.focus_first_item = false;
                ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::Down));
            }
            content(ui)
        });
        let response = result.response;
        state.roots[index] = Some(response.id);
        state.popups[index] = Some(egui::Popup::menu(&response).get_id());
        if state.selected == index && state.focus_root {
            response.request_focus();
            state.focus_root = false;
        }
        if response.clicked_by(egui::PointerButton::Primary) {
            state.selected = index;
            super::regions::pointer_enter(ui.ctx(), super::regions::Region::Header);
        }
    });
}

pub(super) fn menu(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    let ctx = ui.ctx().clone();
    let reject_theme_keyboard = super::ime_blocks_keys(&ctx)
        || ctx.input(|input| input.events.iter().any(|event| matches!(event,
            egui::Event::Key { key: egui::Key::Enter | egui::Key::Space, pressed: true, repeat, modifiers, .. }
                if *repeat || !modifiers.is_none())));
    let mut focus = ctx.data(|data| {
        data.get_temp::<HeaderFocus>(header_state_id())
            .unwrap_or_default()
    });
    let is_header = super::regions::current(&ctx) == Some(super::regions::Region::Header);
    if !is_header
        || ((app.dialogs.any_open() || app.csv_import.is_some()) && !options_theme_only(app))
    {
        for popup in focus.popups.iter().flatten() {
            egui::Popup::close_id(&ctx, *popup);
        }
        focus.focus_first_item = false;
    }
    if super::regions::take_entry(&ctx, super::regions::Region::Header) {
        focus.focus_root = true;
    }
    if is_header
        && !app.dialogs.any_open()
        && app.csv_import.is_none()
        && !super::ime_blocks_keys(&ctx)
    {
        let left =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let right =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        if left || right {
            if let Some(id) = focus.popups[focus.selected] {
                egui::Popup::close_id(&ctx, id);
            }
            focus.selected = if left {
                (focus.selected + 7) % 8
            } else {
                (focus.selected + 1) % 8
            };
            focus.focus_root = true;
            focus.focus_first_item = false;
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter | egui::Key::Space,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
        } else if focus.popups[focus.selected].is_some_and(|id| !egui::Popup::is_id_open(&ctx, id))
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
        {
            if let Some(id) = focus.popups[focus.selected] {
                egui::Popup::open_id(&ctx, id);
                focus.focus_first_item = true;
                ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                ctx.request_repaint();
            }
        }
    }
    if (is_header && !app.dialogs.any_open() && app.csv_import.is_none())
        || (options_theme_only(app) && theme_popup_open(&ctx))
    {
        let composing = super::ime_blocks_keys(&ctx);
        ctx.input_mut(|input| input.events.retain(|event| !matches!(event,
            egui::Event::Key { key: egui::Key::Enter | egui::Key::Space, pressed: true, repeat, modifiers, .. }
                if composing || *repeat || !modifiers.is_none()
        )));
    }
    let panel = egui::Panel::top("menu_bar").show(ui, |ui| {
        let narrow = ui.available_width() < 700.0;
        if narrow {
            egui::MenuBar::new().ui(ui, |ui| {
                theme_menu(app, ui, lang, &mut focus, reject_theme_keyboard)
            });
        }
        egui::MenuBar::new().ui(ui, |ui| {
            if !narrow {
                theme_menu(app, ui, lang, &mut focus, reject_theme_keyboard);
            }
            let can_open_other_menus = !app.dialogs.any_open() && app.csv_import.is_none();
            ui.add_enabled_ui(can_open_other_menus, |ui| {
                header_menu(ui, &mut focus, 1, tr(lang, Tr::MenuClient), |ui| {
                    if ui
                        .add_enabled(
                            !app.dialogs.any_open() && app.csv_import.is_none(),
                            egui::Button::new(tr(lang, Tr::PaletteTitle)),
                        )
                        .clicked()
                    {
                        app.dialogs.palette.open = true;
                        app.dialogs.palette.query.clear();
                        app.dialogs.palette.selected = None;
                        app.dialogs.palette.focus_pending = true;
                        ui.close();
                    }
                    if super::actions::action_button(
                        app,
                        ui,
                        crate::app::actions::UiAction::OpenConnectDialog,
                        egui::Button::new(tr(lang, Tr::ConnectEllipsis)).shortcut_text(
                            super::shortcuts::action_shortcut(
                                crate::app::actions::UiAction::OpenConnectDialog.id(),
                            )
                            .map(|shortcut| ui.ctx().format_shortcut(&shortcut))
                            .unwrap_or_default(),
                        ),
                    )
                    .clicked()
                    {
                        let _ =
                            app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::Disconnect)).clicked() {
                        if let Some(session) = app.active_session {
                            request_disconnect(app, session);
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr(lang, Tr::SettingsEllipsis)).clicked() {
                        app.open_options_dialog();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr(lang, Tr::Exit)).clicked() {
                        app.save_settings();
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                header_menu(ui, &mut focus, 2, tr(lang, Tr::MenuServers), |ui| {
                    let available = app
                        .ui_action_unavailable(crate::app::actions::UiAction::OpenConnectDialog)
                        .is_none();
                    let response = super::actions::action_button(
                        app,
                        ui,
                        crate::app::actions::UiAction::OpenConnectDialog,
                        egui::Button::new(tr(lang, Tr::ConnectEllipsis)),
                    );
                    let response = if available {
                        response.on_hover_text(tr(lang, Tr::HintConnectUa))
                    } else {
                        response
                    };
                    if response.clicked() {
                        let _ =
                            app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::Disconnect)).clicked() {
                        if let Some(session) = app.active_session {
                            request_disconnect(app, session);
                        }
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::Reconnect)).clicked() {
                        if let Some(session) = app.active_session {
                            app.reconnect_session(session);
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(tr(lang, Tr::RefreshBranch))
                        .on_hover_text(crate::i18n::fmt(
                            lang,
                            Tr::HintRefreshF5,
                            &[&super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::Refresh,
                            )],
                        ))
                        .clicked()
                    {
                        refresh_selected(app);
                        ui.close();
                    }
                    if ui
                        .button(tr(lang, Tr::LoadTreeCsv))
                        .on_hover_text(tr(lang, Tr::HintLoadCsv))
                        .clicked()
                    {
                        load_tree_csv(app);
                        ui.close();
                    }
                    if ui
                        .button(tr(lang, Tr::SaveTreeCsv))
                        .on_hover_text(tr(lang, Tr::HintSaveCsv))
                        .clicked()
                    {
                        save_tree_csv(app);
                        ui.close();
                    }
                });

                header_menu(ui, &mut focus, 3, tr(lang, Tr::MenuSignals), |ui| {
                    // Tree Enter is context-dependent and can open Write; it is
                    // not an equivalent shortcut for the Add Selected menu action.
                    if ui.button(tr(lang, Tr::AddSelected)).clicked() {
                        app.add_selected_tree();
                        ui.close();
                    }
                    if ui
                        .button(tr(lang, Tr::AddChildrenSignals))
                        .on_hover_text(tr(lang, Tr::HintAddChildren))
                        .clicked()
                    {
                        app.add_selected_children();
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::AddByName)).clicked() {
                        open_add_signal(app);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(tr(lang, Tr::RemoveSelected))
                        .on_hover_text(crate::i18n::fmt(
                            lang,
                            Tr::HintDelSelected,
                            &[&super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::Delete,
                            )],
                        ))
                        .clicked()
                    {
                        app.remove_selected_rows();
                        ui.close();
                    }
                    let multiple = app.selected_rows.len() > 1;
                    let write = ui.button(tr(
                        lang,
                        if multiple {
                            Tr::ChangeSelectedValues
                        } else {
                            Tr::ChangeValue
                        },
                    ));
                    let write = if multiple {
                        write
                    } else {
                        write.on_hover_text(crate::i18n::fmt(
                            lang,
                            Tr::HintEnterF2,
                            &[&super::shortcuts::binding_texts(
                                ui.ctx(),
                                super::keymap::KeyAction::TableWrite,
                            )],
                        ))
                    };
                    if write.clicked() {
                        app.open_write_dialog(Vec::new());
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(tr(lang, Tr::SelectAll))
                        .on_hover_text(crate::i18n::fmt(
                            lang,
                            Tr::HintCtrlA,
                            &[&super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::SelectAll,
                            )],
                        ))
                        .clicked()
                    {
                        app.select_all_rows();
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::ClearSelection)).clicked() {
                        app.clear_selection();
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(tr(lang, Tr::ColumnsEllipsis))
                        .on_hover_text(tr(lang, Tr::HintEditColumns))
                        .clicked()
                    {
                        app.dialogs.columns.open = true;
                        ui.close();
                    }
                });

                header_menu(ui, &mut focus, 4, tr(lang, Tr::MenuScript), |ui| {
                    if ui.button(tr(lang, Tr::Save)).clicked() {
                        save_script_file(app);
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::Load)).clicked() {
                        load_script_file(app);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr(lang, Tr::CheckSyntax)).clicked() {
                        app.dialogs.message = Some(MessageDialog {
                            title: tr(lang, Tr::ScriptTitle).to_string(),
                            text: tr(lang, Tr::ScriptNotImpl).to_string(),
                            details: app.script_protocol_note(),
                        });
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .checkbox(
                            &mut app.settings.view.options.show_script,
                            tr(lang, Tr::ShowScript),
                        )
                        .clicked()
                    {
                        app.persist_settings();
                    }
                    if ui
                        .checkbox(
                            &mut app.settings.view.options.show_log,
                            tr(lang, Tr::ShowJournal),
                        )
                        .clicked()
                    {
                        app.persist_settings();
                    }
                });

                header_menu(ui, &mut focus, 5, tr(lang, Tr::MenuJournal), |ui| {
                    if ui
                        .checkbox(
                            &mut app.settings.view.options.show_log,
                            tr(lang, Tr::ShowWorkJournal),
                        )
                        .clicked()
                    {
                        app.persist_settings();
                    }
                    ui.separator();
                    if ui.button(tr(lang, Tr::Clear)).clicked() {
                        app.journal.clear();
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::SaveJournal)).clicked() {
                        save_journal(app);
                        ui.close();
                    }
                    if ui.button(tr(lang, Tr::LoadJournal)).clicked() {
                        load_journal(app);
                        ui.close();
                    }
                });

                header_menu(ui, &mut focus, 6, tr(lang, Tr::MenuLanguage), |ui| {
                    let current = app.lang();
                    for (target, label) in [
                        (Lang::Ru, tr(lang, Tr::LangRu)),
                        (Lang::En, tr(lang, Tr::LangEn)),
                        (Lang::Zh, tr(lang, Tr::LangZh)),
                    ] {
                        let mut selected = current == target;
                        if ui
                            .checkbox(&mut selected, label)
                            .on_hover_text(lang_label(target))
                            .clicked()
                        {
                            app.set_lang(target);
                            ui.close();
                        }
                    }
                });

                header_menu(ui, &mut focus, 7, tr(lang, Tr::MenuHelp), |ui| {
                    if ui.button(tr(lang, Tr::AboutProgram)).clicked() {
                        app.dialogs.about.open = true;
                        ui.close();
                    }
                });
            });
        });
    });
    if !app.dialogs.any_open()
        && app.csv_import.is_none()
        && ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| panel.response.rect.contains(pos))
        })
    {
        super::regions::pointer_enter(&ctx, super::regions::Region::Header);
    }
    ctx.data_mut(|data| data.insert_temp(header_state_id(), focus));
}

fn theme_menu(
    app: &mut App,
    ui: &mut egui::Ui,
    lang: Lang,
    focus: &mut HeaderFocus,
    reject_keyboard: bool,
) {
    header_menu(ui, focus, 0, tr(lang, Tr::ThemeTitle), |ui| {
        for (theme, label) in [
            (Theme::System, Tr::ThemeSystem),
            (Theme::Light, Tr::ThemeLight),
            (Theme::Dark, Tr::ThemeDark),
        ] {
            let response =
                ui.selectable_label(app.settings.view.options.theme == theme, tr(lang, label));
            if response.clicked_by(egui::PointerButton::Primary)
                || (response.clicked() && !reject_keyboard)
            {
                app.settings.view.options.theme = theme;
                // Preserve other unsaved options while preventing a stale draft
                // from reverting the theme on Save.
                app.dialogs.options.draft.theme = theme;
                app.persist_settings();
                ui.close();
                ui.ctx().request_repaint();
            }
        }
    });
}

/// Displayed label for each language option in the language menu. Each option
/// is an endonym (see `Lang::endonym`), not a translation.
pub(super) fn lang_label(lang: Lang) -> &'static str {
    lang.endonym()
}
