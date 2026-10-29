use super::table::handle_table_keys;
use super::tree::{refresh_selected, toggle_tree_item, visible_tree_indices};
use crate::app::App;
use crate::bff::model::{NodeKind, SessionId};
use crate::i18n::Tr;

/// Translate portable catalog defaults to egui's logical shortcut representation.
pub(super) fn action_shortcut(
    id: opclook_contract::actions::ActionId,
) -> Option<egui::KeyboardShortcut> {
    use opclook_contract::actions::{CATALOG, DefaultShortcut};
    CATALOG
        .iter()
        .find(|spec| spec.id == id)?
        .shortcut
        .map(|shortcut| match shortcut {
            DefaultShortcut::ModN => {
                egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N)
            }
        })
}

pub(super) fn binding_text(ctx: &egui::Context, action: super::keymap::KeyAction) -> String {
    super::keymap::bindings(action)
        .next()
        .map(|binding| {
            ctx.format_shortcut(&egui::KeyboardShortcut::new(binding.modifiers, binding.key))
        })
        .unwrap_or_default()
}

pub(super) fn connect_shortcut_text(ctx: &egui::Context) -> String {
    action_shortcut(crate::app::actions::UiAction::OpenConnectDialog.id())
        .map(|shortcut| ctx.format_shortcut(&shortcut))
        .unwrap_or_default()
}

/// Window-level keys, handled BEFORE the panels draw (K3) so egui's own Tab
/// focus walk never runs for them:
/// - Tab / Shift+Tab / F6: move the keyboard between the tag tree and the
///   signal table (with neither owning it, the table first, else the tree);
/// - Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+PgDn / Ctrl+PgUp: next / previous tab;
/// - Ctrl+N: open the connect dialog.
///
/// A focused text field or an open dialog keeps every key.
pub(super) fn keyboard_prepass(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if ctx.text_edit_focused()
        || app.dialogs.any_open()
        || app.csv_import.is_some()
        || super::ime_blocks_keys(&ctx)
    {
        return;
    }
    // Focus changes are applied later in the pass. Do not dispatch a shortcut
    // before a simultaneous pointer press can focus its destination widget.
    if ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerButton { pressed: true, .. }))
    }) {
        return;
    }
    let take = |action| {
        super::keymap::bindings(action)
            .any(|binding| ctx.input_mut(|i| i.consume_key(binding.modifiers, binding.key)))
    };

    let shortcut = action_shortcut(crate::app::actions::UiAction::OpenConnectDialog.id());
    let is_mac = ctx.os().is_mac();
    let hit = shortcut.is_some_and(|shortcut| {
        ctx.input_mut(|input| {
            let mut matched = false;
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } if *key == shortcut.logical_key
                    && modifiers.matches_exact(shortcut.modifiers)
                    && !(is_mac && modifiers.ctrl) =>
                {
                    matched |= !repeat;
                    false
                }
                _ => true,
            });
            matched
        })
    });
    if hit {
        let _ = app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
        return;
    }
    use super::keymap::KeyAction;
    if take(KeyAction::PreviousSession) {
        app.cycle_session(false);
        return;
    }
    if take(KeyAction::NextSession) {
        app.cycle_session(true);
        return;
    }
    let pane_switch = take(KeyAction::SwitchPane);
    if pane_switch {
        // egui already read the Tab for its widget focus walk at the start of
        // the pass; cancel it so no button silently takes the keyboard.
        ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        let in_table = app.table_focus.is_some() && !app.tree_focused;
        if in_table {
            if !app.focus_tree() {
                app.focus_table();
            }
        } else if app.tree_focused {
            if !app.focus_table() {
                app.focus_tree();
            }
        } else if !app.focus_table() {
            app.focus_tree();
        }
    }
}

pub(super) fn handle_shortcuts(app: &mut App, ui: &mut egui::Ui) {
    // A focused text field or a modal dialog takes over the keyboard, so the
    // tag tree must no longer own it. (`text_edit_focused` iff a text edit has
    // egui keyboard focus; plain button/checkbox focus is harmless and must
    // NOT drop the tree focus or arrow keys would stop working after a click.)
    if ui.ctx().text_edit_focused() || super::ime_blocks_keys(ui.ctx()) || app.csv_import.is_some()
    {
        app.tree_focused = false;
        return;
    }
    if app.dialogs.any_open() {
        app.tree_focused = false;
        return;
    }
    // While the tree or the table owns the keyboard, no button may hold egui
    // focus: Enter/Space would press it and arrows would walk the widgets.
    if app.tree_focused || app.table_focus.is_some() {
        let ctx = ui.ctx();
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }
    if app.tree_focused {
        handle_tree_keys(app, ui);
        return;
    }
    if app.table_focus.is_some() {
        handle_table_keys(app, ui);
        return;
    }
    // Selection can exist without a focused table row (e.g. right after Ctrl+A,
    // which clears table_focus). A Delete key must still act on that selection.
    if !app.selected_rows.is_empty()
        && super::keymap::key(super::keymap::KeyAction::Delete)
            .is_some_and(|key| ui.ctx().input(|i| i.key_pressed(key)))
    {
        app.remove_selected_rows();
    }
}
pub(super) fn handle_tree_keys(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx();
    let Some((session, focus_id)) = app.tree_focus else {
        // Clicked empty tree space: no node focused yet. Arrow keys pick the
        // first (or last) visible node so navigation can begin.
        let Some(session) = app.selected_tree.map(|(sid, _)| sid).or(app.active_session) else {
            return;
        };
        let Some(view) = app.sessions.iter().find(|s| s.id == session) else {
            return;
        };
        let ids: Vec<u64> = visible_tree_indices(view)
            .iter()
            .copied()
            .map(|i| view.tree()[i].id)
            .collect();
        if ids.is_empty() {
            return;
        }
        if super::keymap::key(super::keymap::KeyAction::TreeDown)
            .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
        {
            app.set_tree_focus(session, ids[0]);
        } else if super::keymap::key(super::keymap::KeyAction::TreeUp)
            .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
        {
            app.set_tree_focus(session, ids[ids.len() - 1]);
        }
        return;
    };
    let visible_ids: Vec<u64> = {
        let Some(view) = app.sessions.iter().find(|s| s.id == session) else {
            return;
        };
        visible_tree_indices(view)
            .iter()
            .copied()
            .map(|i| view.tree()[i].id)
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

    if super::keymap::key(super::keymap::KeyAction::TreeDown)
        .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
        && pos < hi
    {
        app.set_tree_focus(session, visible_ids[pos + 1]);
    } else if super::keymap::key(super::keymap::KeyAction::TreeUp)
        .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
        && pos > lo
    {
        app.set_tree_focus(session, visible_ids[pos - 1]);
    } else if super::keymap::key(super::keymap::KeyAction::TreeLeft)
        .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
    {
        if expanded {
            toggle_tree_item(app, session, focus_id);
        } else if let Some(parent) = parent {
            app.set_tree_focus(session, parent);
        }
    } else if super::keymap::key(super::keymap::KeyAction::TreeRight)
        .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
    {
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

    if super::keymap::key(super::keymap::KeyAction::TreeToggle)
        .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
    {
        toggle_tree_item(app, session, focus_id);
    }
    if super::keymap::key(super::keymap::KeyAction::TreeEnter)
        .is_some_and(|key| crate::ui::key_pressed_once(ctx, key))
    {
        tree_enter(app, session, focus_id);
    }
    if super::keymap::key(super::keymap::KeyAction::Refresh)
        .is_some_and(|key| ctx.input(|i| i.key_pressed(key)))
    {
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
