use super::*;

/// Height reserved for the empty-table prompt (hint, second line, button).
const EMPTY_PROMPT_HEIGHT: f32 = 110.0;

/// UI10: the prompt of an empty table, centred in `area`. Without an active
/// session it offers «Connect (Ctrl+N)» (returns true when pressed); with one
/// it says how to fill the table. Never keeps the keyboard focus, so Enter and
/// the arrows stay with the table / tree.
pub(super) fn empty_table_prompt(
    ui: &mut egui::Ui,
    app: &crate::app::App,
    lang: crate::i18n::Lang,
    area: egui::Rect,
    has_session: bool,
) -> bool {
    let rect = egui::Rect::from_center_size(
        area.center(),
        egui::vec2(area.width(), EMPTY_PROMPT_HEIGHT.min(area.height())),
    );
    let mut connect = false;
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(tr(lang, Tr::TableEmptyInner))
                        .size(18.0)
                        .color(ui.visuals().weak_text_color()),
                )
                .selectable(false),
            );
            ui.add_space(6.0);
            let second = if has_session {
                Tr::TableEmptyDragHint
            } else {
                Tr::TableEmptyKeys
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(crate::i18n::fmt(
                        lang,
                        second,
                        &[
                            &crate::ui::shortcuts::connect_shortcut_text(ui.ctx()),
                            &crate::ui::shortcuts::binding_texts(
                                ui.ctx(),
                                crate::ui::keymap::KeyAction::SwitchPane,
                            ),
                        ],
                    ))
                    .weak(),
                )
                .selectable(false),
            );
            if !has_session {
                ui.add_space(12.0);
                let button = crate::ui::actions::action_button(
                    app,
                    ui,
                    crate::app::actions::UiAction::OpenConnectDialog,
                    egui::Button::new(crate::i18n::fmt(
                        lang,
                        Tr::TableEmptyConnect,
                        &[&crate::ui::shortcuts::connect_shortcut_text(ui.ctx())],
                    ))
                    .min_size(egui::vec2(180.0, 28.0)),
                );
                // A pressed button must not keep egui's focus: Enter would press
                // it again (the table owns Enter).
                if button.has_focus() {
                    button.surrender_focus();
                }
                connect = button.clicked();
            }
        },
    );
    connect
}

/// UI4: the accent-coloured glow behind a value that has just changed. `flash`
/// runs from 1.0 (just changed) to 0.0; while it fades the table keeps asking
/// for new frames, so the glow dies out smoothly even with no other input.
pub(super) fn paint_change_flash(ui: &egui::Ui, flash: f32) {
    if flash <= 0.0 {
        return;
    }
    let accent = ui.visuals().selection.bg_fill;
    ui.painter().rect_filled(
        ui.max_rect(),
        2.0,
        accent.gamma_multiply(CHANGE_FLASH_MAX_ALPHA * flash),
    );
    ui.ctx().request_repaint();
}

/// The glow's opacity right after a change.
const CHANGE_FLASH_MAX_ALPHA: f32 = 0.8;

/// Gap between a right-aligned number and the right edge of its cell.
const NUMBER_RIGHT_PADDING: f32 = 8.0;

/// UI5: a value in the «Value» column. A number is monospace and sticks to the
/// right edge of the cell; strings, structure summaries, dates and the rest
/// keep the left edge.
pub(super) fn value_text_cell(
    ui: &mut egui::Ui,
    text: impl Into<String>,
    numeric: bool,
    color: egui::Color32,
) -> egui::Response {
    let text = text.into();
    if numeric {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // A little air before the next column (the quality pill).
            ui.add_space(NUMBER_RIGHT_PADDING);
            ui.label(egui::RichText::new(text).monospace().color(color))
        })
        .inner
    } else {
        ui.label(egui::RichText::new(text).color(color))
    }
}

/// UI2: the color a quality is shown in: yellow while the first value is still
/// pending, else the quality's color; the neutral text color when the
/// «show colors» option is off.
pub(super) fn quality_tint(
    ui: &egui::Ui,
    show_colors: bool,
    pending: bool,
    kind: crate::bff::model::QualityKind,
) -> egui::Color32 {
    if !show_colors {
        ui.visuals().weak_text_color()
    } else if pending {
        yellow()
    } else {
        quality_color(kind)
    }
}

/// UI2: a quality as a small rounded pill: the quality's color on a soft
/// background of the same color. `tint` is `None` when colors are switched off:
/// then the pill is neutral.
pub(super) fn quality_pill(ui: &mut egui::Ui, text: &str, tint: Option<egui::Color32>) {
    let (fg, bg) = match tint {
        Some(color) => (color, color.gamma_multiply(QUALITY_PILL_BG_STRENGTH)),
        None => (
            ui.visuals().text_color(),
            ui.visuals().widgets.noninteractive.weak_bg_fill,
        ),
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(text).color(fg).size(12.0))
                    .selectable(false)
                    .truncate(),
            );
        });
}

/// How strongly the quality color shows through in the pill's background.
const QUALITY_PILL_BG_STRENGTH: f32 = 0.08;

/// One expanded field of a row value, borrowed from its [`TableLine`].
pub(super) struct FieldLine<'a> {
    pub(super) key: crate::bff::model::ItemKey,
    pub(super) path: &'a [u32],
    pub(super) depth: usize,
    pub(super) name: &'a str,
    pub(super) value_text: &'a str,
    pub(super) type_text: &'a str,
    pub(super) expandable: bool,
    pub(super) expanded: bool,
    pub(super) numeric: bool,
}

/// The expander of a structured value (same look as the tag tree): a painted
/// triangle that turns smoothly between closed and open. Returns its rect: the
/// table row owns the pointer, so the caller turns a row click inside this rect
/// into expand / collapse. The accessible name says what a click does.
pub(super) fn value_toggle(
    ui: &mut egui::Ui,
    lang: crate::i18n::Lang,
    id: egui::Id,
    expanded: bool,
) -> egui::Rect {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let name = tr(
        lang,
        if expanded {
            Tr::TableValueCollapse
        } else {
            Tr::TableValueExpand
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    let openness = ui.ctx().animate_bool(id, expanded);
    let color = if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    crate::ui::paint::paint_expander(ui.painter(), rect, openness, color);
    rect
}

/// V3: a field / element line under an expanded row: indented name with its
/// own expander, the field value and, for a nested structure, its type name.
/// Clicking it selects the row it belongs to.
pub(super) fn draw_field_line(
    row: &mut egui_extras::TableRow<'_, '_>,
    columns: &[ColumnId],
    lang: crate::i18n::Lang,
    field: &FieldLine<'_>,
    index: usize,
    actions: &mut Vec<TableAction>,
) {
    let mut toggle_rect = None;
    for column in columns {
        match column {
            ColumnId::FullName => {
                row.col(|ui| {
                    ui.add_space(field.depth as f32 * 16.0);
                    if field.expandable {
                        toggle_rect = Some(value_toggle(
                            ui,
                            lang,
                            egui::Id::NULL.with(("value_toggle", field.key, field.path)),
                            field.expanded,
                        ));
                    } else {
                        ui.add_space(16.0 + ui.spacing().item_spacing.x);
                    }
                    ui.label(egui::RichText::new(field.name).weak());
                });
            }
            ColumnId::Value => {
                row.col(|ui| {
                    let color = ui.visuals().text_color();
                    value_text_cell(ui, field.value_text, field.numeric, color);
                });
            }
            ColumnId::Type => {
                row.col(|ui| {
                    ui.label(egui::RichText::new(field.type_text).weak());
                });
            }
            _ => {
                row.col(|_ui| {});
            }
        }
    }
    let response = row.response();
    let on_toggle = toggle_rect.is_some_and(|rect: egui::Rect| {
        response
            .interact_pointer_pos()
            .is_some_and(|pos| rect.expand(2.0).contains(pos))
    });
    if response.clicked() && on_toggle {
        actions.push(TableAction::ToggleValue(field.key, field.path.to_vec()));
    } else if response.clicked() {
        actions.push(TableAction::SelectSingle(index));
    }
}
