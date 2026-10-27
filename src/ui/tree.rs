use crate::app::{App, SessionState};
use crate::bff::model::{NodeKind, SessionId};
use crate::i18n::{Tr, fmt, tr};

pub(super) fn visible_tree_indices(view: &crate::app::SessionView) -> Vec<usize> {
    let mut out = Vec::new();
    let mut hide_from: Option<usize> = None;
    for (index, item) in view.tree.iter().enumerate() {
        if let Some(depth) = hide_from {
            if item.depth > depth {
                continue;
            }
            hide_from = None;
        }
        out.push(index);
        if !item.expanded {
            hide_from = Some(item.depth);
        }
    }
    out
}

pub(super) fn toggle_tree_item(app: &mut App, session: SessionId, id: u64) {
    let mut browse = false;
    if let Some(view) = app.sessions.iter_mut().find(|s| s.id == session) {
        if let Some(item) = view.tree.iter_mut().find(|t| t.id == id) {
            item.expanded = !item.expanded;
            if item.expanded && !item.loaded && item.node.has_children {
                item.loading = true;
                browse = true;
            }
        }
    }
    if browse {
        // U7: the reply lands under THIS item, not the first node with the
        // same NodeId.
        app.browse_item(session, id);
    }
}

pub(super) fn refresh_selected(app: &mut App) {
    let Some(session) = app.active_session else {
        return;
    };
    let selected = app.selected_tree.and_then(|(sid, id)| {
        if sid == session {
            app.tree_item(session, id)
                .filter(|item| item.node.kind == NodeKind::Branch)
                .map(|item| item.id)
        } else {
            None
        }
    });
    if let Some(item_id) = selected {
        app.browse_item(session, item_id);
    } else {
        if let Some(view) = app.sessions.iter_mut().find(|s| s.id == session) {
            view.tree.clear();
            view.root_loaded = false;
        }
        app.browse(session, String::new());
    }
}

pub(super) fn open_add_signal(app: &mut App) {
    app.dialogs.add_signal.open = true;
    app.dialogs.add_signal.error = None;
    app.dialogs.add_signal.session = app.active_session;
    app.dialogs.add_signal.item_id.clear();
    app.dialogs.add_signal.display_name.clear();
    app.dialogs.add_signal.focus_pending = true;
}

/// Confirm disconnection for a live session (mirrors the original prompt
/// «Отключиться от сервера "%s"?»); a dead session is dropped immediately.
pub(super) fn request_disconnect(app: &mut App, session: SessionId) {
    let name = app
        .sessions
        .iter()
        .find(|s| s.id == session)
        .map(|s| s.endpoint.display_name())
        .unwrap_or_else(|| "?".to_string());
    let live = app
        .sessions
        .iter()
        .find(|s| s.id == session)
        .map(|s| s.state != SessionState::Disconnected)
        .unwrap_or(false);
    if !live {
        app.disconnect_session(session);
        return;
    }
    app.request_confirm(
        tr(app.lang(), Tr::DisconnectConfirmTitle).to_string(),
        fmt(app.lang(), Tr::DisconnectConfirmText, &[&name]),
        crate::app::ConfirmAction::Disconnect(session),
    );
}
