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
    MoveColumn(ColumnId, ColumnDir),
    DropColumn(ColumnId, usize),
    /// V3: expand / collapse a structured value node of a row.
    ToggleValue(ItemKey, Vec<u32>),
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

/// Render one GUI action using its read-only availability and localized reason.
pub(super) fn action_button(
    app: &crate::app::App,
    ui: &mut egui::Ui,
    action: crate::app::actions::UiAction,
    button: egui::Button<'_>,
) -> egui::Response {
    let reason = app.ui_action_unavailable(action);
    let response = ui.add_enabled(reason.is_none(), button);
    if let Some(reason) = reason {
        let text = crate::i18n::tr(app.lang(), reason);
        response.on_hover_text(text).on_disabled_hover_text(text)
    } else {
        response
    }
}
