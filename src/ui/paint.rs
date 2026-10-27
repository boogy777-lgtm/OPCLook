/// UI1: the expand / collapse triangle shared by the tag tree and the signal
/// table, drawn with the painter (no font glyph). `openness` runs from 0.0
/// (closed, pointing right) to 1.0 (open, pointing down); pass the value of
/// `ctx.animate_bool(id, expanded)` for a smooth turn.
#[allow(dead_code)]
pub(super) fn paint_expander(
    painter: &egui::Painter,
    rect: egui::Rect,
    openness: f32,
    color: egui::Color32,
) {
    let openness = openness.clamp(0.0, 1.0);
    let center = rect.center();
    let r = rect.width().min(rect.height()) * 0.3;
    // Right-pointing triangle, rotated by 90° * openness around the center.
    let angle = openness * std::f32::consts::FRAC_PI_2;
    let rot = egui::emath::Rot2::from_angle(angle);
    let points = [
        egui::vec2(-r * 0.6, -r),
        egui::vec2(r, 0.0),
        egui::vec2(-r * 0.6, r),
    ]
    .map(|v| center + rot * v)
    .to_vec();
    painter.add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}

/// Small clickable square painted with `paint`. No rendered text, so no tofu
/// glyphs; the `name` string becomes the accessible node label and the tooltip.
/// Returns true when clicked (only while `enabled`).
pub(super) fn painter_icon_button(
    ui: &mut egui::Ui,
    size: f32,
    enabled: bool,
    name: &str,
    paint: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let response = response.on_hover_text(name);
    // Report the true enabled state to the accessibility tree (the tests assert
    // `is_disabled` on it). Hover/focus is captured beforehand so the closure
    // does not re-enter the Context write lock inside the accesskit builder.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    if enabled {
        let bg = if response.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 4.0, bg);
    }
    let color = if enabled {
        if response.hovered() {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        }
    } else {
        ui.visuals().weak_text_color()
    };
    paint(ui.painter(), rect.shrink(4.0), color);
    enabled && response.clicked()
}

/// A «×» close-style button drawn with the painter (no font dependency).
pub(super) fn painter_x_button(ui: &mut egui::Ui, size: f32, tooltip: &str) -> bool {
    painter_icon_button(ui, size, true, tooltip, |p, r, color| {
        let inset = r.width() * 0.22;
        let stroke = egui::Stroke::new(1.6, color);
        p.line_segment(
            [
                r.left_top() + egui::vec2(inset, inset),
                r.right_bottom() - egui::vec2(inset, inset),
            ],
            stroke,
        );
        p.line_segment(
            [
                r.right_top() + egui::vec2(-inset, inset),
                r.left_bottom() + egui::vec2(inset, -inset),
            ],
            stroke,
        );
    })
}

/// Draw a small column-reorder grip (three short lines, two dot pairs) using the
/// painter so it never depends on a font glyph. `active` paints it highlighted
/// while that column is being dragged.
pub(super) fn paint_column_grip(ui: &egui::Ui, rect: egui::Rect, active: bool) {
    let visuals = ui.visuals();
    let color = if active {
        visuals.selection.stroke.color
    } else {
        visuals.weak_text_color()
    };
    let p = ui.painter();
    let stroke = egui::Stroke::new(1.4, color);
    let min = rect.min;
    let w = rect.width();
    // Three rows, each a short line + dot (a classic list-drag grip).
    for row in 0..3 {
        let y = min.y + rect.height() * (row as f32 + 0.5) / 3.0;
        let lx = min.x + 2.0;
        let rx = min.x + w * 0.55;
        p.line_segment([egui::pos2(lx, y), egui::pos2(rx, y)], stroke);
        p.circle_filled(egui::pos2(rx + 5.0, y), 1.2, color);
    }
}

pub(super) fn arc_points(
    center: egui::Pos2,
    radius: f32,
    start: f32,
    end: f32,
    n: usize,
) -> Vec<egui::Pos2> {
    (0..=n)
        .map(|i| {
            let angle = start + (end - start) * (i as f32 / n as f32);
            egui::Pos2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

/// Painter-drawn circular arrow for «reconnect». No glyph, so it can never
/// render as a tofu box on a missing font.
pub(super) fn paint_reconnect_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.36;
    let stroke = egui::Stroke::new(1.6, color);
    // Open arc leaving a gap at the top; the arrowhead shows rotation direction.
    let start = 0.2;
    let end = (std::f32::consts::TAU - 0.8).max(start + 0.5);
    let pts = arc_points(c, r, start, end, 22);
    p.add(egui::Shape::line(pts, stroke));
    // Arrowhead at the start of the arc, pointing along the clockwise tangent.
    let tip = egui::Pos2::new(c.x + r * start.cos(), c.y + r * start.sin());
    let dir = egui::Vec2::new(-start.sin(), start.cos());
    let perp = egui::Vec2::new(-dir.y, dir.x) * 2.6;
    let base = tip - dir * 2.6;
    p.line_segment([tip, base + perp], stroke);
    p.line_segment([tip, base - perp], stroke);
}

/// Painter-drawn pause icon: two vertical bars (no glyph).
pub(super) fn paint_pause_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let w = rect.width();
    let bar = w * 0.22;
    let gap = w * 0.12;
    let x0 = rect.min.x + w * 0.22;
    let top = rect.center().y - rect.height() * 0.32;
    let h = rect.height() * 0.64;
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(x0, top), egui::vec2(bar, h)),
        1.0,
        color,
    );
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(x0 + bar + gap, top), egui::vec2(bar, h)),
        1.0,
        color,
    );
}

/// Painter-drawn play/resume icon: a filled triangle (no glyph).
pub(super) fn paint_play_icon(p: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let w = rect.width();
    let h = rect.height();
    let cx = rect.center();
    let tip = egui::pos2(cx.x + w * 0.32, cx.y);
    let left = egui::pos2(cx.x - w * 0.26, cx.y);
    let p2 = egui::pos2(left.x, cx.y - h * 0.34);
    let p3 = egui::pos2(left.x, cx.y + h * 0.34);
    p.add(egui::Shape::convex_polygon(
        vec![tip, p2, p3],
        color,
        egui::Stroke::NONE,
    ));
}
