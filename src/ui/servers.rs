use super::colors::{green, red, yellow};
use super::dialogs::{security_mode_label, security_policy_label};
use super::paint::{paint_lock_icon, painter_x_button};
use super::truncate_text;
use crate::app::{App, SessionState};
use crate::bff::model::{Protocol, SessionId, UaSecurityMode, UaSecurityPolicy};
use crate::constants::TRUNCATE_TAB_LABEL;
use crate::i18n::{Lang, Tr, fmt, tr};

/// The session tab strip is rendered inside the central tags (table) panel,
/// not as a window-wide top panel. This helper lets tests assert that the old
/// `"session_tabs"` top panel is no longer mounted.
pub const fn top_tab_panel_enabled() -> bool {
    false
}
/// Per-session tab strip. Rendered as a slim row at the top of the central
/// (tags/table) panel so tabs belong to the tags area, not the app frame above
/// the tree/menu. Closing a tab hides the view but keeps the container/tags; a
/// Connected container keeps running.
pub(super) fn session_tab_strip(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    let mut switch = None;
    let mut close = None;
    let mut remove = None;
    let mut new_connection = false;

    // (id, name, state, tooltip head, security suffix, protocol prefix)
    type TabRow = (
        SessionId,
        String,
        SessionState,
        String,
        Option<String>,
        &'static str,
    );
    let visible: Vec<TabRow> = app
        .sessions
        .iter()
        .filter(|s| s.visible)
        .map(|s| {
            let prefix = match s.protocol() {
                Protocol::OpcDa => "DA",
                _ => "UA",
            };
            // V2: endpoints of one server differ by security only, so an
            // explicit choice is part of the tab (a padlock), of its tooltip
            // and of its accessible name.
            match security_suffix(s.endpoint(), lang) {
                Some((short, long)) => (
                    s.id,
                    s.display_name.clone(),
                    s.state,
                    format!("{}\n{long}", s.display_name),
                    Some(short),
                    prefix,
                ),
                None => (
                    s.id,
                    s.display_name.clone(),
                    s.state,
                    s.display_name.clone(),
                    None,
                    prefix,
                ),
            }
        })
        .collect();

    ui.horizontal(|ui| {
        // Many tabs: scroll horizontally inside the tags header without ever
        // breaking the table layout below.
        egui::ScrollArea::horizontal()
            .id_salt("session_tabs_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (id, label, state, full, security, prefix) in &visible {
                        let short = truncate_text(label, TRUNCATE_TAB_LABEL);
                        let tab_label = format!("{prefix}: {short}");
                        let access_label = match security {
                            Some(security) => format!("{tab_label} · {security}"),
                            None => tab_label.clone(),
                        };
                        let tooltip = format!("{}\n{}", full, state_text(*state, lang));
                        let selected = app.active_session == Some(*id);
                        let (response, closed) = session_tab(
                            ui,
                            &TabContent {
                                selected,
                                dot: tab_state_color(*state),
                                text: &tab_label,
                                access_label: &access_label,
                                locked: security.is_some(),
                                close_tooltip: tr(lang, Tr::CloseTabTooltip),
                            },
                        );
                        let response = response.on_hover_text(&tooltip);
                        if closed {
                            close = Some(*id);
                        } else if response.clicked() {
                            switch = Some(*id);
                        }
                        response.context_menu(|ui| {
                            if ui.button(tr(lang, Tr::RemoveFromList)).clicked() {
                                remove = Some(*id);
                                ui.close();
                            }
                        });
                    }
                    // «+» at the END of the tab strip opens the connect dialog
                    // (a new connection). Unlike a tab's «×», this never hides
                    // or removes anything. It scrolls with the tabs so it is
                    // always reachable.
                    if ui
                        .small_button("+")
                        .on_hover_text(tr(lang, Tr::NewConnection))
                        .clicked()
                    {
                        new_connection = true;
                    }
                });
            });
    });

    // Empty tab state: only a localized hint text keeps the area from looking
    // like something is missing. The «Подключиться» button lives in the left
    // nav and the menu, not here.
    // V1: each tab owns its tag list (the table below shows the active tab's
    // tags), so no "tabs are not a filter" hint any more.
    if visible.is_empty() {
        ui.label(egui::RichText::new(tr(lang, Tr::EmptyTabsHint)).weak());
    }

    if let Some(id) = switch {
        app.active_session = Some(id);
        app.selected_tree = None;
    }
    if let Some(id) = close {
        app.close_tab(id);
    }
    if let Some(id) = remove {
        request_remove_session(app, id);
    }
    if new_connection {
        app.open_connect_dialog();
    }
}

/// What one session tab shows.
struct TabContent<'a> {
    selected: bool,
    /// State dot colour.
    dot: egui::Color32,
    /// Visible text («DA: name»).
    text: &'a str,
    /// Accessible name: the visible text plus the security, if any.
    access_label: &'a str,
    /// Draw the padlock (an explicitly chosen UA security).
    locked: bool,
    /// Tooltip and accessible name of the close button.
    close_tooltip: &'a str,
}

const TAB_HEIGHT: f32 = 26.0;
const TAB_ACCENT_HEIGHT: f32 = 2.0;
const TAB_PAD_X: f32 = 8.0;
const TAB_GAP: f32 = 6.0;
const TAB_DOT: f32 = 8.0;
const TAB_LOCK: f32 = 12.0;
const TAB_CLOSE: f32 = 18.0;

/// UI3: one session tab, painted. The active tab sits on a slightly lighter
/// background with an accent bar along its bottom edge; an inactive one has no
/// background until the pointer is over it. The state dot and the padlock are
/// painted (no font glyph), and «×» shows only on the active tab or while the
/// pointer is over the tab (its place is always reserved, so the tabs do not
/// jump). Returns the tab's response and whether «×» was clicked.
fn session_tab(ui: &mut egui::Ui, tab: &TabContent<'_>) -> (egui::Response, bool) {
    let visuals = ui.visuals();
    let text_color = if tab.selected {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    let galley = ui.painter().layout_no_wrap(
        tab.text.to_string(),
        egui::TextStyle::Button.resolve(ui.style()),
        text_color,
    );
    let lock_width = if tab.locked { TAB_GAP + TAB_LOCK } else { 0.0 };
    let width = TAB_PAD_X
        + TAB_DOT
        + TAB_GAP
        + galley.size().x
        + lock_width
        + TAB_GAP
        + TAB_CLOSE
        + TAB_PAD_X * 0.5;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, TAB_HEIGHT), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, tab.selected, tab.access_label)
    });
    let hovered = ui.rect_contains_pointer(rect);

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if tab.selected {
            visuals
                .panel_fill
                .lerp_to_gamma(visuals.strong_text_color(), 0.13)
        } else if hovered {
            visuals
                .panel_fill
                .lerp_to_gamma(visuals.strong_text_color(), 0.05)
        } else {
            egui::Color32::TRANSPARENT
        };
        let top_round = egui::CornerRadius {
            nw: 5,
            ne: 5,
            sw: 0,
            se: 0,
        };
        if fill != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, top_round, fill);
        }
        if tab.selected {
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.min.x, rect.max.y - TAB_ACCENT_HEIGHT),
                egui::vec2(rect.width(), TAB_ACCENT_HEIGHT),
            );
            ui.painter()
                .rect_filled(bar, 0.0, visuals.selection.stroke.color);
        }
        let mut x = rect.min.x + TAB_PAD_X;
        let cy = rect.center().y - TAB_ACCENT_HEIGHT * 0.5;
        ui.painter()
            .circle_filled(egui::pos2(x + TAB_DOT * 0.5, cy), TAB_DOT * 0.5, tab.dot);
        x += TAB_DOT + TAB_GAP;
        let text_pos = egui::pos2(x, cy - galley.size().y * 0.5);
        x += galley.size().x;
        ui.painter().galley(text_pos, galley, text_color);
        if tab.locked {
            x += TAB_GAP;
            let lock_rect = egui::Rect::from_center_size(
                egui::pos2(x + TAB_LOCK * 0.5, cy),
                egui::vec2(TAB_LOCK, TAB_LOCK),
            );
            paint_lock_icon(ui.painter(), lock_rect, text_color);
        }
    }

    let mut closed = false;
    if tab.selected || hovered {
        let close_rect = egui::Rect::from_center_size(
            egui::pos2(
                rect.max.x - TAB_PAD_X * 0.5 - TAB_CLOSE * 0.5,
                rect.center().y - TAB_ACCENT_HEIGHT * 0.5,
            ),
            egui::vec2(TAB_CLOSE, TAB_CLOSE),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(close_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        closed = painter_x_button(&mut child, TAB_CLOSE, tab.close_tooltip);
    }
    (response, closed)
}

/// Short and full security text of an explicitly chosen UA endpoint
/// (`None` for DA and for Auto/Auto).
pub(super) fn security_suffix(
    endpoint: &crate::bff::model::EndpointSpec,
    lang: Lang,
) -> Option<(String, String)> {
    if endpoint.protocol != Protocol::OpcUa {
        return None;
    }
    let policy = endpoint.ua_security_policy;
    let mode = endpoint.ua_security_mode;
    if policy == UaSecurityPolicy::Auto && mode == UaSecurityMode::Auto {
        return None;
    }
    let policy_text = security_policy_label(lang, policy);
    let mode_text = security_mode_label(lang, mode);
    Some((
        mode_text.to_string(),
        format!("{policy_text} · {mode_text}"),
    ))
}

pub(super) fn tab_state_color(state: SessionState) -> egui::Color32 {
    match state {
        SessionState::Connected => green(),
        SessionState::Connecting => yellow(),
        SessionState::Disconnected => red(),
    }
}

/// State dot color for the left-panel servers table: green connected, yellow
/// connecting/reconnecting, gray disconnected.
pub(super) fn server_state_color(state: SessionState) -> egui::Color32 {
    match state {
        SessionState::Connected => green(),
        SessionState::Connecting => yellow(),
        SessionState::Disconnected => egui::Color32::GRAY,
    }
}

pub(super) fn state_text(state: SessionState, lang: Lang) -> &'static str {
    match state {
        SessionState::Connected => tr(lang, Tr::Connected),
        SessionState::Connecting => tr(lang, Tr::Connecting),
        SessionState::Disconnected => tr(lang, Tr::NoLink),
    }
}

/// Confirm then fully drop a session container («Удалить из списка»).
pub(super) fn request_remove_session(app: &mut App, session: SessionId) {
    let name = app
        .sessions
        .iter()
        .find(|s| s.id == session)
        .map(|s| s.endpoint().display_name())
        .unwrap_or_else(|| "?".to_string());
    app.request_confirm(
        tr(app.lang(), Tr::RemoveFromListConfirmTitle).to_string(),
        fmt(app.lang(), Tr::RemoveFromListConfirmText, &[&name]),
        crate::app::ConfirmAction::RemoveSession(session),
    );
}
