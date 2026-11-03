mod about;
mod acquisition;
mod add_signal;
mod columns;
mod confirm;
mod connect;
mod csv_import;
mod message;
mod network;
mod options;
mod palette;
mod ua_tree;
mod write;

pub(super) use connect::{security_mode_label, security_policy_label};

use about::about_dialog;
use add_signal::add_signal_dialog;
use columns::columns_dialog;
use confirm::confirm_dialog;
use connect::connect_dialog;
use csv_import::csv_import_dialog;
use message::message_dialog;
use options::options_dialog;
use write::write_dialog;

use crate::app::App;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogRegion {
    Message,
    Import,
    Confirm,
    Write,
    Add,
    Options,
    Columns,
    About,
    Acquisition,
    Connect,
}

fn active_dialog(app: &App) -> Option<DialogRegion> {
    if app.dialogs.message.is_some() {
        Some(DialogRegion::Message)
    } else if app.csv_import.is_some() {
        Some(DialogRegion::Import)
    } else if app.dialogs.confirm.is_some() {
        Some(DialogRegion::Confirm)
    } else if app.dialogs.write.open {
        Some(DialogRegion::Write)
    } else if app.dialogs.add_signal.open {
        Some(DialogRegion::Add)
    } else if app.dialogs.options.open {
        Some(DialogRegion::Options)
    } else if app.dialogs.columns.open {
        Some(DialogRegion::Columns)
    } else if app.dialogs.about.open {
        Some(DialogRegion::About)
    } else if app.dialogs.acquisition.open {
        Some(DialogRegion::Acquisition)
    } else if app.dialogs.connect.open {
        Some(DialogRegion::Connect)
    } else {
        None
    }
}

pub(super) fn dialogs(app: &mut App, ui: &mut egui::Ui) {
    // Exactly one active dialog owns this frame's keyboard. Background state
    // remains intact and is drawn again after its child closes. Messages take
    // precedence, so asynchronous errors cannot be hidden behind a prompt.
    let region = active_dialog(app);
    let region_id = egui::Id::unique("opclook_active_dialog_region");
    let previous = ui
        .ctx()
        .data(|data| data.get_temp::<Option<DialogRegion>>(region_id).flatten());
    ui.ctx()
        .data_mut(|data| data.insert_temp(region_id, region));
    if region.is_some() && region != previous {
        // The old region may have received this frame's input before the new
        // one became active. Defer the new dialog to a clean input frame.
        ui.ctx().request_repaint();
        return;
    }
    if app.dialogs.message.is_some() {
        if let Some(confirm) = app.dialogs.confirm.as_mut() {
            confirm.focus_pending = true;
        } else if app.dialogs.write.open {
            app.dialogs.write.focus_pending = true;
        }
        message_dialog(app, ui);
    } else if app.csv_import.is_some() {
        if let Some(confirm) = app.dialogs.confirm.as_mut() {
            confirm.focus_pending = true;
        } else if app.dialogs.write.open {
            app.dialogs.write.focus_pending = true;
        }
        csv_import_dialog(app, ui);
    } else if app.dialogs.confirm.is_some() {
        confirm_dialog(app, ui, true);
    } else if app.dialogs.write.open {
        write_dialog(app, ui);
        if app.dialogs.confirm.is_some() {
            ui.ctx().request_repaint();
        }
    } else if app.dialogs.add_signal.open {
        add_signal_dialog(app, ui);
    } else if app.dialogs.options.open {
        options_dialog(app, ui);
    } else if app.dialogs.columns.open {
        columns_dialog(app, ui);
    } else if app.dialogs.about.open {
        about_dialog(app, ui);
    } else if app.dialogs.acquisition.open {
        acquisition::acquisition_dialog(app, ui);
    } else if app.dialogs.connect.open {
        connect_dialog(app, ui);
    }
}

/// Consistent item spacing/padding for dialogs (UI polish pass).
pub(super) fn dialog_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
}

pub(super) fn palette(app: &mut App, ui: &mut egui::Ui) {
    palette::palette_dialog(app, ui);
}
