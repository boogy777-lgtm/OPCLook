use super::colors::{accent, red, yellow};
use super::files::{load_journal, load_script_file, save_journal, save_script_file};
use crate::app::{App, BottomTab};
use crate::constants::{
    BOTTOM_PANEL_DEFAULT_HEIGHT, BOTTOM_PANEL_MAX_HEIGHT, BOTTOM_PANEL_MIN_HEIGHT,
};
use crate::i18n::{Tr, fmt, tr};
use crate::journal::Level;

pub(super) fn bottom_panels(app: &mut App, ui: &mut egui::Ui) {
    let show_script = app.settings.options.show_script;
    let show_log = app.settings.options.show_log;
    if !show_script && !show_log {
        return;
    }
    let lang = app.lang();
    egui::Panel::bottom("bottom_workspace")
        .resizable(true)
        .default_size(BOTTOM_PANEL_DEFAULT_HEIGHT)
        .size_range(BOTTOM_PANEL_MIN_HEIGHT..=BOTTOM_PANEL_MAX_HEIGHT)
        .show(ui, |ui| {
            // Both panels enabled -> show real exclusive tabs (TPageControl-like).
            if show_script && show_log {
                let mut selected = app.bottom_tab;
                ui.horizontal(|ui| {
                    // UI7: the journal is the primary tab; the script tab
                    // comes second and its hint says the interpreter is absent.
                    let journal_tab = ui
                        .selectable_label(selected == BottomTab::Journal, tr(lang, Tr::JournalTab))
                        .on_hover_text(tr(lang, Tr::HintJournalTab));
                    let script_tab = ui
                        .selectable_label(selected == BottomTab::Script, tr(lang, Tr::ScriptTab))
                        .on_hover_text(tr(lang, Tr::HintScriptTabNotImpl));
                    if journal_tab.clicked() {
                        selected = BottomTab::Journal;
                    }
                    if script_tab.clicked() {
                        selected = BottomTab::Script;
                    }
                    // UI9: the active tab carries the accent colour underline.
                    let active = if selected == BottomTab::Journal {
                        &journal_tab
                    } else {
                        &script_tab
                    };
                    let rect = active.rect;
                    ui.painter().hline(
                        rect.x_range(),
                        rect.bottom() + 1.0,
                        egui::Stroke::new(2.0, accent()),
                    );
                    ui.separator();
                    match selected {
                        BottomTab::Script => {
                            if ui.button(tr(lang, Tr::SaveScriptLabel)).clicked() {
                                save_script_file(app);
                            }
                            if ui.button(tr(lang, Tr::LoadScriptLabel)).clicked() {
                                load_script_file(app);
                            }
                        }
                        // The journal view has its own toolbar (clear / save / load).
                        BottomTab::Journal => {}
                    }
                    if selected == BottomTab::Script {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let note = egui::RichText::new(tr(lang, Tr::ScriptsNotImpl)).weak();
                            ui.label(note).on_hover_text(app.script_protocol_note());
                        });
                    }
                });
                if selected != app.bottom_tab {
                    app.bottom_tab = selected;
                }
                ui.separator();
                match app.bottom_tab {
                    BottomTab::Script => script_view(app, ui),
                    BottomTab::Journal => journal_view(app, ui),
                }
            } else {
                // Single enabled panel -> no tab bar, just the panel.
                if show_script {
                    ui.horizontal(|ui| {
                        if ui.button(tr(lang, Tr::SaveScriptLabel)).clicked() {
                            save_script_file(app);
                        }
                        if ui.button(tr(lang, Tr::LoadScriptLabel)).clicked() {
                            load_script_file(app);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let note = egui::RichText::new(tr(lang, Tr::ScriptsNotImpl)).weak();
                            ui.label(note).on_hover_text(app.script_protocol_note());
                        });
                    });
                    ui.separator();
                    script_view(app, ui);
                } else {
                    journal_view(app, ui);
                }
            }
        });
}

pub(super) fn script_view(app: &mut App, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .id_salt("script_scroll")
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut app.script_text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(9),
            );
        });
}

pub(super) fn journal_view(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    ui.horizontal(|ui| {
        if ui.small_button(tr(lang, Tr::Clear)).clicked() {
            app.journal.clear();
        }
        if ui.small_button(tr(lang, Tr::SaveJournal)).clicked() {
            save_journal(app);
        }
        if ui.small_button(tr(lang, Tr::LoadJournal)).clicked() {
            load_journal(app);
        }
        ui.label(fmt(
            lang,
            Tr::EntriesCount,
            &[&app.journal.entries().len().to_string()],
        ));
    });
    egui::ScrollArea::vertical()
        .id_salt("journal_scroll")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in app.journal.entries() {
                let color = match entry.level {
                    Level::Info => ui.visuals().text_color(),
                    Level::Warning => yellow(),
                    Level::Error => red(),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(entry.time.format("%H:%M:%S%.3f").to_string())
                            .monospace()
                            .weak(),
                    );
                    ui.label(egui::RichText::new(entry.render(lang)).color(color));
                });
            }
        });
}
