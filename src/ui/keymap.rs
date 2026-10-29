//! Defaults for legacy contextual keyboard commands. Global commands have
//! precedence over tree/table commands; tree and table cannot own focus together.
use egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Scope {
    Global,
    Tree,
    Table,
    TreeAndTable,
    TableOrUnfocused,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KeyAction {
    #[cfg(test)]
    CatalogAction,
    PreviousSession,
    NextSession,
    SwitchPane,
    Refresh,
    TreeEnter,
    TableWrite,
    SelectAll,
    Copy,
    Delete,
    TreeUp,
    TreeDown,
    TreeLeft,
    TreeRight,
    TreeToggle,
    TableUp,
    TableDown,
    TableLeft,
    TableRight,
    TablePageUp,
    TablePageDown,
    TableHome,
    TableEnd,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Binding {
    pub action: KeyAction,
    pub scope: Scope,
    pub key: Key,
    pub modifiers: Modifiers,
}
const fn b(action: KeyAction, scope: Scope, key: Key, modifiers: Modifiers) -> Binding {
    Binding {
        action,
        scope,
        key,
        modifiers,
    }
}
pub(super) const DEFAULTS: &[Binding] = &[
    b(
        KeyAction::PreviousSession,
        Scope::Global,
        Key::Tab,
        Modifiers {
            alt: false,
            ctrl: false,
            shift: true,
            mac_cmd: false,
            command: true,
        },
    ),
    b(
        KeyAction::PreviousSession,
        Scope::Global,
        Key::PageUp,
        Modifiers::COMMAND,
    ),
    b(
        KeyAction::NextSession,
        Scope::Global,
        Key::Tab,
        Modifiers::COMMAND,
    ),
    b(
        KeyAction::NextSession,
        Scope::Global,
        Key::PageDown,
        Modifiers::COMMAND,
    ),
    b(
        KeyAction::SwitchPane,
        Scope::Global,
        Key::Tab,
        Modifiers::NONE,
    ),
    b(
        KeyAction::SwitchPane,
        Scope::Global,
        Key::Tab,
        Modifiers::SHIFT,
    ),
    b(
        KeyAction::SwitchPane,
        Scope::Global,
        Key::F6,
        Modifiers::NONE,
    ),
    b(
        KeyAction::SwitchPane,
        Scope::Global,
        Key::F6,
        Modifiers::SHIFT,
    ),
    b(
        KeyAction::Refresh,
        Scope::TreeAndTable,
        Key::F5,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TreeEnter,
        Scope::Tree,
        Key::Enter,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableWrite,
        Scope::Table,
        Key::Enter,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableWrite,
        Scope::Table,
        Key::F2,
        Modifiers::NONE,
    ),
    b(
        KeyAction::SelectAll,
        Scope::Table,
        Key::A,
        Modifiers::COMMAND,
    ),
    b(KeyAction::Copy, Scope::Table, Key::C, Modifiers::COMMAND),
    b(
        KeyAction::Delete,
        Scope::TableOrUnfocused,
        Key::Delete,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TreeUp,
        Scope::Tree,
        Key::ArrowUp,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TreeDown,
        Scope::Tree,
        Key::ArrowDown,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TreeLeft,
        Scope::Tree,
        Key::ArrowLeft,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TreeRight,
        Scope::Tree,
        Key::ArrowRight,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TreeToggle,
        Scope::Tree,
        Key::Space,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableUp,
        Scope::Table,
        Key::ArrowUp,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableDown,
        Scope::Table,
        Key::ArrowDown,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableLeft,
        Scope::Table,
        Key::ArrowLeft,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableRight,
        Scope::Table,
        Key::ArrowRight,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TablePageUp,
        Scope::Table,
        Key::PageUp,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TablePageDown,
        Scope::Table,
        Key::PageDown,
        Modifiers::NONE,
    ),
    b(
        KeyAction::TableHome,
        Scope::Table,
        Key::Home,
        Modifiers::NONE,
    ),
    b(KeyAction::TableEnd, Scope::Table, Key::End, Modifiers::NONE),
];

pub(super) fn bindings(action: KeyAction) -> impl Iterator<Item = &'static Binding> {
    DEFAULTS
        .iter()
        .filter(move |binding| binding.action == action)
}

/// Legacy contextual handlers have their own focus and modifier rules.
pub(super) fn key(action: KeyAction) -> Option<Key> {
    bindings(action).next().map(|binding| binding.key)
}

// Preserve legacy Control on macOS as well as the platform command modifier.
pub(super) fn modified_key_pressed(ctx: &egui::Context, action: KeyAction) -> bool {
    let Some(key) = key(action) else { return false };
    ctx.input_mut(|input| {
        let mut hit = false;
        input.events.retain(|event| match event {
            egui::Event::Copy if action == KeyAction::Copy => {
                hit = true;
                false
            }
            egui::Event::Key {
                key: event_key,
                pressed: true,
                modifiers,
                ..
            } if *event_key == key && (modifiers.command || modifiers.ctrl) => {
                hit = true;
                false
            }
            _ => true,
        });
        hit
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn overlaps(a: Scope, b: Scope) -> bool {
        use Scope::*;
        matches!(
            (a, b),
            (Global, _)
                | (_, Global)
                | (Tree, Tree)
                | (Table, Table)
                | (TreeAndTable, _)
                | (_, TreeAndTable)
                | (TableOrUnfocused, Table | TableOrUnfocused)
                | (Table, TableOrUnfocused)
        )
    }
    // Legacy context keys (other than Mod+A/C) ignore extra modifiers.
    // Global Mod+PageUp/Down deliberately wins over table paging.
    fn conflict(a: &Binding, b: &Binding) -> bool {
        if !overlaps(a.scope, b.scope) || a.key != b.key {
            return false;
        }
        if a.action == b.action {
            return a.modifiers == b.modifiers;
        }
        let wildcard = |binding: &Binding| {
            binding.scope != Scope::Global
                && !matches!(binding.action, KeyAction::SelectAll | KeyAction::Copy)
        };
        let legacy_ctrl = |binding: &Binding, other: &Binding| {
            matches!(binding.action, KeyAction::SelectAll | KeyAction::Copy) && other.modifiers.ctrl
        };
        let overlaps_modifiers = legacy_ctrl(a, b)
            || legacy_ctrl(b, a)
            || a.modifiers.matches_logically(b.modifiers)
            || b.modifiers.matches_logically(a.modifiers)
            || wildcard(a)
            || wildcard(b);
        let page_priority = |global: &Binding, table: &Binding| {
            global.scope == Scope::Global
                && global.modifiers == Modifiers::COMMAND
                && table.scope == Scope::Table
                && table.modifiers == Modifiers::NONE
                && matches!(
                    (global.action, table.action, global.key),
                    (
                        KeyAction::PreviousSession,
                        KeyAction::TablePageUp,
                        Key::PageUp
                    ) | (
                        KeyAction::NextSession,
                        KeyAction::TablePageDown,
                        Key::PageDown
                    )
                )
        };
        let global_page_priority = page_priority(a, b) || page_priority(b, a);
        let tab_priority = |previous: &Binding, next: &Binding| {
            previous.scope == Scope::Global
                && next.scope == Scope::Global
                && previous.key == Key::Tab
                && previous.action == KeyAction::PreviousSession
                && next.action == KeyAction::NextSession
                && next.modifiers == Modifiers::COMMAND
                && previous.modifiers
                    == Modifiers {
                        shift: true,
                        ..Modifiers::COMMAND
                    }
        };
        let global_tab_priority = tab_priority(a, b) || tab_priority(b, a);
        overlaps_modifiers && !global_page_priority && !global_tab_priority
    }
    #[test]
    fn every_catalog_shortcut_has_no_context_conflict() {
        use opclook_contract::actions::CATALOG;
        for spec in CATALOG {
            if let Some(shortcut) = super::super::shortcuts::action_shortcut(spec.id) {
                let global = b(
                    KeyAction::CatalogAction,
                    Scope::Global,
                    shortcut.logical_key,
                    shortcut.modifiers,
                );
                for binding in DEFAULTS {
                    assert!(
                        !conflict(&global, binding),
                        "{} conflicts with {binding:?}",
                        spec.semantic_id
                    );
                }
            }
        }
        // Verify that the cross-catalog check detects actual overlapping keys.
        let catalog = b(
            KeyAction::CatalogAction,
            Scope::Global,
            Key::A,
            Modifiers::COMMAND,
        );
        let select = b(
            KeyAction::SelectAll,
            Scope::Table,
            Key::A,
            Modifiers::COMMAND,
        );
        assert!(conflict(&catalog, &select));
    }
    #[test]
    fn defaults_have_no_conflicts_in_overlapping_scopes() {
        for (index, a) in DEFAULTS.iter().enumerate() {
            for b in DEFAULTS.iter().skip(index + 1) {
                assert!(!conflict(a, b), "{a:?} conflicts with {b:?}");
            }
        }
        let previous = DEFAULTS
            .iter()
            .position(|item| item.action == KeyAction::PreviousSession && item.key == Key::Tab)
            .expect("previous tab");
        let next = DEFAULTS
            .iter()
            .position(|item| item.action == KeyAction::NextSession && item.key == Key::Tab)
            .expect("next tab");
        assert!(
            previous < next,
            "more specific global Tab must be checked first"
        );
        let previous_same = b(
            KeyAction::PreviousSession,
            Scope::Global,
            Key::Tab,
            Modifiers::COMMAND,
        );
        let next_same = b(
            KeyAction::NextSession,
            Scope::Global,
            Key::Tab,
            Modifiers::COMMAND,
        );
        assert!(conflict(&previous_same, &next_same));
        let plain_page = b(
            KeyAction::PreviousSession,
            Scope::Global,
            Key::PageUp,
            Modifiers::NONE,
        );
        let table_page = b(
            KeyAction::TablePageUp,
            Scope::Table,
            Key::PageUp,
            Modifiers::NONE,
        );
        assert!(conflict(&plain_page, &table_page));
        let delete = b(
            KeyAction::Delete,
            Scope::TableOrUnfocused,
            Key::Delete,
            Modifiers::NONE,
        );
        let table_delete = b(
            KeyAction::TableUp,
            Scope::Table,
            Key::Delete,
            Modifiers::NONE,
        );
        assert!(conflict(&delete, &table_delete));
        let select = b(
            KeyAction::SelectAll,
            Scope::Table,
            Key::A,
            Modifiers::COMMAND,
        );
        let ctrl_a = b(KeyAction::TableUp, Scope::Table, Key::A, Modifiers::CTRL);
        assert!(conflict(&select, &ctrl_a));
        let a = b(KeyAction::TreeUp, Scope::Tree, Key::N, Modifiers::NONE);
        let other = b(KeyAction::Refresh, Scope::Tree, Key::N, Modifiers::COMMAND);
        assert!(conflict(&a, &other));
        let other = b(KeyAction::TableUp, Scope::Table, Key::N, Modifiers::NONE);
        assert!(!conflict(&a, &other));
    }
}
