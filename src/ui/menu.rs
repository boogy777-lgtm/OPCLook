use super::files::{
    load_journal, load_script_file, load_tree_csv, save_journal, save_script_file, save_tree_csv,
};
use super::tree::{open_add_signal, refresh_selected, request_disconnect};
use crate::app::{App, MessageDialog};
use crate::i18n::{Lang, Tr, tr};

pub(super) fn menu(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    egui::Panel::top("menu_bar").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr(lang, Tr::MenuClient), |ui| {
                if ui
                    .add(egui::Button::new(tr(lang, Tr::ConnectEllipsis)).shortcut_text("Ctrl+N"))
                    .clicked()
                {
                    app.open_connect_dialog();
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
                if ui
                    .button(tr(lang, Tr::ConnectEllipsis))
                    .on_hover_text(tr(lang, Tr::HintConnectUa))
                    .clicked()
                {
                    app.open_connect_dialog();
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
                    .on_hover_text(tr(lang, Tr::HintRefreshF5))
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
                if ui
                    .button(tr(lang, Tr::AddSelected))
                    .on_hover_text(tr(lang, Tr::HintEnterTree))
                    .clicked()
                {
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
                    .on_hover_text(tr(lang, Tr::HintDelSelected))
                    .clicked()
                {
                    app.remove_selected_rows();
                    ui.close();
                }
                if ui
                    .button(tr(lang, Tr::ChangeValue))
                    .on_hover_text(tr(lang, Tr::HintEnterF2))
                    .clicked()
                {
                    app.open_write_dialog(Vec::new());
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(tr(lang, Tr::SelectAll))
                    .on_hover_text(tr(lang, Tr::HintCtrlA))
                    .clicked()
                {
                    app.selected_rows = app.rows.iter().map(|row| row.key).collect();
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
                        &mut app.settings.options.show_script,
                        tr(lang, Tr::ShowScript),
                    )
                    .clicked()
                {
                    app.settings.save();
                }
                if ui
                    .checkbox(
                        &mut app.settings.options.show_log,
                        tr(lang, Tr::ShowJournal),
                    )
                    .clicked()
                {
                    app.settings.save();
                }
            });

            ui.menu_button(tr(lang, Tr::MenuJournal), |ui| {
                if ui
                    .checkbox(
                        &mut app.settings.options.show_log,
                        tr(lang, Tr::ShowWorkJournal),
                    )
                    .clicked()
                {
                    app.settings.save();
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

/// Displayed label for each language option in the language menu. Each option
/// is an endonym (see `Lang::endonym`), not a translation.
pub(super) fn lang_label(lang: Lang) -> &'static str {
    lang.endonym()
}
