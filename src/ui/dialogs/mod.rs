mod about;
mod add_signal;
mod columns;
mod confirm;
mod connect;
mod csv_import;
mod message;
mod network;
mod options;
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

pub(super) fn dialogs(app: &mut App, ui: &mut egui::Ui) {
    connect_dialog(app, ui);
    add_signal_dialog(app, ui);
    write_dialog(app, ui);
    columns_dialog(app, ui);
    options_dialog(app, ui);
    about_dialog(app, ui);
    message_dialog(app, ui);
    confirm_dialog(app, ui);
    csv_import_dialog(app, ui);
}

/// Consistent item spacing/padding for dialogs (UI polish pass).
pub(super) fn dialog_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
}
