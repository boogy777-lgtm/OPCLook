//! What the signal table shows (V1): the tags of the active session only.
//!
//! Every session (one server endpoint) has its own tag list. The table lists
//! the rows of the active tab; with no active tab it lists all rows, so a row
//! never disappears without a tab that owns it being picked.
//!
//! `table_focus` / `table_anchor` are positions in this visible list, not in
//! `rows`. Selection never reaches rows of another session: switching the tab
//! drops the selection, so Delete or a write can only act on rows the user sees.

use super::*;

impl App {
    /// Whether the table shows `row` (it belongs to the active session).
    pub fn row_shown(&self, row: &Row) -> bool {
        match self.active_session {
            Some(session) => row.session == session,
            None => true,
        }
    }

    /// Positions in `rows` of the rows the table shows, in table order.
    pub fn visible_row_positions(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.row_shown(row))
            .map(|(position, _)| position)
            .collect()
    }

    /// Number of rows the table shows.
    pub fn visible_row_count(&self) -> usize {
        match self.active_session {
            Some(session) => self.rows.iter().filter(|r| r.session == session).count(),
            None => self.rows.len(),
        }
    }

    /// Drop selection and focus that belong to another tab. Called every frame
    /// before the table and the shortcuts see the state; work is done only
    /// when the active tab changed.
    pub fn sync_table_view(&mut self) {
        if self.table_view_session == self.active_session {
            return;
        }
        self.table_view_session = self.active_session;
        let shown: HashSet<ItemKey> = self
            .rows
            .iter()
            .filter(|row| self.row_shown(row))
            .map(|row| row.key)
            .collect();
        self.selected_rows.retain(|key| shown.contains(key));
        self.table_focus = None;
        self.table_anchor = None;
    }

    /// Switch to the next (`forward`) or previous visible session tab.
    pub fn cycle_session(&mut self, forward: bool) {
        let visible: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.id)
            .collect();
        if visible.is_empty() {
            return;
        }
        let current = self
            .active_session
            .and_then(|id| visible.iter().position(|v| *v == id));
        let next = match current {
            None => 0,
            Some(index) if forward => (index + 1) % visible.len(),
            Some(index) => (index + visible.len() - 1) % visible.len(),
        };
        if let Some(id) = visible.get(next) {
            self.active_session = Some(*id);
            self.selected_tree = None;
            self.tree_focus = None;
        }
    }

    /// Give the keyboard to the signal table: focus the first selected visible
    /// row, else the first row. Returns false when the table is empty.
    pub fn focus_table(&mut self) -> bool {
        let visible = self.visible_row_positions();
        if visible.is_empty() {
            return false;
        }
        let selected = visible.iter().position(|p| {
            self.rows
                .get(*p)
                .is_some_and(|row| self.selected_rows.contains(&row.key))
        });
        match selected {
            Some(index) => {
                self.table_focus = Some(index);
                self.table_anchor = Some(index);
            }
            None => self.select_only(0),
        }
        self.tree_focused = false;
        true
    }

    /// Give the keyboard to the tag tree of the active session: its selected
    /// node, else its first visible node. Returns false without a tree.
    pub fn focus_tree(&mut self) -> bool {
        let Some(session) = self.active_session else {
            return false;
        };
        let Some(view) = self.sessions.iter().find(|s| s.id == session) else {
            return false;
        };
        let target = match self.selected_tree {
            Some((sid, id)) if sid == session && view.tree.iter().any(|t| t.id == id) => Some(id),
            // The first node is a root, and roots are always visible.
            _ => view.tree.first().map(|t| t.id),
        };
        match target {
            Some(id) => {
                self.set_tree_focus(session, id);
                true
            }
            None => false,
        }
    }
}
