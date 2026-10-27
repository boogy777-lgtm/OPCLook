use super::actions::{ColumnDir, TableAction, TreeDragPayload};
use super::colors::{green, quality_color, yellow};
use super::files::save_tree_csv;
use super::navigation::tags_context_menu;
use super::servers::session_tab_strip;
#[cfg(test)]
use super::test_hooks::{record_header_cell, record_table_scroll, set_tags_rect_for_test};
use super::tree::refresh_selected;
use crate::app::App;
use crate::columns::ColumnId;
use crate::constants::{MIN_COLUMN_WIDTH, TABLE_HEADER_HEIGHT, TABLE_ROW_HEIGHT};
use crate::i18n::{Tr, fmt, tr};
use egui_extras::{Column, TableBuilder};

pub(super) fn table_panel(app: &mut App, ui: &mut egui::Ui) {
    let lang = app.lang();
    // Paint the whole central area opaquely with the panel background so the
    // central region never shows a black seam while the left panel is resized.
    let central_frame = egui::Frame::central_panel(&ui.style()).fill(ui.style().visuals.panel_fill);
    let _panel = egui::CentralPanel::default()
        .frame(central_frame)
        .show(ui, |ui| {
            let lang = lang;
            // Session tab strip lives at the top of the central tags panel, not in
            // a window-wide top panel. It belongs to the tags area, so it stays
            // reachable even when the left nav/tree is collapsed.
            session_tab_strip(app, ui);
            ui.separator();
            // Opaque drop-zone frame: the tags area owns the full central panel
            // background (no transparent defaults leaking a black band between
            // panels, and no extra padding shifting the table).
            let drop_frame = egui::Frame::NONE.fill(ui.style().visuals.panel_fill);
            // The drop target must cover unused space below an empty/short table.
            let drop_height = ui.available_height();
            let (dropped_zone, dropped) =
                ui.dnd_drop_zone::<TreeDragPayload, _>(drop_frame, |ui| {
                    ui.set_min_height(drop_height);
                    let mut actions = Vec::new();
                    let mut columns: Vec<ColumnId> = app.settings.columns.visible().collect();

                    if columns.is_empty() {
                        columns.push(ColumnId::Status);
                    }
                    // Prefer the persisted header-menu order, then intersect with
                    // visibility.
                    let ordered: Vec<ColumnId> = app
                        .settings
                        .resolved_order()
                        .into_iter()
                        .filter(|id| columns.contains(id))
                        .collect();
                    let columns: Vec<ColumnId> = if ordered.is_empty() { columns } else { ordered };

                    // The tags (central) area is ALWAYS the signal table: headers
                    // and body render even with zero rows, so rows/columns never
                    // disappear and the drop zone covers the area. The table is
                    // bounded by the outer horizontal scroll area below, so it never
                    // shrink-wraps to its content (which would shift the header each
                    // time rows are added/dropped).

                    let selected_count = app.selected_rows.len();
                    let count_strip = ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(fmt(
                                lang,
                                Tr::SignalsCount,
                                &[&app.rows.len().to_string(), &selected_count.to_string()],
                            ))
                            .weak(),
                        );
                        if selected_count > 0 {
                            ui.label(egui::RichText::new(tr(lang, Tr::EscHint)).weak());
                        }
                    });
                    // The «Сигналов: N | Выбрано: M» strip is tags-area background: right
                    // clicking it opens the panel context menu (it never overlaps a row).
                    count_strip
                        .response
                        .container_context_menu(|ui| tags_context_menu(app, ui));
                    ui.separator();

                    let text_height = TABLE_ROW_HEIGHT;
                    let table_ctx = ui.ctx().clone();
                    let empty = app.rows.is_empty();
                    // The egui_extras table only scrolls vertically and its internal
                    // ScrollArea shrink-wraps to content width, which would let the table
                    // overflow and/or shift the header on every row change. Wrapping it in
                    // an outer horizontal ScrollArea bounds the table to the panel width
                    // (real horizontal scrolling for the wide 9-column layout) and keeps the
                    // header/rows fixed so adding rows or dropping never moves them.
                    let table_out = egui::ScrollArea::horizontal()
                        .id_salt("signal_table_hscroll")
                        .auto_shrink([true, true])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                        .show(ui, |ui| {
                            let mut table = TableBuilder::new(ui)
                                .id_salt("signal_table")
                                .striped(true)
                                .sense(egui::Sense::click())
                                // Rows are full-width, so the internal area must never shrink
                                // horizontally (content defines the width the outer horizontal
                                // ScrollArea scrolls). Vertically we never shrink so the scrollbar
                                // appears when rows overflow the panel.
                                .auto_shrink([true, true])
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                )
                                // Dragging inside the table (incl. starting a DnD from the tree)
                                // must never scroll the tags table.
                                .drag_to_scroll(egui::scroll_area::DragScroll::Never)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
                            let table_columns = columns.clone();
                            for column in &table_columns {
                                table = table.column(
                                    Column::initial(column.width())
                                        .at_least(MIN_COLUMN_WIDTH)
                                        .clip(true),
                                );
                            }
                            table
                                .header(TABLE_HEADER_HEIGHT, |mut header| {
                                    for (index, column) in table_columns.iter().enumerate() {
                                        header.col(|ui| {
                                            // The entire header cell is a drag source and target.
                                            // Column widths and the body stay owned by TableBuilder.
                                            if *column == ColumnId::Status {
                                                ui.label("");
                                            } else {
                                                ui.strong(tr(lang, column.title_tr()));
                                            }
                                            let rect = ui.max_rect();
                                            #[cfg(test)]
                                            record_header_cell(*column, rect);
                                            // A tree payload belongs to the outer table drop
                                            // zone. Do not let header drag widgets consume it.
                                            let dropping_tree =
                                                egui::DragAndDrop::payload::<TreeDragPayload>(
                                                    ui.ctx(),
                                                )
                                                .is_some();
                                            if !dropping_tree {
                                                let response = ui.interact(
                                                    rect,
                                                    ui.scope_id()
                                                        .with(("signal_column_header", *column)),
                                                    egui::Sense::click_and_drag(),
                                                );
                                                response.dnd_set_drag_payload(*column);
                                                if let Some(payload) =
                                                    response.dnd_release_payload::<ColumnId>()
                                                {
                                                    let after_midpoint = ui
                                                        .input(|i| i.pointer.hover_pos())
                                                        .is_some_and(|pos| {
                                                            pos.x >= rect.center().x
                                                        });
                                                    actions.push(TableAction::DropColumn(
                                                        *payload,
                                                        index + usize::from(after_midpoint),
                                                    ));
                                                }
                                                if response
                                                    .dnd_hover_payload::<ColumnId>()
                                                    .is_some()
                                                {
                                                    let after_midpoint = ui
                                                        .input(|i| i.pointer.hover_pos())
                                                        .is_some_and(|pos| {
                                                            pos.x >= rect.center().x
                                                        });
                                                    let x = if after_midpoint {
                                                        rect.right()
                                                    } else {
                                                        rect.left()
                                                    };
                                                    ui.painter().vline(
                                                        x,
                                                        rect.y_range(),
                                                        egui::Stroke::new(
                                                            2.0,
                                                            ui.visuals().selection.stroke.color,
                                                        ),
                                                    );
                                                }
                                                response
                                                    .on_hover_text(tr(lang, Tr::PkmMoveColumn))
                                                    .context_menu(|ui| {
                                                        if ui
                                                            .button(tr(lang, Tr::MoveLeft))
                                                            .clicked()
                                                        {
                                                            actions.push(TableAction::MoveColumn(
                                                                *column,
                                                                ColumnDir::Left,
                                                            ));
                                                            ui.close();
                                                        }
                                                        if ui
                                                            .button(tr(lang, Tr::MoveRight))
                                                            .clicked()
                                                        {
                                                            actions.push(TableAction::MoveColumn(
                                                                *column,
                                                                ColumnDir::Right,
                                                            ));
                                                            ui.close();
                                                        }
                                                    });
                                            }
                                        });
                                    }
                                })
                                .body(|mut body| {
                                    if empty {
                                        // Reserve a body row so the table headers stay visible and
                                        // the empty hint sits on the same column grid. The hint is
                                        // painted (non-interactive) so it never blocks drops.
                                        body.row(64.0, |mut row| {
                                            for _ in 0..table_columns.len() {
                                                row.col(|_ui| {});
                                            }
                                            // Right-clicking the empty body opens the panel context
                                            // menu (no data rows here, so the row menu can't conflict).
                                            row.response().container_context_menu(|ui| {
                                                tags_context_menu(app, ui)
                                            });
                                        });
                                        return;
                                    }
                                    body.rows(text_height, app.rows.len(), |mut row| {
                                        let index = row.index();
                                        let Some(signal) = app.rows.get(index) else {
                                            return;
                                        };
                                        let key = signal.key;
                                        row.set_selected(app.selected_rows.contains(&key));
                                        for (column_index, column) in columns.iter().enumerate() {
                                            match column {
                                                ColumnId::Status => {
                                                    row.col(|ui| {
                                                        let color = if signal.pending {
                                                            yellow()
                                                        } else {
                                                            quality_color(signal.quality.kind)
                                                        };
                                                        ui.colored_label(color, "●");
                                                    });
                                                }
                                                ColumnId::FullName => {
                                                    row.col(|ui| {
                                                        ui.label(&signal.display_name)
                                                            .on_hover_text(&signal.item_id);
                                                    });
                                                }
                                                ColumnId::WriteType => {
                                                    row.col(|ui| {
                                                        ui.label(signal.write_type_text());
                                                    });
                                                }
                                                ColumnId::Type => {
                                                    row.col(|ui| {
                                                        ui.label(
                                                            signal
                                                                .data_type
                                                                .as_deref()
                                                                .unwrap_or(""),
                                                        );
                                                    });
                                                }
                                                ColumnId::Value => {
                                                    row.col(|ui| {
                                                        let color =
                                                            if app.settings.options.show_colors {
                                                                quality_color(signal.quality.kind)
                                                            } else {
                                                                ui.visuals().text_color()
                                                            };
                                                        ui.label(
                                                            egui::RichText::new(
                                                                signal.value.to_string(),
                                                            )
                                                            .monospace()
                                                            .color(color),
                                                        )
                                                        .on_hover_text(&signal.quality.text);
                                                    });
                                                }
                                                ColumnId::Quality => {
                                                    row.col(|ui| {
                                                        let color =
                                                            if app.settings.options.show_colors {
                                                                quality_color(signal.quality.kind)
                                                            } else {
                                                                ui.visuals().text_color()
                                                            };
                                                        ui.label(
                                                            egui::RichText::new(
                                                                &signal.quality.text,
                                                            )
                                                            .color(color),
                                                        );
                                                    });
                                                }
                                                ColumnId::Time => {
                                                    row.col(|ui| {
                                                        ui.label(signal.timestamp_text());
                                                    });
                                                }
                                                ColumnId::Server => {
                                                    row.col(|ui| {
                                                        ui.label(&signal.server_label);
                                                    });
                                                }
                                                ColumnId::Description => {
                                                    row.col(|ui| {
                                                        ui.label(&signal.description);
                                                    });
                                                }
                                            }
                                            let _ = column_index;
                                        }
                                        let response = row.response();
                                        if response.clicked() {
                                            let modifiers = table_ctx.input(|i| i.modifiers);
                                            if modifiers.shift && app.table_anchor.is_some() {
                                                actions.push(TableAction::SelectExtend(index));
                                            } else if modifiers.ctrl || modifiers.command {
                                                actions.push(TableAction::SelectToggle(index));
                                            } else {
                                                actions.push(TableAction::SelectSingle(index));
                                            }
                                        }
                                        if response.double_clicked() {
                                            actions.push(TableAction::Write(key));
                                        }
                                        response.context_menu(|ui| {
                                            if ui.button(tr(lang, Tr::ChangeValue)).clicked() {
                                                actions.push(TableAction::Write(key));
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui.button(tr(lang, Tr::CopyFullNameCtx)).clicked() {
                                                actions.push(TableAction::CopyFullName(key));
                                                ui.close();
                                            }
                                            if ui.button(tr(lang, Tr::CopyValueCtx)).clicked() {
                                                actions.push(TableAction::CopyValue(key));
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui.button(tr(lang, Tr::RemoveSelected)).clicked() {
                                                actions.push(TableAction::Remove(key));
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui.button(tr(lang, Tr::SelectAllCtx)).clicked() {
                                                actions.push(TableAction::SelectAll);
                                                ui.close();
                                            }
                                            if ui.button(tr(lang, Tr::ClearSelectionCtx)).clicked()
                                            {
                                                actions.push(TableAction::ClearSelection);
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui.button(tr(lang, Tr::SaveCsvCtx)).clicked() {
                                                actions.push(TableAction::SaveCsv);
                                                ui.close();
                                            }
                                        });
                                    })
                                })
                        });
                    let table_out = table_out.inner;

                    // Localized hint INSIDE the (otherwise empty) table body. Painted, not
                    // a widget, so it cannot swallow drops.
                    if empty {
                        let rect = table_out.inner_rect;
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            tr(lang, Tr::TableEmptyInner),
                            egui::FontId::proportional(15.0),
                            ui.visuals().weak_text_color(),
                        );
                    }

                    #[cfg(test)]
                    record_table_scroll(&table_out);

                    actions
                });

            #[cfg(test)]
            set_tags_rect_for_test(dropped_zone.response.rect);

            let actions = dropped_zone.inner;
            let dropped = dropped;

            // Visual feedback while a tag tree payload hovers over the table.
            let hovering = dropped_zone
                .response
                .dnd_hover_payload::<TreeDragPayload>()
                .is_some();
            if hovering {
                let rect = ui.available_rect_before_wrap();
                let hint = egui::RichText::new(tr(lang, Tr::DropToAdd))
                    .strong()
                    .color(green());
                egui::Area::new(egui::Id::unique("drop_to_add_hint"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(rect.center().x, rect.top() + 10.0))
                    .show(ui.ctx(), |ui| {
                        ui.label(hint);
                    });
            }

            if let Some(payload) = dropped {
                app.add_dropped_tree(payload.session, payload.node_ids.clone());
            }

            for action in actions {
                match action {
                    TableAction::SelectSingle(index) => app.select_only(index),
                    TableAction::SelectToggle(index) => app.select_toggle(index),
                    TableAction::SelectExtend(index) => app.select_extend_to(index),
                    TableAction::Write(key) => {
                        let keys = app.selected_rows_for_context(key);
                        app.open_write_dialog(keys);
                    }
                    TableAction::Remove(key) => {
                        if app.selected_rows.contains(&key) {
                            app.remove_selected_rows();
                        } else {
                            app.selected_rows.clear();
                            app.selected_rows.insert(key);
                            app.remove_selected_rows();
                        }
                    }
                    TableAction::CopyValue(key) => {
                        if let Some(row) = app.rows.iter().find(|r| r.key == key) {
                            ui.ctx().copy_text(row.value.to_string());
                            app.status = tr(app.lang(), Tr::ValueCopied).to_string();
                        }
                    }
                    TableAction::CopyFullName(key) => {
                        if let Some(row) = app.rows.iter().find(|r| r.key == key) {
                            ui.ctx().copy_text(row.item_id.clone());
                            app.status = tr(app.lang(), Tr::FullNameCopied).to_string();
                        }
                    }
                    TableAction::SelectAll => app.select_all_rows(),
                    TableAction::ClearSelection => app.clear_selection(),
                    TableAction::SaveCsv => save_tree_csv(app),
                    TableAction::MoveColumn(column, dir) => {
                        let order = app.settings.resolved_order();
                        let Some(index) = order.iter().position(|c| *c == column) else {
                            continue;
                        };
                        let before = match dir {
                            ColumnDir::Left if index > 0 => index - 1,
                            ColumnDir::Right if index + 1 < order.len() => index + 2,
                            _ => continue,
                        };
                        if app.settings.move_column_to_insert(column, before) {
                            app.settings.save();
                            let title = tr(app.lang(), column.title_tr());
                            app.status = if dir == ColumnDir::Left {
                                fmt(app.lang(), Tr::ColumnMovedLeft, &[&title])
                            } else {
                                fmt(app.lang(), Tr::ColumnMovedRight, &[&title])
                            };
                        }
                    }
                    TableAction::DropColumn(column, before) => {
                        if app.settings.move_column_to_insert(column, before) {
                            app.settings.save();
                            let title = tr(app.lang(), column.title_tr());
                            app.status = fmt(app.lang(), Tr::MovedFormat, &[&title]);
                        }
                    }
                }
            }
        });
}
pub(super) fn handle_table_keys(app: &mut App, ui: &mut egui::Ui) {
    if app.rows.is_empty() {
        app.table_focus = None;
        app.table_anchor = None;
        return;
    }
    let len = app.rows.len();
    let ctx = ui.ctx();
    let focus = app.table_focus.unwrap_or(0);
    let shift = ctx.input(|i| i.modifiers.shift);
    let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);

    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        let target = (focus + 1).min(len - 1);
        if shift {
            app.select_extend_to(target);
        } else {
            app.select_only(target);
        }
    } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        let target = focus.saturating_sub(1);
        if shift {
            app.select_extend_to(target);
        } else {
            app.select_only(target);
        }
    } else if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
        let target = (focus + 10).min(len - 1);
        if shift {
            app.select_extend_to(target);
        } else {
            app.select_only(target);
        }
    } else if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
        let target = focus.saturating_sub(10);
        if shift {
            app.select_extend_to(target);
        } else {
            app.select_only(target);
        }
    } else if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
        if shift {
            app.select_extend_to(0);
        } else {
            app.select_only(0);
        }
    } else if ctx.input(|i| i.key_pressed(egui::Key::End)) {
        if shift {
            app.select_extend_to(len - 1);
        } else {
            app.select_only(len - 1);
        }
    }

    if ctrl && ctx.input(|i| i.key_pressed(egui::Key::A)) {
        app.select_all_rows();
    }
    if ctrl && ctx.input(|i| i.key_pressed(egui::Key::C)) {
        if let Some(text) = app.copy_selected_rows() {
            ctx.copy_text(text);
            app.status = tr(app.lang(), Tr::Copied).to_string();
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
        app.remove_selected_rows();
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) || ctx.input(|i| i.key_pressed(egui::Key::F2))
    {
        app.open_write_dialog(Vec::new());
    }
    if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
        refresh_selected(app);
    }
}
