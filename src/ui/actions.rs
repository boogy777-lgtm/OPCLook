use crate::bff::model::{ItemKey, SessionId};
use crate::columns::ColumnId;

#[derive(Clone, Debug)]
pub(super) enum TreeAction {
    Toggle(u64),
    Select(u64),
    Add(u64),
    Children(u64),
    Refresh,
    CopyName(u64),
}

#[derive(Clone, Debug)]
pub(super) enum TableAction {
    SelectSingle(usize),
    SelectToggle(usize),
    SelectExtend(usize),
    Write(ItemKey),
    Remove(ItemKey),
    CopyValue(ItemKey),
    CopyFullName(ItemKey),
    SelectAll,
    ClearSelection,
    SaveCsv,
    AddSelectedTree,
    AddChildren,
    MoveColumn(ColumnId, ColumnDir),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColumnDir {
    Left,
    Right,
}

/// Drag-and-drop payload dragged from the tag tree onto the signal table.
/// Carries the owning session and the tree node ids (the whole dragged
/// selection). Must be `Any + Send + Sync` for egui's DnD machinery.
#[derive(Clone, Debug)]
pub struct TreeDragPayload {
    pub session: SessionId,
    pub node_ids: Vec<u64>,
}
