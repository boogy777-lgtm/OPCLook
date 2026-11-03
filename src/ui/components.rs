//! Small UI primitives: rendering only, with no application or backend access.

/// Render supporting text with the current theme's subdued text style.
/// The caller owns the content and any behavior attached to the response.
pub(super) fn hint(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.label(egui::RichText::new(text).weak())
}

/// Render a checkbox without applying a keyboard-generated IME candidate.
/// `Response::changed` is egui's raw event; compare the value after this call.
pub(super) fn checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
    text: impl Into<String>,
) -> egui::Response {
    checkbox_enabled(ui, value, text, true)
}

/// Enabled variant for controls whose availability is owned by the caller.
pub(super) fn checkbox_enabled(
    ui: &mut egui::Ui,
    value: &mut bool,
    text: impl Into<String>,
    enabled: bool,
) -> egui::Response {
    let previous = *value;
    let response = ui.add_enabled(enabled, egui::Checkbox::new(value, text.into()));
    if super::ime_blocks_keys(ui.ctx())
        && response.changed()
        && !response.clicked_by(egui::PointerButton::Primary)
    {
        *value = previous;
    }
    response
}

/// Render a number control and reject arrow-key changes while the IME owns
/// candidate navigation. Normal editing and pointer input remain unchanged.
/// As with checkbox, `Response::changed` describes egui's raw event.
pub(super) fn numeric<T: Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    render: impl FnOnce(&mut egui::Ui, &mut T) -> egui::Response,
) -> egui::Response {
    let previous = *value;
    // Snapshot before the widget: egui's DragValue consumes arrow events.
    let candidate_key = super::ime_blocks_keys(ui.ctx())
        && ui.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::ArrowUp
                            | egui::Key::ArrowDown
                            | egui::Key::ArrowLeft
                            | egui::Key::ArrowRight,
                        pressed: true,
                        ..
                    }
                )
            })
        });
    let response = render(ui, value);
    if candidate_key {
        *value = previous;
    }
    response
}

/// Keep an IME candidate key from changing an embedded selection; pointer
/// selection remains deliberate. The enclosing popup still owns its lifetime.
/// `Response::changed` is egui's raw event; compare the value after this call.
pub(super) fn selectable_value<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    selected: T,
    text: impl Into<String>,
) -> egui::Response {
    let previous = *value;
    let response = ui.selectable_value(value, selected, text.into());
    if super::ime_blocks_keys(ui.ctx())
        && response.changed()
        && !response.clicked_by(egui::PointerButton::Primary)
    {
        *value = previous;
    }
    response
}

/// Render a button with an optional reason exposed on hover and to accessibility.
/// `enabled` remains independent of `reason`: some disabled states have no reason.
pub(super) fn button_with_reason(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    enabled: bool,
    reason: Option<&str>,
) -> egui::Response {
    let response = ui.add_enabled(enabled, button);
    if let Some(text) = reason {
        response.on_hover_text(text).on_disabled_hover_text(text)
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[derive(Default)]
    struct State {
        enabled: bool,
        clicks: usize,
        double_clicks: usize,
    }

    fn harness(enabled: bool) -> Harness<'static, State> {
        Harness::builder()
            .with_size([320.0, 120.0])
            .with_step_dt(0.01)
            .with_accessibility_check(false)
            .build_ui_state(
                |ui, state: &mut State| {
                    let reason = (!state.enabled).then_some("Unavailable for this test");
                    let response =
                        button_with_reason(ui, egui::Button::new("Action"), state.enabled, reason);
                    if response.clicked() {
                        state.clicks += 1;
                    }
                    if response.double_clicked() {
                        state.double_clicks += 1;
                    }
                },
                State {
                    enabled,
                    ..Default::default()
                },
            )
    }

    #[test]
    fn ime_candidate_does_not_change_choice_but_pointer_can() {
        let mut h = Harness::builder()
            .with_size([320.0, 120.0])
            .with_accessibility_check(false)
            .build_ui_state(
                |ui, value: &mut u8| {
                    super::super::update_ime_state(ui.ctx());
                    selectable_value(ui, value, 1, "First");
                    selectable_value(ui, value, 2, "Second");
                },
                1u8,
            );
        h.run_steps(2);
        h.get_by_label("Second").focus();
        h.run_steps(1);
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "a".into(),
            active_range_chars: None,
        }));
        h.key_press(egui::Key::Space);
        h.run_steps(2);
        assert_eq!(*h.state(), 1);
        h.get_by_label("Second").click();
        h.run_steps(2);
        assert_eq!(*h.state(), 2);
    }

    #[test]
    fn subdued_hint_retains_accessible_text() {
        let mut h = Harness::builder()
            .with_size([320.0, 120.0])
            .with_accessibility_check(false)
            .build_ui_state(
                |ui, _: &mut ()| {
                    hint(ui, "Supporting hint");
                },
                (),
            );
        h.run_steps(2);
        assert!(h.query_all_by_label("Supporting hint").next().is_some());
    }

    #[test]
    fn response_preserves_single_and_double_clicks() {
        let mut h = harness(true);
        h.run_steps(2);
        h.get_by_label("Action").click();
        h.run_steps(2);
        assert_eq!(h.state().clicks, 1);
        h.get_by_label("Action").click();
        h.run_steps(2);
        assert_eq!(h.state().clicks, 2);
        assert_eq!(h.state().double_clicks, 1);
    }

    #[test]
    fn disabled_button_exposes_reason_and_never_clicks() {
        let mut h = harness(false);
        h.run_steps(2);
        let button = h.get_by_label("Action");
        assert!(button.accesskit_node().is_disabled());
        assert_eq!(
            button.accesskit_node().description().as_deref(),
            Some("Unavailable for this test")
        );
        button.click();
        h.run_steps(2);
        assert_eq!(h.state().clicks, 0);
        assert_eq!(h.state().double_clicks, 0);
    }
}
