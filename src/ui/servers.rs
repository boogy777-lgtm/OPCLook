use super::colors::{green, red, yellow};
use super::truncate_text;
use crate::app::{App, SessionState};
use crate::bff::model::{Protocol, SessionId};
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

    let visible: Vec<(SessionId, String, SessionState, String)> = app
        .sessions
        .iter()
        .filter(|s| s.visible)
        .map(|s| {
            (
                s.id,
                s.display_name.clone(),
                s.state,
                s.display_name.clone(),
            )
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
                    for (id, label, state, full) in &visible {
                        let color = tab_state_color(*state);
                        let prefix = match app
                            .sessions
                            .iter()
                            .find(|s| s.id == *id)
                            .map(|s| s.protocol())
                        {
                            Some(Protocol::OpcDa) => "DA",
                            _ => "UA",
                        };
                        let short = truncate_text(label, TRUNCATE_TAB_LABEL);
                        let tab_label = format!("{prefix}: {short}");
                        let tooltip = format!("{}\n{}", full, state_text(*state, lang));
                        let selected = app.active_session == Some(*id);
                        ui.horizontal(|ui| {
                            ui.colored_label(color, "●");
                            let response = ui.selectable_label(selected, tab_label.as_str());
                            if response.clicked() {
                                switch = Some(*id);
                            }
                            response.context_menu(|ui| {
                                if ui.button(tr(lang, Tr::RemoveFromList)).clicked() {
                                    remove = Some(*id);
                                    ui.close();
                                }
                            });
                            if ui
                                .small_button("×")
                                .on_hover_text(tr(lang, Tr::CloseTabTooltip))
                                .clicked()
                            {
                                close = Some(*id);
                            }
                        })
                        .response
                        .on_hover_text(&tooltip);
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
    if visible.is_empty() {
        ui.label(egui::RichText::new(tr(lang, Tr::EmptyTabsHint)).weak());
    } else if visible.len() > 1 {
        // A8: the tabs pick the server whose tree is shown on the left; they
        // are NOT a filter of the table below.
        ui.label(egui::RichText::new(tr(lang, Tr::TabsNotAFilterHint)).weak());
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
        .map(|s| s.endpoint.display_name())
        .unwrap_or_else(|| "?".to_string());
    app.request_confirm(
        tr(app.lang(), Tr::RemoveFromListConfirmTitle).to_string(),
        fmt(app.lang(), Tr::RemoveFromListConfirmText, &[&name]),
        crate::app::ConfirmAction::RemoveSession(session),
    );
}
