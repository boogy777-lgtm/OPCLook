mod actions;
mod colors;
mod components;
mod csv;
mod dialogs;
mod files;
mod keymap;
mod menu;
mod navigation;
mod paint;
mod panels;
mod servers;
mod shortcuts;
mod status;
mod style;
mod table;
mod test_hooks;
mod tree;

pub use actions::TreeDragPayload;
pub use csv::{
    export_tree_csv, export_tree_csv_to_path, import_tree_csv, import_tree_csv_from_path,
    start_csv_import,
};
pub use servers::top_tab_panel_enabled;
pub use style::apply_style;

#[cfg(test)]
pub use test_hooks::{
    columns_hook_api, dnd_rects_snapshot, reset_ui_test_hooks, server_rows_for_test,
    set_tags_rect_for_test, set_tree_row_rect_for_test, table_scroll_for_test, tags_rect_for_test,
    tree_row_rect_for_test,
};

pub fn draw(app: &mut crate::app::App, ui: &mut egui::Ui) {
    style::sync_style(ui.ctx(), app.settings.view.options.theme);
    colors::set_dark(ui.visuals().dark_mode);
    update_ime_state(ui.ctx());
    // A modal must receive keyboard input before background dialogs.
    // Afterwards, remove keyboard events so their global handlers cannot act
    // on the same key, even if a system dialog arrived while it was open.
    let palette_was_open = app.dialogs.palette.open;
    if palette_was_open {
        // Surface newly arrived dialogs instead of hiding them behind the palette.
        if app.dialogs.any_open_except_palette() || app.csv_import.is_some() {
            app.dialogs.palette.open = false;
        } else {
            dialogs::palette(app, ui);
        }
        ui.ctx().input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Ime(_)
                        | egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                )
            })
        });
    }
    shortcuts::keyboard_prepass(app, ui);
    menu::menu(app, ui);
    status::status_bar(app, ui);
    panels::bottom_panels(app, ui);
    navigation::navigation(app, ui);
    table::table_panel(app, ui);
    shortcuts::handle_shortcuts(app, ui);
    status::status_popup(app, ui);
    dialogs::dialogs(app, ui);
    if !palette_was_open {
        dialogs::palette(app, ui);
    }
}

// Keep the entire frame protected when composition ends with Enter.
fn update_ime_state(ctx: &egui::Context) {
    let id = egui::Id::unique("opclook-ime-state");
    let frame = ctx.cumulative_frame_nr();
    let (previous_frame, mut active, _) = ctx.data(|d| {
        d.get_temp::<(u64, bool, bool)>(id)
            .unwrap_or((u64::MAX, false, false))
    });
    if previous_frame == frame {
        return;
    }
    let mut blocked = active;
    ctx.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    active = !text.is_empty();
                    blocked = true;
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                    active = false;
                    blocked = true;
                }
                egui::Event::WindowFocused(false) => active = false,
                _ => {}
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, (frame, active, blocked)));
}

fn ime_blocks_keys(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<(u64, bool, bool)>(egui::Id::unique("opclook-ime-state"))
            .is_some_and(|(_, _, blocked)| blocked)
    })
}

// Keyboard-generated button clicks must not commit IME candidates. Pointer
// clicks remain deliberate even when a composition is active.
trait ImeSafeClick {
    fn ime_safe_clicked(&self) -> bool;
}
impl ImeSafeClick for egui::Response {
    fn ime_safe_clicked(&self) -> bool {
        self.clicked_by(egui::PointerButton::Primary)
            || (self.clicked() && !ime_blocks_keys(&self.ctx))
    }
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

/// True only for an initial press, never for an OS key-repeat event.
fn key_pressed_once(ctx: &egui::Context, key: egui::Key) -> bool {
    if ime_blocks_keys(ctx) {
        return false;
    }
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(event,
        egui::Event::Key { key: event_key, pressed: true, repeat: false, .. } if *event_key == key)
        })
    })
}

fn key_repeat(ctx: &egui::Context, key: egui::Key) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(event,
        egui::Event::Key { key: event_key, pressed: true, repeat: true, .. } if *event_key == key)
        })
    })
}
