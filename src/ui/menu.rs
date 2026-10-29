use super::files::{
    load_journal, load_script_file, load_tree_csv, save_journal, save_script_file, save_tree_csv,
};
use super::tree::{open_add_signal, refresh_selected, request_disconnect};
use crate::app::{App, MessageDialog};
use crate::i18n::{Lang, Tr, tr};
use crate::settings::Theme;

pub(super) fn menu(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    egui::Panel::top("menu_bar").show(ui, |ui| {
        let narrow = ui.available_width() < 700.0;
        if narrow {
            egui::MenuBar::new().ui(ui, |ui| theme_menu(app, ui, lang));
        }
        egui::MenuBar::new().ui(ui, |ui| {
            if !narrow {
                theme_menu(app, ui, lang);
            }
            ui.menu_button(tr(lang, Tr::MenuClient), |ui| {
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
                    let _ = app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
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

            ui.menu_button(tr(lang, Tr::MenuServers), |ui| {
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
                    let _ = app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
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

            ui.menu_button(tr(lang, Tr::MenuSignals), |ui| {
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
                if ui
                    .button(tr(lang, Tr::ChangeValue))
                    .on_hover_text(crate::i18n::fmt(
                        lang,
                        Tr::HintEnterF2,
                        &[&super::shortcuts::binding_texts(
                            ui.ctx(),
                            super::keymap::KeyAction::TableWrite,
                        )],
                    ))
                    .clicked()
                {
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
                    app.selected_rows = app.rows().iter().map(|row| row.key).collect();
                    ui.close();
                }
                if ui.button(tr(lang, Tr::ClearSelection)).clicked() {
                    app.selected_rows.clear();
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

            ui.menu_button(tr(lang, Tr::MenuScript), |ui| {
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

            ui.menu_button(tr(lang, Tr::MenuJournal), |ui| {
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

            ui.menu_button(tr(lang, Tr::MenuLanguage), |ui| {
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

            ui.menu_button(tr(lang, Tr::MenuHelp), |ui| {
                if ui.button(tr(lang, Tr::AboutProgram)).clicked() {
                    app.dialogs.about.open = true;
                    ui.close();
                }
            });
        });
    });
}

fn theme_menu(app: &mut App, ui: &mut egui::Ui, lang: Lang) {
    ui.menu_button(tr(lang, Tr::ThemeTitle), |ui| {
        for (theme, label) in [
            (Theme::System, Tr::ThemeSystem),
            (Theme::Light, Tr::ThemeLight),
            (Theme::Dark, Tr::ThemeDark),
        ] {
            if ui
                .selectable_label(app.settings.view.options.theme == theme, tr(lang, label))
                .clicked()
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
