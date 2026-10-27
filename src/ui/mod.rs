mod actions;
mod colors;
mod csv;
mod dialogs;
mod files;
mod menu;
mod navigation;
mod paint;
mod panels;
mod servers;
mod shortcuts;
mod status;
mod table;
mod test_hooks;
mod tree;

pub use actions::TreeDragPayload;
pub use csv::{
    export_tree_csv, export_tree_csv_to_path, import_tree_csv, import_tree_csv_from_path,
    start_csv_import,
};
pub use servers::top_tab_panel_enabled;

#[cfg(test)]
pub use test_hooks::{
    columns_hook_api, dnd_rects_snapshot, reset_ui_test_hooks, server_rows_for_test,
    set_tags_rect_for_test, set_tree_row_rect_for_test, table_scroll_for_test, tags_rect_for_test,
    tree_row_rect_for_test,
};

pub fn draw(app: &mut crate::app::App, ui: &mut egui::Ui) {
    shortcuts::keyboard_prepass(app, ui);
    menu::menu(app, ui);
    status::status_bar(app, ui);
    panels::bottom_panels(app, ui);
    navigation::navigation(app, ui);
    table::table_panel(app, ui);
    shortcuts::handle_shortcuts(app, ui);
    status::status_popup(app, ui);
    dialogs::dialogs(app, ui);
}

/// Truncates a long label to `max` characters, appending a Unicode ellipsis.
pub(super) fn truncate_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max).collect();
        format!("{cut}…")
    }
}
