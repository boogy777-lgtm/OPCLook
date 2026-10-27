use super::actions::{TreeAction, TreeDragPayload};
use super::colors::yellow;
use super::files::save_tree_csv;
use super::paint::{
    paint_expander, paint_pause_icon, paint_play_icon, paint_reconnect_icon, painter_icon_button,
    painter_x_button,
};
use super::servers::{request_remove_session, security_suffix, server_state_color, state_text};
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
            // «Подключиться» is always available (first start: no sessions yet).
            let can_connect = true;
            let (can_disconnect, can_reconnect) = match active_state {
                None => (false, false),
                Some(SessionState::Connecting) => (true, true),
                Some(SessionState::Connected) => (true, true),
                Some(SessionState::Disconnected) => (false, true),
            };
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_connect, egui::Button::new(tr(lang, Tr::Connect)))
                    .clicked()
                {
                    app.toolbar_connect();
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
                    // V2: one server's endpoints differ by security only.
                    let name = match security_suffix(&s.endpoint, lang) {
                        Some((short, _)) => format!("{} · {short}", s.display_name),
                        None => s.display_name.clone(),
                    };
                    (
                        s.id,
                        name,
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

            let mut vars_only_changed: Option<bool> = None;
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
                    let mut only = app
                        .sessions
                        .iter()
                        .find(|s| s.id == session_id)
                        .is_some_and(|s| s.ua_browse_vars_only);
                    if ui.checkbox(&mut only, tr(lang, Tr::OnlyVars)).changed() {
                        // U5: the filter applies to what is loaded, so the
                        // tree is re-read.
                        vars_only_changed = Some(only);
                    }
                }
            });
            if let Some(only) = vars_only_changed {
                app.set_ua_browse_vars_only(session_id, only);
            }
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
                    // A12: only the rows inside the viewport are built (a branch
                    // can hold thousands of nodes and the panel repaints every
                    // 200 ms).
                    let visible = visible_tree_indices(view);
                    let scroll_out = egui::ScrollArea::vertical()
                        .id_salt("tree_scroll")
                        .auto_shrink([false, false])
                        .show_rows(ui, TREE_ROW_HEIGHT, visible.len(), |ui, range| {
                            for index in visible.get(range).into_iter().flatten() {
                                if let Some(item) = view.tree.get(*index) {
                                    draw_tree_item(
                                        ui,
                                        item,
                                        session_id,
                                        selected,
                                        &mut actions,
                                        lang,
                                    );
                                }
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

/// A single-line, left-aligned list row: soft fill when selected (with an
/// accent bar on the left when `bar`), a faint fill under the pointer, the text
/// truncated to the row. `label` is the accessible name. Used by the tag tree
/// and the servers list instead of a centred selectable button.
fn flat_row(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    sense: egui::Sense,
    selected: bool,
    text: egui::RichText,
    label: &str,
    bar: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if selected {
            visuals.selection.bg_fill.gamma_multiply(0.45)
        } else if response.hovered() {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        if fill != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, 3.0, fill);
        }
        if selected && bar {
            let stripe =
                egui::Rect::from_min_size(rect.min, egui::vec2(SELECTION_BAR_WIDTH, rect.height()));
            ui.painter()
                .rect_filled(stripe, 1.5, visuals.selection.stroke.color);
        }
        let inset = if bar { SELECTION_BAR_WIDTH + 5.0 } else { 4.0 };
        let galley = egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.width() - inset - 4.0).max(1.0),
            egui::TextStyle::Button,
        );
        let pos = egui::pos2(rect.min.x + inset, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, ui.visuals().text_color());
    }
    response
}

/// Width of the accent bar on the left of a selected list row.
const SELECTION_BAR_WIDTH: f32 = 3.0;

/// Height of one tree row without the spacing between rows (virtualized list).
const TREE_ROW_HEIGHT: f32 = 20.0;

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
            // UI1: a painted triangle that turns smoothly (no «>» / «v» glyph).
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(DOT_LEAF, DOT_LEAF), egui::Sense::click());
            let label = tr(
                lang,
                if item.expanded {
                    Tr::TreeCollapseLabel
                } else {
                    Tr::TreeExpandLabel
                },
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
            let openness = ui.ctx().animate_bool(response.id, item.expanded);
            let color = if response.hovered() {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            paint_expander(ui.painter(), rect, openness, color);
            if response.clicked() {
                actions.push(TreeAction::Toggle(item.id));
            }
        } else {
            ui.add_space(DOT_LEAF);
        }

        let is_selected = selected == Some((session, item.id));
        let text = match item.node.kind {
            NodeKind::Branch => {
                egui::RichText::new(&item.node.name).color(egui::Color32::from_rgb(130, 180, 255))
            }
            NodeKind::Item => egui::RichText::new(&item.node.name),
        };
        // A full-width row is both clickable/queryable and a reliable drag
        // source: the whole row (not just the label glyphs) senses
        // click_and_drag, so a mouse grab anywhere on the row starts a drag.
        // This is what the headless RawInput DnD proof drives.
        let response = flat_row(
            ui,
            egui::vec2(ui.available_width(), TREE_ROW_HEIGHT),
            egui::Sense::click_and_drag(),
            is_selected,
            text,
            &item.node.name,
            false,
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
