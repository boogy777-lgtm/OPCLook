mod tree_rows;
use tree_rows::draw_tree_item;

use super::actions::{TreeAction, TreeDragPayload};
use super::colors::yellow;
use super::files::save_tree_csv;
use super::paint::{
    paint_children_icon, paint_expander, paint_name_icon, paint_pause_icon, paint_play_icon,
    paint_plus_icon, paint_reconnect_icon, paint_refresh_icon, painter_icon_button,
    painter_icon_button_tip, painter_x_button,
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
    LEFT_PANEL_DEFAULT_WIDTH, RECONNECT_MAX_ATTEMPTS, TRUNCATE_SERVER_ROW,
};
use crate::i18n::{Lang, Tr, fmt, tr};

fn server_cursor_id() -> egui::Id {
    egui::Id::unique("opclook_server_region_cursor")
}

pub(super) fn enter_keyboard_region(app: &mut App, ctx: &egui::Context) {
    app.tree_focused = false;
    app.table_focus = None;
    let cursor = app
        .active_session
        .or_else(|| app.sessions.first().map(|session| session.id));
    ctx.data_mut(|data| data.insert_temp(server_cursor_id(), cursor));
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::unique("opclook_servers_region")));
    ctx.request_repaint();
}

fn server_cursor(ctx: &egui::Context) -> Option<SessionId> {
    ctx.data(|data| {
        data.get_temp::<Option<SessionId>>(server_cursor_id())
            .flatten()
    })
}

pub(super) fn handle_server_region_keys(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx();
    if crate::ui::ime_blocks_keys(ctx) || !ctx.input(|input| input.modifiers.is_none()) {
        return;
    }
    let ids: Vec<_> = app.sessions.iter().map(|session| session.id).collect();
    let cursor = server_cursor(ctx)
        .and_then(|id| ids.iter().position(|value| *value == id))
        .unwrap_or(0);
    let down = ctx.input(|input| input.key_pressed(egui::Key::ArrowDown));
    let up = ctx.input(|input| input.key_pressed(egui::Key::ArrowUp));
    if !ids.is_empty() && (down || up) {
        let next = if down {
            (cursor + 1).min(ids.len() - 1)
        } else {
            cursor.saturating_sub(1)
        };
        ctx.data_mut(|data| data.insert_temp(server_cursor_id(), ids.get(next).copied()));
        ctx.request_repaint();
    } else if crate::ui::key_pressed_once(ctx, egui::Key::Enter) {
        if let Some(id) = ids.get(cursor).copied() {
            app.activate_container(id);
        } else {
            let _ = app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
        }
    } else if crate::ui::key_pressed_once(ctx, egui::Key::ArrowRight)
        && let Some(id) = ids.get(cursor).copied()
    {
        app.activate_container(id);
        app.focus_tree();
    }
}

pub(super) fn navigation(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    // Opaque frame so the left panel paints its full allocated rect (no
    // transparent seam next to the central table while resizing).
    let side_frame = egui::Frame::side_top_panel(ui.style()).fill(ui.style().visuals.panel_fill);
    egui::Panel::left("navigation")
        .resizable(true)
        .default_size(LEFT_PANEL_DEFAULT_WIDTH)
        .frame(side_frame)
        .show(ui, |ui| {
            let region = super::regions::Region::Servers;
            let entry = super::regions::take_entry(ui.ctx(), region);
            let region_response = ui.interact(
                ui.max_rect(),
                egui::Id::unique("opclook_servers_region"),
                egui::Sense::focusable_noninteractive(),
            );
            if super::regions::current(ui.ctx()) == Some(region) {
                region_response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Label, true, tr(lang, Tr::Servers))
                });
            }
            if entry {
                region_response.request_focus();
            }
            if !app.dialogs.any_open()
                && app.csv_import.is_none()
                && ui.input(|input| {
                    input.pointer.primary_pressed()
                        && input
                            .pointer
                            .interact_pos()
                            .is_some_and(|pos| region_response.rect.contains(pos))
                })
            {
                super::regions::pointer_enter(ui.ctx(), region);
            }
            // Toolbar: connect / disconnect / reconnect with correct enabled
            let active_state = app
                .active_session
                .and_then(|id| app.sessions.iter().find(|s| s.id == id))
                .map(|s| s.state);
            // The modal/import guard also applies to the reconnect shortcut.
            let (can_disconnect, can_reconnect) = match active_state {
                None => (false, false),
                Some(SessionState::Connecting | SessionState::Connected) => (true, true),
                Some(SessionState::Disconnected) => (false, true),
            };
            ui.horizontal(|ui| {
                if super::actions::action_button(
                    app,
                    ui,
                    crate::app::actions::UiAction::OpenConnectDialog,
                    egui::Button::new(tr(lang, Tr::Connect)),
                )
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
            let add_width = ui.available_width();
            if super::actions::action_button(
                app,
                ui,
                crate::app::actions::UiAction::OpenConnectDialog,
                egui::Button::new(tr(lang, Tr::AddServer)).min_size(egui::vec2(add_width, 0.0)),
            )
            .clicked()
            {
                let _ = app.execute_ui_action(crate::app::actions::UiAction::OpenConnectDialog);
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
                    let name = match security_suffix(s.endpoint(), lang) {
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
                    let auto = app.settings.view.options.auto_reconnect;
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

                    // UI6: the selected server is a soft fill with an accent bar on
                    // the left (painted under the row), not a solid button.
                    let row_width = ui.available_width();
                    let row_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(row_width, SERVER_ROW_HEIGHT),
                    );
                    paint_row_band(ui, row_rect, selected, true);
                    if super::regions::current(ui.ctx()) == Some(super::regions::Region::Servers)
                        && !app.tree_focused
                        && server_cursor(ui.ctx()) == Some(*id)
                    {
                        ui.painter().rect_stroke(
                            row_rect.shrink(1.0),
                            2.0,
                            ui.visuals().selection.stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                    let awaitable = ui
                        .allocate_ui_with_layout(
                            egui::vec2(row_width, SERVER_ROW_HEIGHT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.add_space(SELECTION_BAR_WIDTH + 3.0);
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

                                // Name: flexible single-line, left-aligned, truncated, with
                                // tooltip. Capped so a wide label cannot feed back into the
                                // left panel's auto-size (which would widen the panel
                                // by ~28px every frame); truncation clips it anyway. The
                                // «×» at the row end keeps its own room.
                                let name_width = (ui.available_width()
                                    - ICON_SERVER_REMOVE
                                    - ui.spacing().item_spacing.x)
                                    .clamp(84.0, 300.0);
                                let short = truncate_text(name, TRUNCATE_SERVER_ROW);
                                let mut text = egui::RichText::new(short.as_str());
                                if selected {
                                    text = text.color(ui.visuals().strong_text_color());
                                }
                                let response = flat_row(
                                    ui,
                                    egui::vec2(name_width, SERVER_ROW_HEIGHT),
                                    egui::Sense::click(),
                                    selected,
                                    text,
                                    &short,
                                    RowBand::None,
                                );
                                response.clone().on_hover_ui(|ui| {
                                    ui.label(format!(
                                        "{}\n{} | {} | {}",
                                        name,
                                        if *is_da { "OPC DA" } else { "OPC UA" },
                                        info,
                                        state_text(*state, lang)
                                    ));
                                });
                                if response.clicked() || response.double_clicked() {
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
                            },
                        )
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
                app.tree_focused = false;
                ui.ctx()
                    .data_mut(|data| data.insert_temp(server_cursor_id(), Some(id)));
                super::regions::pointer_enter(ui.ctx(), super::regions::Region::Servers);
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
                // UI10: no session at all - only the hint here: «Подключиться»
                // is the button above and the one in the empty table (one action,
                // not two identical buttons side by side).
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new(tr(lang, Tr::ConnectPrompt)).weak());
                });
                return;
            };
            let Some(view) = app.sessions.iter().find(|s| s.id == session_id) else {
                return;
            };
            let protocol = view.protocol();
            let root_loading = view.root_loading;
            let connected = view.state == SessionState::Connected;

            // UI6: the four tree actions are one compact row of painted icons;
            // the accessible name of each is the former button text.
            let mut vars_only_changed: Option<bool> = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if painter_icon_button_tip(
                    ui,
                    ICON_TOOLBAR,
                    connected && !root_loading,
                    tr(lang, Tr::Refresh),
                    &fmt(
                        lang,
                        Tr::HintRefreshF5,
                        &[&super::shortcuts::binding_text(
                            ui.ctx(),
                            super::keymap::KeyAction::Refresh,
                        )],
                    ),
                    paint_refresh_icon,
                ) {
                    refresh_selected(app);
                }
                if painter_icon_button_tip(
                    ui,
                    ICON_TOOLBAR,
                    connected,
                    tr(lang, Tr::Add),
                    tr(lang, Tr::AddNodeTooltip),
                    paint_plus_icon,
                ) {
                    app.add_selected_tree();
                }
                if painter_icon_button_tip(
                    ui,
                    ICON_TOOLBAR,
                    connected,
                    tr(lang, Tr::AddChildren),
                    tr(lang, Tr::HintAddChildren),
                    paint_children_icon,
                ) {
                    app.add_selected_children();
                }
                if painter_icon_button_tip(
                    ui,
                    ICON_TOOLBAR,
                    connected,
                    tr(lang, Tr::AddByNameShort),
                    tr(lang, Tr::AddByNameTooltip),
                    paint_name_icon,
                ) {
                    open_add_signal(app);
                }
            });
            if protocol == Protocol::OpcUa {
                let mut only = app
                    .sessions
                    .iter()
                    .find(|s| s.id == session_id)
                    .is_some_and(|s| {
                        s.ua().is_some_and(|ua| {
                            ua.ua_browse_filter == crate::bff::model::NodeClassFilter::Variables
                        })
                    });
                if ui.checkbox(&mut only, tr(lang, Tr::OnlyVars)).changed() {
                    // U5: the filter applies to what is loaded, so the
                    // tree is re-read.
                    vars_only_changed = Some(only);
                }
            }
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
            let mut refresh_root = false;
            {
                let Some(view) = app.sessions.iter().find(|s| s.id == session_id) else {
                    return;
                };
                if view.tree().is_empty() {
                    if root_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr(lang, Tr::LoadingTree));
                        });
                    } else if connected {
                        // UI10: the hint and the same action as F5. An empty tree
                        // has no node to carry the tree keyboard focus, so the
                        // tree's own F5 handler never runs here; the key is
                        // taken here instead, unless the table, a text field or a
                        // dialog owns the keyboard (the table has its own F5).
                        let f5 = super::keymap::key(super::keymap::KeyAction::Refresh)
                            .is_some_and(|key| super::key_pressed_once(ui.ctx(), key))
                            && super::regions::current(ui.ctx())
                                == Some(super::regions::Region::Servers)
                            && app.table_focus.is_none()
                            && !app.dialogs.any_open()
                            && app.csv_import.is_none()
                            && !super::ime_blocks_keys(ui.ctx())
                            && !ui.ctx().text_edit_focused();
                        if empty_state(
                            ui,
                            tr(lang, Tr::TreeNotLoaded),
                            &fmt(
                                lang,
                                Tr::RefreshF5Button,
                                &[&super::shortcuts::binding_text(
                                    ui.ctx(),
                                    super::keymap::KeyAction::Refresh,
                                )],
                            ),
                        ) || f5
                        {
                            refresh_root = true;
                        }
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
                                if let Some(item) = view.tree().get(*index) {
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
            if refresh_root {
                refresh_selected(app);
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

/// UI10: a quiet empty state - the hint, then one button with the action (and
/// its shortcut). Returns true when the button was clicked.
fn empty_state(ui: &mut egui::Ui, hint: &str, action: &str) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new(hint).weak());
        ui.add_space(8.0);
        clicked = ui.button(action).clicked();
    });
    clicked
}

/// Who paints the selection / hover band of a `flat_row`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowBand {
    /// The caller painted it (the servers list paints one band for the whole
    /// row, under the dot and the buttons too).
    None,
    /// Soft fill under the row (the tag tree).
    Fill,
}

/// The soft band of a list row: a fill when selected (plus an accent bar on
/// the left edge when `bar`), a fainter one under the pointer.
fn paint_row_band(ui: &egui::Ui, rect: egui::Rect, selected: bool, bar: bool) {
    let visuals = ui.visuals();
    let fill = if selected {
        visuals.selection.bg_fill.gamma_multiply(0.45)
    } else if ui.rect_contains_pointer(rect) {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        return;
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    if selected && bar {
        let stripe =
            egui::Rect::from_min_size(rect.min, egui::vec2(SELECTION_BAR_WIDTH, rect.height()));
        ui.painter()
            .rect_filled(stripe, 1.5, visuals.selection.stroke.color);
    }
}

/// A single-line, left-aligned list row: the text truncated to the row, a band
/// under it when `band` says so. `label` is the accessible name. Used by the
/// tag tree and the servers list instead of a centred selectable button.
fn flat_row(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    sense: egui::Sense,
    selected: bool,
    text: egui::RichText,
    label: &str,
    band: RowBand,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    if ui.is_rect_visible(rect) {
        if band == RowBand::Fill {
            paint_row_band(ui, rect, selected, false);
        }
        let inset = 4.0;
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

/// Height of one server row in the left panel (a single line, <= 28 px).
const SERVER_ROW_HEIGHT: f32 = 26.0;

/// Height of one tree row without the spacing between rows (virtualized list).
const TREE_ROW_HEIGHT: f32 = 20.0;

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
