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

/// All declared bindings in display order, using the same registry as dispatch.
pub(super) fn binding_texts(ctx: &egui::Context, action: super::keymap::KeyAction) -> String {
    super::keymap::bindings(action)
        .map(|binding| {
            ctx.format_shortcut(&egui::KeyboardShortcut::new(binding.modifiers, binding.key))
        })
        .collect::<Vec<_>>()
        .join(" / ")
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
    let owner = super::regions::current(&ctx);
    let editing_footer = ctx.text_edit_focused() && owner == Some(super::regions::Region::Footer);
    if (ctx.text_edit_focused() && !editing_footer)
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
            .any(|event| matches!(event, egui::Event::PointerButton { .. }))
    }) {
        return;
    }
    let take = |action| {
        super::keymap::bindings(action)
            .any(|binding| ctx.input_mut(|i| i.consume_key(binding.modifiers, binding.key)))
    };

    use super::keymap::KeyAction;
    let is_mac = ctx.os().is_mac();
    let open_palette = !editing_footer
        && super::keymap::bindings(KeyAction::OpenPalette).any(|binding| {
            ctx.input_mut(|input| {
                let mut hit = false;
                input.events.retain(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat,
                        modifiers,
                        ..
                    } if *key == binding.key
                        && modifiers.matches_exact(binding.modifiers)
                        && !(is_mac && modifiers.ctrl) =>
                    {
                        hit |= !repeat;
                        false
                    }
                    _ => true,
                });
                hit
            })
        });
    if open_palette {
        app.open_command_palette();
        return;
    }

    let shortcut = action_shortcut(crate::app::actions::UiAction::OpenConnectDialog.id());
    let hit = !editing_footer
        && shortcut.is_some_and(|shortcut| {
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
    if !editing_footer && take(KeyAction::PreviousSession) {
        app.cycle_session(false);
        return;
    }
    if !editing_footer && take(KeyAction::NextSession) {
        app.cycle_session(true);
        return;
    }
    let reverse = ctx.input(|input| input.modifiers.shift);
    let pane_switch = take(KeyAction::SwitchPane);
    if pane_switch {
        // egui already read the Tab for its widget focus walk at the start of
        // the pass; cancel it so no button silently takes the keyboard.
        ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        egui::Popup::close_all(&ctx);
        if let Some(id) = ctx.memory(|memory| memory.focused()) {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
        let region = super::regions::step(owner, reverse);
        super::regions::activate(&ctx, region);
        app.tree_focused = false;
        app.table_focus = None;
        match region {
            super::regions::Region::Tags => {
                if app.focus_table()
                    && let Some(index) = app.table_focus
                {
                    super::table::request_table_scroll(&ctx, index);
                }
            }
            super::regions::Region::Servers => super::navigation::enter_keyboard_region(app, &ctx),
            super::regions::Region::Header | super::regions::Region::Footer => {}
        }
    }
}

pub(super) fn handle_shortcuts(app: &mut App, ui: &mut egui::Ui) {
    // A focused text field or a modal dialog takes over the keyboard, so the
    // tag tree must no longer own it. (`text_edit_focused` iff a text edit has
    // egui keyboard focus; plain button/checkbox focus is harmless and must
    // NOT drop the tree focus or arrow keys would stop working after a click.)
    if app.dialogs.any_open() || app.csv_import.is_some() {
        // Suspend region ownership while a dialog is active; preserve the
        // underlying tree cursor so it resumes after the dialog closes.
        return;
    }
    if ui.ctx().text_edit_focused() || super::ime_blocks_keys(ui.ctx()) {
        app.tree_focused = false;
        return;
    }
    if ui.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerButton { .. }))
    }) {
        // A pointer gesture may change the destination in this same frame.
        return;
    }
    // While the tree or the table owns the keyboard, no button may hold egui
    // focus: Enter/Space would press it and arrows would walk the widgets.
    let owner = super::regions::current(ui.ctx()).or_else(|| {
        if app.tree_focused {
            Some(super::regions::Region::Servers)
        } else if app.table_focus.is_some() {
            Some(super::regions::Region::Tags)
        } else {
            None
        }
    });
    if (owner == Some(super::regions::Region::Servers) && app.tree_focused)
        || (owner == Some(super::regions::Region::Tags) && app.table_focus.is_some())
    {
        let ctx = ui.ctx();
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }
    if owner == Some(super::regions::Region::Servers) {
        if app.tree_focused {
            handle_tree_keys(app, ui);
        } else {
            super::navigation::handle_server_region_keys(app, ui);
        }
        return;
    }
    if owner == Some(super::regions::Region::Tags) && app.table_focus.is_some() {
        handle_table_keys(app, ui);
        return;
    }
    // Selection can exist without a focused table row (e.g. right after Ctrl+A,
    // which clears table_focus). A Delete key must still act on that selection.
    if owner == Some(super::regions::Region::Tags)
        && !app.selected_rows.is_empty()
        && !ui.input(|input| {
            input.events.iter().any(|event| {
                matches!(event,
            egui::Event::Key { key, pressed: true, .. } if *key != egui::Key::Delete)
            })
        })
        && super::keymap::key(super::keymap::KeyAction::Delete).is_some_and(|key| {
            ui.input(|input| {
                input.events.iter().any(|event| {
                    matches!(event,
                egui::Event::Key { key: event_key, pressed: true, repeat: false, modifiers, .. }
                    if *event_key == key && modifiers.is_none())
                })
            })
        })
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
        } else {
            super::navigation::enter_keyboard_region(app, ctx);
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
    let moved = ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::ArrowDown
                        | egui::Key::ArrowUp
                        | egui::Key::ArrowLeft
                        | egui::Key::ArrowRight,
                    pressed: true,
                    ..
                }
            )
        })
    });
    if !moved
        && app.tree_focus == Some((session, focus_id))
        && super::keymap::key(super::keymap::KeyAction::TreeEnter).is_some_and(|key| {
            !crate::ui::ime_blocks_keys(ctx)
                && ctx.input(|input| {
                    input.events.iter().any(|event| {
                        matches!(event,
            egui::Event::Key { key: event_key, pressed: true, repeat: false, modifiers, .. }
                if *event_key == key && modifiers.is_none())
                    })
                })
        })
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

#[cfg(test)]
mod display_tests {
    use super::*;
    use crate::i18n::{Lang, Tr, fmt};
    use crate::ui::keymap::KeyAction;

    #[test]
    fn local_bindings_feed_localized_shortcut_hints() {
        let ctx = egui::Context::default();
        ctx.set_os(egui::os::OperatingSystem::Windows);
        let refresh = binding_text(&ctx, KeyAction::Refresh);
        let write = binding_texts(&ctx, KeyAction::TableWrite);
        let switch = binding_texts(&ctx, KeyAction::SwitchPane);
        assert_eq!(refresh, "F5");
        assert_eq!(write, "Enter / F2");
        assert!(switch.contains("Tab"));
        assert!(switch.contains("F6"));
        for lang in [Lang::Ru, Lang::En, Lang::Zh] {
            for text in [
                fmt(lang, Tr::HintRefreshF5, &[&refresh]),
                fmt(lang, Tr::HintEnterF2, &[&write]),
                fmt(
                    lang,
                    Tr::HintDelSelected,
                    &[&binding_text(&ctx, KeyAction::Delete)],
                ),
                fmt(lang, Tr::RefreshF5Button, &[&refresh]),
                fmt(
                    lang,
                    Tr::TableEmptyKeys,
                    &[&connect_shortcut_text(&ctx), &switch],
                ),
                fmt(lang, Tr::TreeHint, &[&switch, &write, &refresh, &switch]),
                fmt(
                    lang,
                    Tr::RowsHint,
                    &[&switch, &switch, &switch, &write, &write, &refresh, &switch],
                ),
                fmt(
                    lang,
                    Tr::KeyboardIdleHint,
                    &[&connect_shortcut_text(&ctx), &switch],
                ),
            ] {
                assert!(!text.contains('{') && !text.contains('}'));
                assert!(!text.contains('?'));
            }
        }
    }
}
