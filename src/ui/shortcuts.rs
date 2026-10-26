use super::table::handle_table_keys;
use super::tree::{refresh_selected, toggle_tree_item, visible_tree_indices};
use crate::app::App;
use crate::bff::model::{NodeKind, SessionId};
use crate::i18n::Tr;

pub(super) fn handle_shortcuts(app: &mut App, ui: &mut egui::Ui) {
    // A focused text field or a modal dialog takes over the keyboard, so the
    // tag tree must no longer own it. (`text_edit_focused` iff a text edit has
    // egui keyboard focus; plain button/checkbox focus is harmless and must
    // NOT drop the tree focus or arrow keys would stop working after a click.)
    if ui.ctx().text_edit_focused() {
        app.tree_focused = false;
        return;
    }
    if app.dialogs.any_open() {
        app.tree_focused = false;
        return;
    }
    if app.table_focus.is_some() {
        handle_table_keys(app, ui);
        return;
    }
    if app.tree_focused {
        handle_tree_keys(app, ui);
        return;
    }
    // Selection can exist without a focused table row (e.g. right after Ctrl+A,
    // which clears table_focus). A Delete key must still act on that selection.
    if !app.selected_rows.is_empty() && ui.ctx().input(|i| i.key_pressed(egui::Key::Delete)) {
        app.remove_selected_rows();
    }
}
pub(super) fn handle_tree_keys(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx();
    let Some((session, focus_id)) = app.tree_focus else {
        // Clicked empty tree space: no node focused yet. Arrow keys pick the
        // first (or last) visible node so navigation can begin.
        let session = match app.selected_tree {
            Some((sid, _)) => sid,
            None => app.active_session.unwrap_or(0),
        };
        let Some(view) = app.sessions.iter().find(|s| s.id == session) else {
            return;
        };
        let ids: Vec<u64> = visible_tree_indices(view)
            .into_iter()
            .map(|i| view.tree[i].id)
            .collect();
        if ids.is_empty() {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            app.set_tree_focus(session, ids[0]);
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            app.set_tree_focus(session, ids[ids.len() - 1]);
        }
        return;
    };
    let visible_ids: Vec<u64> = {
        let Some(view) = app.sessions.iter().find(|s| s.id == session) else {
            return;
        };
        visible_tree_indices(view)
            .into_iter()
            .map(|i| view.tree[i].id)
            .collect()
    };
    if visible_ids.is_empty() {
        return;
    }
    let Some(pos) = visible_ids.iter().position(|&id| id == focus_id) else {
        return;
    };
    let lo = 0usize;
    let hi = visible_ids.len() - 1;
    let expanded = app
        .tree_item(session, focus_id)
        .map(|item| item.expanded)
        .unwrap_or(false);
    let has_children = app
        .tree_item(session, focus_id)
        .map(|item| item.node.has_children)
        .unwrap_or(false);
    let parent = app
        .tree_item(session, focus_id)
        .and_then(|item| item.parent);

    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) && pos < hi {
        app.set_tree_focus(session, visible_ids[pos + 1]);
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) && pos > lo {
        app.set_tree_focus(session, visible_ids[pos - 1]);
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
        if expanded {
            toggle_tree_item(app, session, focus_id);
        } else if let Some(parent) = parent {
            app.set_tree_focus(session, parent);
        }
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
        if expanded {
            if let Some(&first_child) = visible_ids
                .get(pos + 1)
                .filter(|&&c| app.tree_item(session, c).map(|i| i.parent) == Some(Some(focus_id)))
            {
                app.set_tree_focus(session, first_child);
            }
        } else if has_children {
            toggle_tree_item(app, session, focus_id);
        }
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
        toggle_tree_item(app, session, focus_id);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        tree_enter(app, session, focus_id);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
        refresh_selected(app);
    }
}

/// Enter on a focused tree node:
/// - Branch -> toggle expand/collapse.
/// - Item already subscribed (a table row with the same session + item id)
///   -> open the write dialog for that row (immediate value edit).
/// - Item not subscribed yet -> add it to the table (same as double-click);
///   no write dialog (a write would race AddItems).
pub(super) fn tree_enter(app: &mut App, session: SessionId, id: u64) {
    let Some(item) = app.tree_item(session, id) else {
        return;
    };
    if item.node.kind == NodeKind::Branch {
        toggle_tree_item(app, session, id);
        return;
    }
    let node_id = item.node.node_id.clone();
    let name = item.node.name.clone();
    if let Some(row_key) = app.tree_row_key(session, &node_id) {
        app.open_write_dialog(vec![row_key]);
    } else {
        app.add_items(session, vec![(node_id, name)]);
        app.journal.info_l(Tr::SignalAdded, &[]);
    }
}
