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
    pub fn visible_row_positions(&self) -> std::rc::Rc<Vec<usize>> {
        if let Some((revision, session, visible)) = &*self.visible_cache.borrow() {
            if *revision == self.rows_revision && *session == self.active_session {
                return visible.clone();
            }
        }
        let visible: std::rc::Rc<Vec<usize>> = std::rc::Rc::new(
            self.rows
                .iter()
                .enumerate()
                .filter(|(_, row)| self.row_shown(row))
                .map(|(position, _)| position)
                .collect(),
        );
        *self.visible_cache.borrow_mut() =
            Some((self.rows_revision, self.active_session, visible.clone()));
        visible
    }

    /// Number of rows the table shows.
    pub fn visible_row_count(&self) -> usize {
        self.visible_row_positions().len()
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
        let Some(view) = self.session(session) else {
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

/// UI4: how long a changed value stays lit in the table.
pub const CHANGE_FLASH: std::time::Duration = std::time::Duration::from_secs(1);

impl Row {
    /// UI4: whether `new` is a real change of the value this row shows. The
    /// first value of a row (still pending, a placeholder quality or no value
    /// yet) is not a change, nor is the same value arriving again (two NaN are
    /// the same value too).
    pub fn is_value_change(&self, new: &Value) -> bool {
        if self.pending
            || matches!(self.quality, RowQuality::NoData(_) | RowQuality::Waiting(_))
            || self.value == Value::Empty
        {
            return false;
        }
        match (&self.value, new) {
            (Value::F64(old), Value::F64(new)) => old != new && !(old.is_nan() && new.is_nan()),
            (old, new) => old != new,
        }
    }

    /// UI4: how lit the value cell is `now`: 1.0 right after a change, fading
    /// out to 0.0 over [`CHANGE_FLASH`]; 0.0 for a row that never changed.
    pub fn change_flash(&self, now: std::time::Instant) -> f32 {
        let Some(at) = self.changed_at else {
            return 0.0;
        };
        let t = now.saturating_duration_since(at).as_secs_f32() / CHANGE_FLASH.as_secs_f32();
        if t >= 1.0 {
            0.0
        } else {
            let left = 1.0 - t.max(0.0);
            left * left
        }
    }
}

/// One line of the signal table (V3): a signal row, or a field / element of
/// an expanded structured or array value under it.
#[derive(Clone, Debug, PartialEq)]
pub enum TableLine {
    Row {
        /// Position in the visible list (selection and keyboard use it).
        index: usize,
        /// Position in `rows`.
        position: usize,
    },
    Field {
        /// Visible index of the row this field belongs to.
        index: usize,
        key: ItemKey,
        /// Field indices from the row value down to this field.
        path: Vec<u32>,
        /// 1 for a direct field of the row value.
        depth: usize,
        name: String,
        value_text: String,
        /// Structure type name of a nested structure, else empty.
        type_text: String,
        expandable: bool,
        expanded: bool,
        /// UI5: a number: shown monospace and right-aligned.
        numeric: bool,
    },
}

/// UI5: whether a value is a number (right-aligned, monospace in the table).
pub fn is_numeric_value(value: &Value) -> bool {
    matches!(value, Value::I64(_) | Value::U64(_) | Value::F64(_))
}

/// At most this many field lines are built per frame; a huge array never
/// stalls the table.
pub const MAX_FIELD_LINES: usize = 10_000;

impl App {
    /// The table's lines for the visible rows, with the expanded values
    /// unfolded under their rows.
    pub fn table_lines(&self, visible: &std::rc::Rc<Vec<usize>>) -> std::rc::Rc<Vec<TableLine>> {
        if let Some((cached_visible, revision, lines)) = &*self.table_cache.borrow() {
            if *revision == self.detail_revision && std::rc::Rc::ptr_eq(cached_visible, visible) {
                return lines.clone();
            }
        }
        let mut lines = Vec::with_capacity(visible.len());
        let mut budget = MAX_FIELD_LINES;
        for (index, position) in visible.iter().enumerate() {
            lines.push(TableLine::Row {
                index,
                position: *position,
            });
            let Some(row) = self.rows.get(*position) else {
                continue;
            };
            if row.value.is_expandable() && self.expanded_values.contains(&(row.key, Vec::new())) {
                self.push_fields(&mut lines, &mut budget, index, row.key, &row.value, &[], 1);
            }
        }
        let lines = std::rc::Rc::new(lines);
        *self.table_cache.borrow_mut() =
            Some((visible.clone(), self.detail_revision, lines.clone()));
        lines
    }

    #[allow(clippy::too_many_arguments)]
    fn push_fields(
        &self,
        lines: &mut Vec<TableLine>,
        budget: &mut usize,
        index: usize,
        key: ItemKey,
        value: &Value,
        path: &[u32],
        depth: usize,
    ) {
        for child in 0..value.child_count() {
            if *budget == 0 {
                return;
            }
            let Some((name, field)) = value.child(child) else {
                break;
            };
            *budget -= 1;
            let mut child_path = path.to_vec();
            child_path.push(u32::try_from(child).unwrap_or(u32::MAX));
            let expandable = field.is_expandable();
            let expanded = expandable && self.expanded_values.contains(&(key, child_path.clone()));
            let type_text = match field {
                Value::Struct { type_name, .. } => type_name.clone(),
                _ => String::new(),
            };
            lines.push(TableLine::Field {
                index,
                key,
                path: child_path.clone(),
                depth,
                name: name.into_owned(),
                value_text: field.to_string(),
                type_text,
                expandable,
                expanded,
                numeric: is_numeric_value(field),
            });
            if expanded {
                self.push_fields(lines, budget, index, key, field, &child_path, depth + 1);
            }
        }
    }

    /// Expand / collapse one structured value node of a row.
    pub fn toggle_value(&mut self, key: ItemKey, path: Vec<u32>) {
        self.detail_revision = self.detail_revision.wrapping_add(1);
        let entry = (key, path);
        if !self.expanded_values.remove(&entry) {
            self.expanded_values.insert(entry);
        }
    }

    /// Keyboard → on the focused row: the first press opens the value, the
    /// next one opens every nested level.
    pub fn expand_focused_value(&mut self) {
        let Some(key) = self.table_focus.and_then(|i| self.row_key(i)) else {
            return;
        };
        let Some(value) = self.rows.iter().find(|r| r.key == key).map(|r| &r.value) else {
            return;
        };
        if !value.is_expandable() {
            return;
        }
        self.detail_revision = self.detail_revision.wrapping_add(1);
        if self.expanded_values.insert((key, Vec::new())) {
            return;
        }
        let mut budget = MAX_FIELD_LINES;
        let mut stack: Vec<(Vec<u32>, &Value)> = vec![(Vec::new(), value)];
        while let Some((path, node)) = stack.pop() {
            for child in 0..node.child_count() {
                if budget == 0 {
                    return;
                }
                let Some((_, field)) = node.child(child) else {
                    break;
                };
                budget -= 1;
                if field.is_expandable() {
                    let mut child_path = path.clone();
                    child_path.push(u32::try_from(child).unwrap_or(u32::MAX));
                    self.expanded_values.insert((key, child_path.clone()));
                    stack.push((child_path, field));
                }
            }
        }
    }

    /// Keyboard ← on the focused row: close its value with every level.
    pub fn collapse_focused_value(&mut self) {
        let Some(key) = self.table_focus.and_then(|i| self.row_key(i)) else {
            return;
        };
        self.detail_revision = self.detail_revision.wrapping_add(1);
        self.expanded_values.retain(|(k, _)| *k != key);
    }
}
