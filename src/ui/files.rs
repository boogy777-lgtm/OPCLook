use super::csv::{export_tree_csv_to_path, start_csv_import};
use crate::app::App;
use crate::i18n::{Tr, fmt};

pub(super) fn save_script_file(app: &mut App) {
    let lang = app.lang();
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("script text", &["stx", "txt"])
        .set_file_name("script.stx")
        .save_file()
    {
        match std::fs::write(&path, &app.script_text) {
            Ok(()) => {
                app.journal
                    .info(fmt(lang, Tr::ScriptSavedPath, &[&path.display()]));
                app.save_script();
            }
            Err(err) => app.journal.error(fmt(
                lang,
                Tr::ScriptSaveErr,
                &[&path.display(), &err.to_string()],
            )),
        }
    }
}

pub(super) fn load_script_file(app: &mut App) {
    let lang = app.lang();
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("script text", &["stx", "txt"])
        .pick_file()
    {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                app.script_text = text;
                app.journal
                    .info(fmt(lang, Tr::ScriptLoadedPath, &[&path.display()]));
            }
            Err(err) => app.journal.error(fmt(
                lang,
                Tr::ScriptLoadErr,
                &[&path.display(), &err.to_string()],
            )),
        }
    }
}

pub(super) fn save_journal(app: &mut App) {
    let lang = app.lang();
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("log", &["log", "txt"])
        .set_file_name("OPCtools.log")
        .save_file()
    {
        match app.journal.save_localized(&path, lang) {
            Ok(()) => app
                .journal
                .info(fmt(lang, Tr::JournalSaved, &[&path.display()])),
            Err(err) => app
                .journal
                .error(fmt(lang, Tr::JournalSaveErr, &[&err.to_string()])),
        }
    }
}

pub(super) fn load_journal(app: &mut App) {
    let lang = app.lang();
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("log", &["log", "txt"])
        .pick_file()
    {
        match app.journal.load(&path) {
            Ok(count) => app
                .journal
                .info(fmt(lang, Tr::JournalLoaded, &[&count.to_string()])),
            Err(err) => app
                .journal
                .error(fmt(lang, Tr::JournalLoadErr, &[&err.to_string()])),
        }
    }
}

pub(super) fn save_tree_csv(app: &mut App) {
    let lang = app.lang();
    let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name("signals.csv")
        .save_file()
    else {
        return;
    };
    match export_tree_csv_to_path(app, &path) {
        Ok(()) => app
            .journal
            .info(fmt(lang, Tr::TableSaved, &[&path.display()])),
        Err(err) => app
            .journal
            .error(fmt(lang, Tr::CsvSaveErr, &[&err.to_string()])),
    }
}
pub(super) fn load_tree_csv(app: &mut App) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .pick_file()
    else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => start_csv_import(app, text),
        Err(err) => app.journal.error(fmt(
            app.lang(),
            Tr::CsvReadErr,
            &[&path.display(), &err.to_string()],
        )),
    }
}
