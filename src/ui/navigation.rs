use super::actions::{TreeAction, TreeDragPayload};
use super::colors::yellow;
use super::files::save_tree_csv;
use super::paint::{
    paint_pause_icon, paint_play_icon, paint_reconnect_icon, painter_icon_button, painter_x_button,
};
use super::servers::{request_remove_session, server_state_color, state_text};
use super::test_hooks::record_server_row_rect;
#[cfg(test)]
use super::test_hooks::set_tree_row_rect_for_test;
use super::tree::{
    open_add_signal, refresh_selected, request_disconnect, toggle_tree_item, visible_tree_indices,
};
use super::truncate_text;
use crate::app::{App, ReconnectState, SessionState, TreeItem};
use crate::bff::model::{NodeKind, Protocol, SessionId};
use crate::constants::{
    DOT_LEAF, DOT_SERVER_STATE, ICON_SERVER_REMOVE, ICON_SERVER_ROW, ICON_TOOLBAR,
    LEFT_PANEL_DEFAULT_WIDTH, RECONNECT_MAX_ATTEMPTS, TABLE_ROW_HEIGHT, TRUNCATE_SERVER_ROW,
};
use crate::i18n::{Lang, Tr, fmt, tr};

pub(super) fn navigation(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    // Opaque frame so the left panel paints its full allocated rect (no
    // transparent seam next to the central table while resizing).
    let side_frame = egui::Frame::side_top_panel(&ui.style()).fill(ui.style().visuals.panel_fill);
    egui::Panel::left("navigation")
        .resizable(true)
        .default_size(LEFT_PANEL_DEFAULT_WIDTH)
        .frame(side_frame)
        .show(ui, |ui| {
            // Toolbar: connect / disconnect / reconnect with correct enabled
            let active_state = app
                .active_session
                .and_then(|id| app.sessions.iter().find(|s| s.id == id))
                .map(|s| s.state);
            let (can_connect, can_disconnect, can_reconnect) = match active_state {
                None => (false, false, false),
                Some(SessionState::Connecting) => (false, true, true),
                Some(SessionState::Connected) => (true, true, true),
                Some(SessionState::Disconnected) => (true, false, true),
            };
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_connect, egui::Button::new(tr(lang, Tr::Connect)))
                    .clicked()
                {
                    app.open_connect_dialog();
                }
                if ui
                    .add_enabled(can_disconnect, egui::Button::new(tr(lang, Tr::Disconnect)))
                    .clicked()
                {
                    if let Some(session) = app.active_session {
                        request_disconnect(app, session);
                    }
                }
                if painter_icon_button(
                    ui,
                    ICON_TOOLBAR,
                    can_reconnect,
                    tr(lang, Tr::ReconnectTooltip),
                    paint_reconnect_icon,
                ) && let Some(session) = app.active_session
                {
                    app.reconnect_session(session);
                }
            });
            ui.separator();
            ui.label(egui::RichText::new(tr(lang, Tr::Servers)).strong());

            // Full-width «Добавить сервер» opens the connect dialog.
            if ui
                .add(
                    egui::Button::new(tr(lang, Tr::AddServer))
                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                )
                .clicked()
            {
                app.open_connect_dialog();
            }
            ui.add_space(4.0);

            // Server containers as a 3-column table: state dot | name | ×.
            let mut activate = None;
            let mut to_remove = None;
            let mut to_resume = None;
            let mut to_stop = None;
            let mut reconnect = None;

            let snapshot: Vec<(
                SessionId,
                String,
                SessionState,
                String,
                bool,
                ReconnectState,
            )> = app
                .sessions
                .iter()
                .map(|s| {
                    (
                        s.id,
                        s.display_name.clone(),
                        s.state,
                        s.info.clone(),
                        s.protocol() == Protocol::OpcDa,
                        s.reconnect,
                    )
                })
                .collect();

            if !snapshot.is_empty() {
                // One stable single-line row per server container: state dot |
                // (reconnect/stop/resume controls) | name (flexible, truncated) | ×.
                // The name never wraps, so the row height stays <= 28 px.
                for (id, name, state, info, is_da, reconnect_state) in &snapshot {
                    let selected = app.active_session == Some(*id);
                    let tooltip = format!(
                        "{} | {} | {}",
                        if *is_da { "OPC DA" } else { "OPC UA" },
                        info,
                        state_text(*state, lang)
                    );
                    let full_tooltip = format!("{}\n{}", name, tooltip);
                    let auto = app.settings.options.auto_reconnect;
                    let reconnecting = *state == SessionState::Connecting;
                    let exhausted = matches!(reconnect_state, ReconnectState::Exhausted);
                    let stopped = matches!(reconnect_state, ReconnectState::Stopped);
                    let waiting = matches!(reconnect_state, ReconnectState::Waiting { .. });
                    let loop_active = reconnecting || waiting;
                    let can_auto = auto && *state == SessionState::Disconnected;
                    let cap = RECONNECT_MAX_ATTEMPTS;
                    let attempts = if exhausted {
                        cap
                    } else {
                        reconnect_state.attempt()
                    };

                    let awaitable = ui
                        .horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            // State dot (~18px). It is painted (no font glyph);
                            // a "●" accessible name lets UI tests find it and
                            // keeps screen-reader symmetry with the tab strip.
                            let dot = server_state_color(*state);
                            let (dot_rect, dot_resp) = ui.allocate_exact_size(
                                egui::vec2(DOT_SERVER_STATE, DOT_SERVER_STATE),
                                egui::Sense::hover(),
                            );
                            dot_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Label, true, "●")
                            });
                            ui.painter().circle_filled(dot_rect.center(), 4.0, dot);

                            // Reconnect / stop / resume controls that never wrap the name.
                            if reconnecting || can_auto {
                                let count =
                                    fmt(lang, Tr::ReconnectCount, &[&attempts.max(1), &cap]);
                                if ui
                                    .small_button(count)
                                    .on_hover_text(tr(lang, Tr::ReconnectTooltip))
                                    .clicked()
                                {
                                    reconnect = Some(*id);
                                }
                            }
                            if loop_active {
                                if painter_icon_button(
                                    ui,
                                    ICON_SERVER_ROW,
                                    true,
                                    tr(lang, Tr::StopAttempts),
                                    paint_pause_icon,
                                ) {
                                    to_stop = Some(*id);
                                }
                            } else if can_auto && (stopped || exhausted) {
                                if painter_icon_button(
                                    ui,
                                    ICON_SERVER_ROW,
                                    true,
                                    tr(lang, Tr::ResumeAttempts),
                                    paint_play_icon,
                                ) {
                                    to_resume = Some(*id);
                                }
                            }

                            // Name: flexible single-line, truncated, with tooltip.
                            // Capped so a wide label cannot feed back into the
                            // left panel's auto-size (which would widen the panel
                            // by ~28px every frame); truncation clips it anyway.
                            let name_width = ui.available_width().clamp(84.0, 300.0);
                            let short = truncate_text(name, TRUNCATE_SERVER_ROW);
                            let response = ui.add_sized(
                                egui::vec2(name_width, TABLE_ROW_HEIGHT),
                                egui::Button::selectable(selected, short).truncate(),
                            );
                            response.clone().on_hover_text(&full_tooltip);
                            if response.double_clicked() {
                                activate = Some(*id);
                            } else if response.clicked() {
                                activate = Some(*id);
                            }

                            // Destructive × at the row end (~24px).
                            if painter_x_button(
                                ui,
                                ICON_SERVER_REMOVE,
                                tr(lang, Tr::RemoveServerTooltip),
                            ) {
                                to_remove = Some(*id);
                            }
                        })
                        .response;
                    // Assert single-line row: keep the height tight for the
                    // test hook (function is a no-op except under #[cfg(test)]).
                    record_server_row_rect(*id, awaitable.rect);
                }
            } else {
                ui.label(tr(lang, Tr::NoActiveServer));
            }

            if let Some(id) = activate {
                app.activate_container(id);
            }
            if let Some(id) = reconnect {
                app.resume_reconnect(id);
                app.reconnect_session(id);
            }
            if let Some(id) = to_resume {
                app.resume_reconnect(id);
            }
            if let Some(id) = to_stop {
                app.stop_reconnect(id);
            }
            if let Some(id) = to_remove {
                request_remove_session(app, id);
            }

            ui.separator();
            let Some(session_id) = app.active_session else {
                ui.label(tr(lang, Tr::ConnectPrompt));
                return;
            };
            let Some(view) = app.sessions.iter().find(|s| s.id == session_id) else {
                return;
            };
            let protocol = view.protocol();
            let root_loading = view.root_loading;
            let connected = view.state == SessionState::Connected;

            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        connected && !root_loading,
                        egui::Button::new(tr(lang, Tr::Refresh)),
                    )
                    .clicked()
                {
                    refresh_selected(app);
                }
                if ui
                    .add_enabled(connected, egui::Button::new(tr(lang, Tr::Add)))
                    .clicked()
                {
                    app.add_selected_tree();
                }
                if ui
                    .add_enabled(connected, egui::Button::new(tr(lang, Tr::AddChildren)))
                    .clicked()
                {
                    app.add_selected_children();
                }
                if ui
                    .add_enabled(connected, egui::Button::new(tr(lang, Tr::AddByNameShort)))
                    .clicked()
                {
                    open_add_signal(app);
                }
                if protocol == Protocol::OpcUa {
                    let mut fallback = false;
                    let mut only = app
                        .sessions
                        .iter_mut()
                        .find(|s| s.id == session_id)
                        .map(|s| &mut s.ua_browse_vars_only)
                        .unwrap_or(&mut fallback);
                    ui.checkbox(&mut only, tr(lang, Tr::OnlyVars));
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(fmt(lang, Tr::BrowseProto, &[&protocol.to_string()]))
                        .weak(),
                );
                if root_loading {
                    ui.spinner();
                    ui.label(tr(lang, Tr::UpdatingTree));
                }
                // Live progress of a recursive «Добавить дочерние» job.
                if let Some(job) = app.add_children.as_ref() {
                    if job.session == session_id
                        && (!job.in_flight.is_empty() || !job.queue.is_empty())
                    {
                        ui.separator();
                        ui.spinner();
                        ui.colored_label(
                            yellow(),
                            fmt(lang, Tr::AddChildrenProgress, &[&job.added.to_string()]),
                        );
                    }
                }
            });
            ui.separator();

            let mut actions = Vec::new();
            {
                let Some(view) = app.sessions.iter().find(|s| s.id == session_id) else {
                    return;
                };
                if view.tree.is_empty() {
                    if root_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr(lang, Tr::LoadingTree));
                        });
                    } else if connected {
                        ui.label(tr(lang, Tr::TreeNotLoaded));
                    } else {
                        ui.label(tr(lang, Tr::NoConnection));
                    }
                } else {
                    let selected = app.selected_tree;
                    let scroll_out = egui::ScrollArea::vertical()
                        .id_salt("tree_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for index in visible_tree_indices(view) {
                                let item = &view.tree[index];
                                draw_tree_item(ui, item, session_id, selected, &mut actions, lang);
                            }
                        });
                    // Clicking anywhere in the tree pane (including empty space
                    // below the items) gives the tree keyboard focus.
                    let clicked_tree = ui.input(|i| {
                        i.pointer.any_click()
                            && scroll_out
                                .inner_rect
                                .expand(2.0)
                                .contains(i.pointer.interact_pos().unwrap_or_default())
                    });
                    if clicked_tree {
                        app.tree_focused = true;
                        app.table_focus = None;
                        app.table_anchor = None;
                    }
                }
            }
            for action in actions {
                match action {
                    TreeAction::Toggle(id) => toggle_tree_item(app, session_id, id),
                    TreeAction::Select(id) => {
                        app.set_tree_focus(session_id, id);
                    }
                    TreeAction::Add(id) => {
                        app.set_tree_focus(session_id, id);
                        app.add_selected_tree();
                    }
                    TreeAction::Children(id) => {
                        app.set_tree_focus(session_id, id);
                        app.add_selected_children();
                    }
                    TreeAction::Refresh => refresh_selected(app),
                    TreeAction::CopyName(id) => {
                        if let Some(item) = app.tree_item(session_id, id) {
                            ui.ctx().copy_text(item.node.name.clone());
                            app.status = tr(app.lang(), Tr::NodeNameCopied).to_string();
                        }
                    }
                }
            }
        });
}

pub(super) fn draw_tree_item(
    ui: &mut egui::Ui,
    item: &TreeItem,
    session: SessionId,
    selected: Option<(SessionId, u64)>,
    actions: &mut Vec<TreeAction>,
    lang: Lang,
) {
    ui.horizontal(|ui| {
        ui.add_space(item.depth as f32 * 14.0);
        if item.node.has_children {
            let arrow = if item.expanded { "v" } else { ">" };
            if ui
                .add_sized([DOT_LEAF, DOT_LEAF], egui::Button::new(arrow).frame(false))
                .clicked()
            {
                actions.push(TreeAction::Toggle(item.id));
            }
        } else {
            ui.add_space(18.0);
        }

        let is_selected = selected == Some((session, item.id));
        let text = match item.node.kind {
            NodeKind::Branch => {
                egui::RichText::new(&item.node.name).color(egui::Color32::from_rgb(130, 180, 255))
            }
            NodeKind::Item => egui::RichText::new(&item.node.name),
        };
        // A full-width row button is both clickable/queryable and a reliable
        // drag source: the whole row (not just the label glyphs) senses
        // click_and_drag, so a mouse grab anywhere on the row starts a drag.
        // This is what the headless RawInput DnD proof drives.
        let response = ui
            .add_sized(
                [ui.available_width(), 20.0],
                egui::Button::selectable(is_selected, text).sense(egui::Sense::click_and_drag()),
            )
            .on_hover_text(&item.node.node_id);
        if item.loading {
            ui.spinner();
        }
        if response.clicked() {
            actions.push(TreeAction::Select(item.id));
        }
        if response.double_clicked() {
            if item.node.kind == NodeKind::Item {
                actions.push(TreeAction::Add(item.id));
            } else {
                actions.push(TreeAction::Toggle(item.id));
            }
        }
        // Announce this row as a drag source. egui only activates the payload
        // once the pointer moves past the drag threshold, so plain clicks keep
        // working. The tree has single selection, so the payload holds just
        // this node's id; `add_dropped_tree` accepts a list for multi-select.
        response.dnd_set_drag_payload(TreeDragPayload {
            session,
            node_ids: vec![item.id],
        });
        response.context_menu(|ui| {
            if ui.button(tr(lang, Tr::Add)).clicked() {
                actions.push(TreeAction::Select(item.id));
                actions.push(TreeAction::Add(item.id));
                ui.close();
            }
            if item.node.kind == NodeKind::Branch {
                if ui.button(tr(lang, Tr::AddChildrenSignals)).clicked() {
                    actions.push(TreeAction::Select(item.id));
                    actions.push(TreeAction::Children(item.id));
                    ui.close();
                }
            }
            if item.node.kind == NodeKind::Branch {
                if ui.button(tr(lang, Tr::RefreshBranch)).clicked() {
                    actions.push(TreeAction::Select(item.id));
                    actions.push(TreeAction::Refresh);
                    ui.close();
                }
            }
            if item.node.kind == NodeKind::Branch {
                if ui.button(tr(lang, Tr::ExpandCollapse)).clicked() {
                    actions.push(TreeAction::Toggle(item.id));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr(lang, Tr::CopyNodeName)).clicked() {
                actions.push(TreeAction::CopyName(item.id));
                ui.close();
            }
        });

        #[cfg(test)]
        set_tree_row_rect_for_test(response.rect);
    });
}

/// Shared «tags area» (table background) context menu. It is opened from the
/// «Сигналов: N | Выбрано: M» strip and the empty table body, which never
/// overlap a data row or a column header, so the row/header context menus always
/// win on their own widgets.
pub(super) fn tags_context_menu(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    if ui.button(tr(lang, Tr::CtxColumns)).clicked() {
        app.dialogs.columns.open = true;
        ui.close();
    }
    if ui.button(tr(lang, Tr::CtxAddByName)).clicked() {
        open_add_signal(app);
        ui.close();
    }
    if ui.button(tr(lang, Tr::CtxRefreshBranch)).clicked() {
        refresh_selected(app);
        ui.close();
    }
    ui.separator();
    if ui.button(tr(lang, Tr::CtxSaveCsv)).clicked() {
        save_tree_csv(app);
        ui.close();
    }
    ui.separator();
    if ui.button(tr(lang, Tr::CtxSelectAll)).clicked() {
        app.select_all_rows();
        ui.close();
    }
    if ui.button(tr(lang, Tr::CtxClearSelection)).clicked() {
        app.clear_selection();
        ui.close();
    }
}
