use super::super::colors::yellow;
use super::super::paint::paint_column_grip;
#[cfg(test)]
use super::super::test_hooks::{record_column_gap, record_column_grip, record_column_row};
use super::dialog_spacing;
use crate::app::App;
use crate::columns::ColumnId;
use crate::i18n::{Tr, fmt, tr};
use crate::ui::ImeSafeClick;

fn control_clicked_once(response: &egui::Response) -> bool {
    response.clicked_by(egui::PointerButton::Primary)
        || (response.ime_safe_clicked()
            && !crate::ui::key_repeat(&response.ctx, egui::Key::Enter)
            && !crate::ui::key_repeat(&response.ctx, egui::Key::Space))
}

pub(super) fn columns_dialog(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.dialogs.columns.open {
        return;
    }
    let lang = app.lang();
    let mut close = false;
    let mut changed = false;
    let mut order = app.settings.resolved_order();
    // Set the dialog's "just moved" highlight within the same frame the button
    // is pressed so the user immediately sees which row changed.
    let mut moved = None;
    let now = ui.input(|i| i.time);

    egui::Window::new(tr(lang, Tr::ColumnsSetup))
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            dialog_spacing(ui);
            let esc = crate::ui::key_pressed_once(ui.ctx(), egui::Key::Escape);
            let enter = crate::ui::key_pressed_once(ui.ctx(), egui::Key::Enter)
                && ui.input(|input| input.modifiers.is_none());
            ui.label(tr(lang, Tr::ColumnsHint));
            ui.label(egui::RichText::new(tr(lang, Tr::ColumnsReorderHint)).weak());
            const GAP: f32 = 14.0;
            // A column dropped onto the gap before absolute row `idx`.
            let mut drop_at: Option<(ColumnId, usize)> = None;

            // Keyboard fallback: Alt/Ctrl + ArrowUp/ArrowDown move the focused
            // (or first) column, mirroring the drag-and-drop reorder.
            let move_focus = if !crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| {
                    (i.modifiers.alt || i.modifiers.ctrl) && i.key_pressed(egui::Key::ArrowUp)
                }) {
                Some(-1)
            } else if !crate::ui::ime_blocks_keys(ui.ctx())
                && ui.input(|i| {
                    (i.modifiers.alt || i.modifiers.ctrl) && i.key_pressed(egui::Key::ArrowDown)
                })
            {
                Some(1)
            } else {
                None
            };

            // The last interacted/focused column (for the keyboard fallback).
            let mut focus_col = app.dialogs.columns.focused;
            // Enter belongs to the focused control; it must not also apply
            // and close this dialog after activating that control.
            let mut control_has_focus = false;

            for idx in 0..=order.len() {
                // Insertion gap before row `idx` (and one trailing gap at the end).
                let (_gap_inner, released) =
                    ui.dnd_drop_zone::<ColumnId, _>(egui::Frame::NONE, |ui| {
                        ui.add_space(GAP);
                    });
                let gap_resp = _gap_inner.response;
                #[cfg(test)]
                record_column_gap(idx, gap_resp.rect);
                if let Some(payload) = released {
                    drop_at = Some((*payload, idx));
                }
                if gap_resp.dnd_hover_payload::<ColumnId>().is_some() {
                    // Insertion indicator: a full-width accent line.
                    let y = gap_resp.rect.center().y;
                    ui.painter().hline(
                        gap_resp.rect.x_range(),
                        y,
                        egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                    );
                }

                if idx >= order.len() {
                    break; // trailing gap after the last row
                }

                let column = order[idx];
                let highlight = app
                    .dialogs
                    .columns
                    .last_moved
                    .map(|(id, t)| id == column && (now - t).abs() < 1.0)
                    .unwrap_or(false);
                // Lock the reorder while a row drag is in progress so the DnD
                // payload/column indices stay stable within the frame.
                let is_dragged = egui::DragAndDrop::payload::<ColumnId>(ui.ctx())
                    .map(|a| *a == column)
                    .unwrap_or(false);
                let row_ui = ui.horizontal(|ui| {
                    if highlight {
                        ui.painter().rect_filled(
                            ui.max_rect(),
                            2.0,
                            ui.visuals().selection.bg_fill,
                        );
                        ui.add_space(2.0);
                    }
                    // Drag handle (grip) + title are the drag source; the grip is
                    // the visual handle, but the whole row responds to drags.
                    let (grip_rect, grip_resp) =
                        ui.allocate_exact_size(egui::vec2(20.0, 18.0), egui::Sense::drag());
                    #[cfg(test)]
                    record_column_grip(column, grip_rect);
                    let grip = grip_resp.on_hover_text(tr(lang, Tr::ColumnsDragHandle));
                    paint_column_grip(ui, grip_rect, is_dragged);
                    grip.dnd_set_drag_payload(column);
                    let mut visible = app.settings.view.columns.is_visible(column);
                    let cb = ui.checkbox(&mut visible, tr(lang, column.title_tr()));
                    control_has_focus |= cb.has_focus();
                    if cb.changed() && cb.ime_safe_clicked() {
                        app.settings.view.columns.set_visible(column, visible);
                        changed = true;
                    }
                    if cb.has_focus() || cb.clicked() || grip.hovered() {
                        focus_col = Some(column);
                    }
                    let up = ui.small_button("↑");
                    control_has_focus |= up.has_focus();
                    if control_clicked_once(&up) && idx > 0 {
                        order.swap(idx, idx - 1);
                        changed = true;
                        moved = Some((column, idx - 1));
                        focus_col = Some(column);
                    }
                    let down = ui.small_button("↓");
                    control_has_focus |= down.has_focus();
                    if control_clicked_once(&down) && idx + 1 < order.len() {
                        order.swap(idx, idx + 1);
                        changed = true;
                        moved = Some((column, idx + 1));
                        focus_col = Some(column);
                    }
                    if column == ColumnId::Status {
                        ui.label(egui::RichText::new(tr(lang, Tr::IndicatorNote)).weak());
                    } else {
                        ui.label(egui::RichText::new(tr(lang, column.hint_tr())).weak());
                    }
                });
                // A user can release on the row itself, not only on the thin
                // insertion gaps. Top half inserts before; bottom half after.
                let row_rect = row_ui.response.rect;
                #[cfg(test)]
                record_column_row(column, row_rect);
                let after_midpoint = ui
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|pos| pos.y >= row_rect.center().y);
                if let Some(payload) = row_ui.response.dnd_release_payload::<ColumnId>() {
                    drop_at = Some((*payload, idx + usize::from(after_midpoint)));
                }
                if row_ui.response.dnd_hover_payload::<ColumnId>().is_some() {
                    let y = if after_midpoint {
                        row_rect.bottom()
                    } else {
                        row_rect.top()
                    };
                    ui.painter().hline(
                        row_rect.x_range(),
                        y,
                        egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                    );
                }
            }

            if let Some(dir) = move_focus {
                let target = focus_col.unwrap_or_else(|| order[0]).clone();
                if let Some(src) = order.iter().position(|c| *c == target) {
                    let to = src as i32 + dir;
                    if to >= 0 && (to as usize) < order.len() {
                        let to = to as usize;
                        order.swap(src, to);
                        changed = true;
                        moved = Some((target, to));
                        focus_col = Some(target);
                    }
                }
            }
            if let Some((dropped, gap_idx)) = drop_at {
                if app.settings.move_column_to_insert(dropped, gap_idx) {
                    order.clone_from(&app.settings.view.columns.order);
                    changed = true;
                    moved = Some((dropped, gap_idx));
                    focus_col = Some(dropped);
                }
            }
            if focus_col.is_some() {
                app.dialogs.columns.focused = focus_col;
            }
            if let Some((column, _)) = moved {
                app.dialogs.columns.last_moved = Some((column, now));
                ui.colored_label(
                    yellow(),
                    fmt(lang, Tr::MovedFormat, &[&tr(lang, column.title_tr())]),
                );
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            } else if let Some((column, t)) = app.dialogs.columns.last_moved {
                if (now - t).abs() < 1.0 {
                    ui.colored_label(
                        yellow(),
                        fmt(lang, Tr::MovedFormat, &[&tr(lang, column.title_tr())]),
                    );
                    ctx.request_repaint_after(std::time::Duration::from_secs(1));
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                let reset = ui.button(tr(lang, Tr::Reset));
                control_has_focus |= reset.has_focus();
                if control_clicked_once(&reset) {
                    order = ColumnId::ALL.to_vec();
                    app.settings.view.columns.reset();
                    app.settings.view.column_order = None;
                    changed = true;
                }
                let apply = ui.button(tr(lang, Tr::Apply));
                control_has_focus |= apply.has_focus();
                if control_clicked_once(&apply) {
                    close = true;
                }
            });
            if enter && !control_has_focus {
                close = true;
            } else if esc {
                close = true;
            }
        });
    if changed {
        // Keep the persisted header-menu order in sync with the dialog. Write
        // directly from our local `order` so the moved row reorders immediately
        // and the table behind the dialog re-renders with it.
        app.settings.persist_column_order(&order);
        app.persist_settings();
    }
    if close {
        app.dialogs.columns.open = false;
        app.dialogs.columns.last_moved = None;
    }
}
