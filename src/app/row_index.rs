//! Lookup index over `App::rows` (A4).
//!
//! UA subscriptions deliver one `Values` event per data change and a table can
//! hold thousands of rows, so `rows.iter().find(|r| r.key == key)` per value is
//! O(N^2) overall. The index maps `ItemKey -> position` and remembers which
//! `(session, item_id)` pairs exist (duplicate check for `add_items`).
//!
//! Structural changes invalidate this index explicitly.

use super::*;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct RowIndex {
    positions: HashMap<ItemKey, usize>,
    items: HashMap<SessionId, HashSet<String>>,
    /// `rows.len()` the index was built (or incrementally kept) for.
    built_for: Option<u64>,
}

impl RowIndex {
    pub(super) fn invalidate(&mut self) {
        self.built_for = None;
    }
}

impl App {
    fn rebuild_row_index(&mut self) {
        let index = &mut self.row_index;
        index.positions.clear();
        index.items.clear();
        index.positions.reserve(self.rows.len());
        for (position, row) in self.rows.iter().enumerate() {
            index.positions.insert(row.key, position);
            index
                .items
                .entry(row.session)
                .or_default()
                .insert(row.item_id.clone());
        }
        index.built_for = Some(self.rows_revision);
    }

    fn ensure_row_index(&mut self) {
        if self.row_index.built_for != Some(self.rows_revision) {
            self.rebuild_row_index();
        }
    }

    /// Position of the row with `key` in `rows`.
    pub(super) fn row_position(&mut self, key: ItemKey) -> Option<usize> {
        self.ensure_row_index();
        let position = self.row_index.positions.get(&key).copied()?;
        if self.rows.get(position).is_some_and(|row| row.key == key) {
            return Some(position);
        }
        // `rows` was reordered behind our back: rebuild once and retry.
        self.rebuild_row_index();
        self.row_index.positions.get(&key).copied()
    }

    /// Whether `session` already has a row for `item_id`.
    pub(super) fn has_row_for_item(&mut self, session: SessionId, item_id: &str) -> bool {
        self.ensure_row_index();
        self.row_index
            .items
            .get(&session)
            .is_some_and(|ids| ids.contains(item_id))
    }

    /// Append a row and keep the index current without a rebuild.
    pub(super) fn push_row(&mut self, row: Row) {
        self.ensure_row_index();
        let kept = self.row_index.built_for == Some(self.rows_revision);
        let position = self.rows.len();
        if kept {
            self.row_index.positions.insert(row.key, position);
            self.row_index
                .items
                .entry(row.session)
                .or_default()
                .insert(row.item_id.clone());
        }
        self.rows.push(row);
        self.rows_revision = self.rows_revision.wrapping_add(1);
        *self.visible_cache.borrow_mut() = None;
        self.row_index.built_for = if kept { Some(self.rows_revision) } else { None };
    }

    /// Remove all rows whose key is in `keys` (one pass, one rebuild later).
    pub(super) fn remove_rows(&mut self, keys: &HashSet<ItemKey>) {
        if keys.is_empty() {
            return;
        }
        self.rows.retain(|row| !keys.contains(&row.key));
        self.rows_changed();
    }
}
