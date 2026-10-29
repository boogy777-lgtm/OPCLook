use crate::i18n::Tr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnId {
    Status,
    FullName,
    WriteType,
    Type,
    Value,
    Quality,
    Time,
    Server,
    Description,
}

impl ColumnId {
    pub const ALL: [ColumnId; 9] = [
        Self::Status,
        Self::FullName,
        Self::WriteType,
        Self::Type,
        Self::Value,
        Self::Quality,
        Self::Time,
        Self::Server,
        Self::Description,
    ];

    /// Canonical Russian column title of the CSV format.
    ///
    /// These are the exact byte streams used by the CSV export/import format.
    /// The UI must NOT use this directly — use `title_tr()` for the localized
    /// header. `csv_hint()` is the matching tooltip (also part of the CSV
    /// format, not localized).
    pub fn csv_title(self) -> &'static str {
        match self {
            Self::Status => "",
            Self::FullName => "Полное имя",
            Self::WriteType => "Записываемый тип",
            Self::Type => "Тип",
            Self::Value => "Значение",
            Self::Quality => "Качество",
            Self::Time => "Время изменения",
            Self::Server => "OРС сервер",
            Self::Description => "Описание",
        }
    }

    pub fn csv_hint(self) -> &'static str {
        match self {
            Self::Status => "Состояние строки",
            Self::FullName => "Полное имя сигнала в сервере",
            Self::WriteType => "Тип сигнала, записываемый пользователем",
            Self::Type => "Тип сигнала в сервере",
            Self::Value => "Текущее значение сигнала в сервере",
            Self::Quality => "Характеристика достоверности значения сигнала",
            Self::Time => "Время последнего изменения сигнала",
            Self::Server => "Программный идентификатор сервера",
            Self::Description => "Описание сигнала",
        }
    }

    /// Localized column title (used by the UI). The Status column's indicator
    /// has no title.
    pub fn title_tr(self) -> Tr {
        match self {
            Self::Status => Tr::ColStatus,
            Self::FullName => Tr::ColFullName,
            Self::WriteType => Tr::ColWriteType,
            Self::Type => Tr::ColType,
            Self::Value => Tr::ColValue,
            Self::Quality => Tr::ColQuality,
            Self::Time => Tr::ColTime,
            Self::Server => Tr::ColServer,
            Self::Description => Tr::ColDescription,
        }
    }

    /// Localized column hint shown in the columns dialog.
    pub fn hint_tr(self) -> Tr {
        match self {
            Self::Status => Tr::HintStatus,
            Self::FullName => Tr::HintFullName,
            Self::WriteType => Tr::HintWriteType,
            Self::Type => Tr::HintType,
            Self::Value => Tr::HintValue,
            Self::Quality => Tr::HintQuality,
            Self::Time => Tr::HintTime,
            Self::Server => Tr::HintServer,
            Self::Description => Tr::HintDescription,
        }
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Status => 28.0,
            Self::FullName => 240.0,
            Self::WriteType => 110.0,
            Self::Type => 100.0,
            Self::Value => 170.0,
            Self::Quality => 110.0,
            Self::Time => 150.0,
            Self::Server => 160.0,
            Self::Description => 180.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColumnSet {
    pub order: Vec<ColumnId>,
    pub hidden: Vec<ColumnId>,
}

impl Default for ColumnSet {
    fn default() -> Self {
        Self {
            order: ColumnId::ALL.to_vec(),
            hidden: Vec::new(),
        }
    }
}

impl ColumnSet {
    pub fn visible(&self) -> impl Iterator<Item = ColumnId> + '_ {
        self.order
            .iter()
            .copied()
            .filter(|id| !self.hidden.contains(id))
    }

    pub fn is_visible(&self, id: ColumnId) -> bool {
        !self.hidden.contains(&id)
    }

    pub fn set_visible(&mut self, id: ColumnId, visible: bool) {
        if visible {
            self.hidden.retain(|c| *c != id);
            if !self.order.contains(&id) {
                self.order.push(id);
            }
        } else if !self.hidden.contains(&id) {
            self.hidden.push(id);
        }
    }

    pub fn move_up(&mut self, id: ColumnId) {
        if let Some(index) = self.order.iter().position(|c| *c == id) {
            if index > 0 {
                self.order.swap(index - 1, index);
            }
        }
    }

    pub fn move_down(&mut self, id: ColumnId) {
        if let Some(index) = self.order.iter().position(|c| *c == id) {
            if index + 1 < self.order.len() {
                self.order.swap(index, index + 1);
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Move `id` so it lands immediately before the column currently at the
    /// absolute `before` index (0-based). `before == self.order.len()` moves it
    /// to the end. Removing the dragged column first shifts later indices down,
    /// which is taken into account. Returns `true` if an actual move happened.
    pub fn move_to_insert(&mut self, id: ColumnId, before: usize) -> bool {
        let Some(src) = self.order.iter().position(|c| *c == id) else {
            return false;
        };
        // A stale/invalid insertion index must never panic or duplicate a column.
        let before = before.min(self.order.len());
        let target = if src < before { before - 1 } else { before };
        if target == src {
            return false;
        }
        self.order.remove(src);
        self.order.insert(target, id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TITLES: [&str; 9] = [
        "",
        "Полное имя",
        "Записываемый тип",
        "Тип",
        "Значение",
        "Качество",
        "Время изменения",
        "OРС сервер", // Cyrillic Р, as in the canonical CSV header.
        "Описание",
    ];

    const HINTS: [&str; 9] = [
        "Состояние строки",
        "Полное имя сигнала в сервере",
        "Тип сигнала, записываемый пользователем",
        "Тип сигнала в сервере",
        "Текущее значение сигнала в сервере",
        "Характеристика достоверности значения сигнала",
        "Время последнего изменения сигнала",
        "Программный идентификатор сервера",
        "Описание сигнала",
    ];

    #[test]
    fn column_order_is_exact() {
        assert_eq!(
            ColumnId::ALL,
            [
                ColumnId::Status,
                ColumnId::FullName,
                ColumnId::WriteType,
                ColumnId::Type,
                ColumnId::Value,
                ColumnId::Quality,
                ColumnId::Time,
                ColumnId::Server,
                ColumnId::Description,
            ]
        );
        assert_eq!(ColumnId::ALL.len(), 9);
        // No duplicate ColumnIds in the canonical order.
        let mut seen = Vec::new();
        for id in ColumnId::ALL {
            assert!(!seen.contains(&id), "duplicate {id:?}");
            seen.push(id);
        }
    }

    #[test]
    fn column_titles_match_canonical_constants() {
        for (i, id) in ColumnId::ALL.iter().enumerate() {
            assert_eq!(id.csv_title(), TITLES[i], "title for {id:?}");
        }
        // Defaults -> user-facing headers.
        let titles: Vec<_> = ColumnId::ALL.iter().map(|id| id.csv_title()).collect();
        assert_eq!(titles, TITLES.to_vec());
        assert_eq!(ColumnId::Server.csv_title(), "OРС сервер");
        assert_eq!(ColumnId::Status.csv_title(), "");
    }

    #[test]
    fn column_hints_are_present() {
        for (i, id) in ColumnId::ALL.iter().enumerate() {
            assert_eq!(id.csv_hint(), HINTS[i], "hint for {id:?}");
            assert!(!id.csv_hint().is_empty());
        }
    }

    #[test]
    fn column_widths_are_positive() {
        for id in ColumnId::ALL {
            assert!(id.width() > 0.0, "width for {id:?}");
        }
    }

    #[test]
    fn columnset_default_shows_every_column() {
        let s = ColumnSet::default();
        assert_eq!(s.order, ColumnId::ALL.to_vec());
        assert!(s.hidden.is_empty());
        assert_eq!(s.visible().collect::<Vec<_>>(), ColumnId::ALL.to_vec());
        for id in ColumnId::ALL {
            assert!(s.is_visible(id));
        }
    }

    #[test]
    fn set_visible_hides_and_reshows() {
        let mut s = ColumnSet::default();
        s.set_visible(ColumnId::Value, false);
        assert!(!s.is_visible(ColumnId::Value));
        let visible: Vec<_> = s.visible().collect();
        assert!(!visible.contains(&ColumnId::Value));
        assert!(s.hidden.contains(&ColumnId::Value));

        s.set_visible(ColumnId::Value, true);
        assert!(s.is_visible(ColumnId::Value));
        assert!(!s.hidden.contains(&ColumnId::Value));
    }

    #[test]
    fn set_visible_appends_unknown_column_to_order() {
        let mut s = ColumnSet::default();
        s.order.clear();
        s.hidden.clear();
        s.set_visible(ColumnId::Value, true);
        assert_eq!(s.order, vec![ColumnId::Value]);
        assert!(s.is_visible(ColumnId::Value));
    }

    #[test]
    fn set_visible_is_idempotent() {
        let mut s = ColumnSet::default();
        s.set_visible(ColumnId::Time, false);
        let before = s.hidden.clone();
        s.set_visible(ColumnId::Time, false);
        assert_eq!(s.hidden, before, "hiding twice must not duplicate");

        let mut s2 = ColumnSet::default();
        s2.set_visible(ColumnId::Time, true);
        assert!(s2.hidden.is_empty(), "re-show must not leave hidden entry");
        let order = s2.order.clone();
        s2.set_visible(ColumnId::Time, true);
        assert_eq!(s2.order, order, "re-show must not duplicate order");
    }

    #[test]
    fn move_up_reorders() {
        let mut s = ColumnSet::default();
        s.move_up(ColumnId::Value); // index 4 -> 3
        assert_eq!(s.order[3], ColumnId::Value);
        assert_eq!(s.order[4], ColumnId::Type);
        // First element cannot move further up.
        s.move_up(ColumnId::Status);
        assert_eq!(s.order[0], ColumnId::Status);
    }

    #[test]
    fn move_down_reorders() {
        let mut s = ColumnSet::default();
        s.move_down(ColumnId::Value); // index 4 -> 5
        assert_eq!(s.order[5], ColumnId::Value);
        assert_eq!(s.order[4], ColumnId::Quality);
        // Last element cannot move further down.
        s.move_down(ColumnId::Description);
        assert_eq!(s.order[8], ColumnId::Description);
    }

    #[test]
    fn move_on_missing_column_is_noop() {
        let mut s = ColumnSet::default();
        s.move_up(ColumnId::Description); // already last
        s.move_down(ColumnId::Status); // already first
        // A column not in order at all.
        s.order.retain(|c| *c != ColumnId::Type);
        let snapshot = s.order.clone();
        s.move_up(ColumnId::Type);
        s.move_down(ColumnId::Type);
        assert_eq!(s.order, snapshot);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut s = ColumnSet::default();
        s.order = vec![ColumnId::Value];
        s.hidden = vec![ColumnId::Time];
        s.reset();
        assert_eq!(s.order, ColumnId::ALL.to_vec());
        assert!(s.hidden.is_empty());
    }

    #[test]
    fn move_to_insert_reorders_before_target() {
        let mut s = ColumnSet::default(); // Status,FullName,WriteType,Type,Value,...
        // Move Value (idx 4) before WriteType (idx 2) -> src 4 > before 2 -> target 2.
        assert!(s.move_to_insert(ColumnId::Value, 2));
        let expected: Vec<ColumnId> = vec![
            ColumnId::Status,
            ColumnId::FullName,
            ColumnId::Value,
            ColumnId::WriteType,
            ColumnId::Type,
            ColumnId::Quality,
            ColumnId::Time,
            ColumnId::Server,
            ColumnId::Description,
        ];
        assert_eq!(s.order, expected);
        // Every canonical column present exactly once.
        assert_eq!(s.order.len(), ColumnId::ALL.len());
        for id in ColumnId::ALL {
            assert_eq!(s.order.iter().filter(|c| **c == id).count(), 1, "{id:?}");
        }
    }

    #[test]
    fn move_to_insert_before_start_and_end() {
        // To front.
        let mut s = ColumnSet::default();
        assert!(s.move_to_insert(ColumnId::Description, 0));
        assert_eq!(s.order[0], ColumnId::Description);

        // To very end (before == len()).
        let mut t = ColumnSet::default();
        assert!(t.move_to_insert(ColumnId::Status, t.order.len()));
        assert_eq!(t.order[8], ColumnId::Status);
    }

    #[test]
    fn move_to_insert_out_of_range_goes_to_end() {
        let mut s = ColumnSet::default();
        assert!(s.move_to_insert(ColumnId::FullName, usize::MAX));
        assert_eq!(s.order.last(), Some(&ColumnId::FullName));
        assert_eq!(s.order.len(), ColumnId::ALL.len());
        assert!(!s.move_to_insert(ColumnId::FullName, usize::MAX));
    }

    #[test]
    fn move_to_insert_noop_and_missing() {
        // Dropping on its own gap is a no-op.
        let mut s = ColumnSet::default();
        let before = s.order.clone();
        let own = s.order.iter().position(|c| *c == ColumnId::Value).unwrap();
        assert!(!s.move_to_insert(ColumnId::Value, own));
        assert_eq!(s.order, before);

        // Missing column is a no-op.
        s.order.retain(|c| *c != ColumnId::Type);
        let snapshot = s.order.clone();
        assert!(!s.move_to_insert(ColumnId::Type, 0));
        assert_eq!(s.order, snapshot);
    }
}
