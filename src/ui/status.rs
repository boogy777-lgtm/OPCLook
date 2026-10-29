use super::colors::{green, red, yellow};
use super::truncate_text;
use crate::app::{App, ReconnectState, SessionState};
use crate::bff::model::SubscriptionState;
use crate::constants::{RECONNECT_MAX_ATTEMPTS, STATUS_BAR_HEIGHT, TRUNCATE_STATUS_NAME};
use crate::i18n::{Lang, Tr, fmt, tr};

#[derive(Clone, Default)]
struct FooterFocus {
    frame: u64,
    ids: Vec<egui::Id>,
}

fn footer_focus_id() -> egui::Id {
    egui::Id::unique("opclook_footer_focus")
}

/// Register the real rendered widgets, so navigation cannot leave this region.
pub(super) fn register_footer_widget(response: &egui::Response) {
    response.ctx.data_mut(|data| {
        let mut state = data
            .get_temp::<FooterFocus>(footer_focus_id())
            .unwrap_or_default();
        if !state.ids.contains(&response.id) {
            state.ids.push(response.id);
        }
        data.insert_temp(footer_focus_id(), state);
    });
}

fn begin_footer_frame(ctx: &egui::Context, app: &App) {
    let frame = ctx.cumulative_frame_nr();
    let mut state = ctx.data(|data| {
        data.get_temp::<FooterFocus>(footer_focus_id())
            .unwrap_or_default()
    });
    if state.frame == frame {
        return;
    }
    let focused = ctx.memory(|memory| memory.focused());
    if super::regions::current(ctx) == Some(super::regions::Region::Footer)
        && !app.dialogs.any_open()
        && app.csv_import.is_none()
        && !super::ime_blocks_keys(ctx)
        && !ctx.text_edit_focused()
    {
        let previous = [egui::Key::ArrowLeft, egui::Key::ArrowUp]
            .into_iter()
            .any(|key| ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key)));
        let next = [egui::Key::ArrowRight, egui::Key::ArrowDown]
            .into_iter()
            .any(|key| ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key)));
        if previous || next {
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            let target = focused
                .and_then(|id| state.ids.iter().position(|item| *item == id))
                .and_then(|index| {
                    if previous {
                        index.checked_sub(1)
                    } else {
                        Some((index + 1).min(state.ids.len().saturating_sub(1)))
                    }
                })
                .and_then(|index| state.ids.get(index))
                .copied()
                .or(focused.filter(|id| state.ids.contains(id)));
            if let Some(id) = target {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
            // A move and Enter in one input batch cannot activate an old focus.
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter | egui::Key::Space,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
        }
    }
    state.frame = frame;
    state.ids.clear();
    ctx.data_mut(|data| data.insert_temp(footer_focus_id(), state));
}

pub(super) fn status_bar(app: &mut App, ui: &mut egui::Ui) {
    begin_footer_frame(ui.ctx(), app);
    let lang = app.lang();
    let mut focus_entry =
        if !app.settings.view.options.show_script && !app.settings.view.options.show_log {
            super::regions::take_entry(ui.ctx(), super::regions::Region::Footer)
        } else {
            false
        };
    let panel = egui::Panel::bottom("status_bar")
        .exact_size(STATUS_BAR_HEIGHT)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Shorten left-to-right labels so a narrow window never lets the
                // right-hand status section collide with the left content.
                let active = app
                    .active_session
                    .and_then(|id| app.sessions.iter().find(|s| s.id == id));
                if let Some(session) = active {
                    let (color, text) = match session.state {
                        SessionState::Connected => (green(), tr(lang, Tr::Connected)),
                        SessionState::Connecting => (yellow(), tr(lang, Tr::Connecting)),
                        SessionState::Disconnected => (red(), tr(lang, Tr::NoLink)),
                    };
                    ui.colored_label(color, "●");
                    let truncated =
                        truncate_text(&session.endpoint().display_name(), TRUNCATE_STATUS_NAME);
                    let indicator = egui::Button::new(
                        egui::RichText::new(format!("{truncated}: {text}")).color(color),
                    )
                    .frame(false);
                    let indicator = ui
                        .add(indicator)
                        .on_hover_text(session.endpoint().display_name());
                    register_footer_widget(&indicator);
                    if focus_entry {
                        indicator.request_focus();
                        focus_entry = false;
                    }
                    if indicator.clicked() {
                        app.status_popup = !app.status_popup;
                        ui.ctx().request_repaint();
                    }
                    if let Some(hint) = app.reconnect_hint() {
                        ui.separator();
                        ui.colored_label(yellow(), &hint);
                    }
                } else {
                    ui.colored_label(red(), "●");
                    let label = ui.add(
                        egui::Label::new(tr(lang, Tr::NoActiveServer))
                            .sense(egui::Sense::focusable_noninteractive()),
                    );
                    register_footer_widget(&label);
                    if focus_entry {
                        label.request_focus();
                        focus_entry = false;
                    }
                }
                ui.separator();
                let hint = if app.tree_focused {
                    Some(fmt(
                        lang,
                        Tr::TreeHint,
                        &[
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::NextSession,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::TreeEnter,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::Refresh,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::SwitchPane,
                            ),
                        ],
                    ))
                } else if app.table_focus.is_some() {
                    Some(fmt(
                        lang,
                        Tr::RowsHint,
                        &[
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::SelectAll,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::Copy,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::NextSession,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::Delete,
                            ),
                            &super::shortcuts::binding_texts(
                                ui.ctx(),
                                super::keymap::KeyAction::TableWrite,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::Refresh,
                            ),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::SwitchPane,
                            ),
                        ],
                    ))
                } else {
                    Some(fmt(
                        lang,
                        Tr::KeyboardIdleHint,
                        &[
                            &super::shortcuts::connect_shortcut_text(ui.ctx()),
                            &super::shortcuts::binding_text(
                                ui.ctx(),
                                super::keymap::KeyAction::SwitchPane,
                            ),
                        ],
                    ))
                };
                let counts = egui::Label::new(fmt(
                    lang,
                    Tr::SignalsCount,
                    &[&app.visible_row_count(), &app.selected_rows.len()],
                ));
                if let Some(hint) = hint {
                    ui.add(counts).on_hover_text(hint);
                } else {
                    ui.add(counts);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if app.write_batch.pending > 0 {
                        ui.spinner();
                        ui.label(tr(lang, Tr::Writing));
                        ui.separator();
                    } else if let Some(summary) = &app.write_batch.summary {
                        ui.label(egui::RichText::new(summary).color(green()));
                        ui.separator();
                    }
                    if let Some(t) = app.last_update {
                        ui.label(
                            egui::RichText::new(fmt(
                                lang,
                                Tr::UpdatedAt,
                                &[&t.with_timezone(&chrono::Local)
                                    .format("%H:%M:%S")
                                    .to_string()],
                            ))
                            .weak(),
                        );
                    }
                    ui.label(&app.status);
                });
            });
        });
    if !app.dialogs.any_open()
        && app.csv_import.is_none()
        && ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| panel.response.rect.contains(pos))
        })
    {
        super::regions::pointer_enter(ui.ctx(), super::regions::Region::Footer);
    }
}
/// Non-blocking popup with details about the active server (opened from the
/// status-bar indicator).
/// Localized label for the UA push-subscription state (server info popup).
pub(super) fn subscription_state_text(lang: Lang, state: SubscriptionState) -> &'static str {
    match state {
        SubscriptionState::Active => tr(lang, Tr::SubscriptionActive),
        SubscriptionState::Inactive => tr(lang, Tr::SubscriptionInactive),
        SubscriptionState::Failed => tr(lang, Tr::SubscriptionFailed),
    }
}

pub(super) fn status_popup(app: &mut App, ui: &mut egui::Ui) {
    if !app.status_popup {
        return;
    }
    let lang = app.lang();
    let Some(session_id) = app.active_session else {
        app.status_popup = false;
        return;
    };
    let session = app.sessions.iter().find(|s| s.id == session_id);
    let Some(session) = session else {
        app.status_popup = false;
        return;
    };
    let subscribed = app
        .rows()
        .iter()
        .filter(|r| r.session == session_id)
        .count();
    let selected = app.selected_rows.len();
    let last_update = app.last_update;
    let protocol = session.protocol();
    let state_text = match session.state {
        SessionState::Connected => tr(lang, Tr::Connected),
        SessionState::Connecting => tr(lang, Tr::Connecting),
        SessionState::Disconnected => tr(lang, Tr::NoLink),
    };
    let (reconnect_line, attempts, cap) = {
        let auto = app.settings.view.options.auto_reconnect;
        let cap = RECONNECT_MAX_ATTEMPTS;
        let attempts = if matches!(session.reconnect, ReconnectState::Exhausted) {
            cap
        } else {
            session.reconnect.attempt()
        };
        let line = match session.reconnect {
            ReconnectState::Exhausted => Some(tr(lang, Tr::AttemptsExhausted).to_string()),
            _ if session.state != SessionState::Disconnected || !auto => None,
            ReconnectState::Stopped => Some(fmt(
                lang,
                Tr::AutoReconnectStopped,
                &[&attempts.to_string()],
            )),
            ReconnectState::Waiting { at, .. } => {
                let secs = at
                    .saturating_duration_since(std::time::Instant::now())
                    .as_secs()
                    .min(3600);
                Some(fmt(
                    lang,
                    Tr::ReconnectingLine,
                    &[&attempts.to_string(), &secs.to_string(), &cap.to_string()],
                ))
            }
            ReconnectState::Idle => {
                Some(fmt(lang, Tr::ReconnectingAttempt, &[&attempts.to_string()]))
            }
        };
        (line, attempts, cap)
    };
    let mut close = false;
    egui::Window::new(tr(lang, Tr::Server))
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::LEFT_BOTTOM, [8.0, -28.0])
        .show(ui.ctx(), |ui| {
            egui::Grid::new("status_popup_grid")
                .num_columns(2)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(tr(lang, Tr::ProtocolLabel)).strong());
                    ui.label(protocol.to_string());
                    ui.end_row();
                    ui.label(egui::RichText::new(tr(lang, Tr::EndpointLabel)).strong());
                    ui.label(&session.endpoint().address);
                    ui.end_row();
                    if let Some(ua) = session.ua() {
                        ui.label(egui::RichText::new(tr(lang, Tr::SessionTimeoutLabel)).strong());
                        ui.label(ua.ua_session_timeout_ms.to_string());
                        ui.end_row();
                    }
                    ui.label(egui::RichText::new(tr(lang, Tr::StateLabel)).strong());
                    let state_color = match session.state {
                        SessionState::Connected => green(),
                        SessionState::Connecting => yellow(),
                        SessionState::Disconnected => red(),
                    };
                    ui.colored_label(state_color, state_text);
                    if !session.info.is_empty() {
                        ui.end_row();
                        ui.label(egui::RichText::new(tr(lang, Tr::DetailsLabel)).strong());
                        ui.label(&session.info);
                    }
                    ui.end_row();
                    ui.label(egui::RichText::new(tr(lang, Tr::SubscribedLabel)).strong());
                    ui.label(subscribed.to_string());
                    ui.end_row();
                    if let Some(ua) = session.ua() {
                        ui.label(egui::RichText::new(tr(lang, Tr::SubscriptionLabel)).strong());
                        ui.label(subscription_state_text(lang, ua.subscription));
                        ui.end_row();
                    }
                    ui.label(egui::RichText::new(tr(lang, Tr::SelectedLabel)).strong());
                    ui.label(selected.to_string());
                    ui.end_row();
                    if let Some(t) = last_update {
                        ui.label(egui::RichText::new(tr(lang, Tr::LastUpdateLabel)).strong());
                        ui.label(
                            t.with_timezone(&chrono::Local)
                                .format("%H:%M:%S")
                                .to_string(),
                        );
                        ui.end_row();
                    }
                    if let Some(line) = reconnect_line {
                        ui.label(egui::RichText::new(tr(lang, Tr::ReconnectLabel)).strong());
                        ui.colored_label(yellow(), line);
                        ui.end_row();
                    } else if cap > 0 && attempts > 0 {
                        ui.label(egui::RichText::new(tr(lang, Tr::RetriesLabel)).strong());
                        ui.label(fmt(
                            lang,
                            Tr::RetriesLine,
                            &[&attempts.to_string(), &cap.to_string()],
                        ));
                        ui.end_row();
                    }
                });
            ui.separator();
            if ui.button(tr(lang, Tr::Close)).clicked() {
                close = true;
            }
        });
    if close {
        app.status_popup = false;
    }
}
