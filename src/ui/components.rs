//! Small UI primitives: rendering only, with no application or backend access.

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
