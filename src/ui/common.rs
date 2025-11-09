// src/ui/common.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Response, Sense};
use std::collections::HashSet;
use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    events::{
        RequestCopyCell, RequestPasteCell, RequestToggleAiRowGeneration,
    },
    resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache},
    systems::logic::{
        determine_cell_background_color, determine_effective_validation_state,
        is_column_ai_included, is_structure_column_ai_included, prefetch_linked_column_values,
    },
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::validation::ValidationState;
use crate::ui::widgets::{
    handle_linked_column_edit, add_cell_context_menu, add_centered_checkbox, add_numeric_drag_value,
    render_technical_column, render_structure_column,
};
#[allow(clippy::too_many_arguments, unused_variables, unused_assignments)]
pub fn edit_cell_widget(
    ui: &mut egui::Ui,
    id: egui::Id,
    validator_opt: &Option<ColumnValidator>,
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
    col_index: usize,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    state: &mut EditorWindowState,
    _toggle_ai_events: &mut EventWriter<RequestToggleAiRowGeneration>,
    copy_events: &mut EventWriter<RequestCopyCell>,
    paste_events: &mut EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
) -> Option<String> {
    let is_column_selected_for_deletion = state.selected_columns_for_deletion.contains(&col_index);
    let is_row_selected = state.ai_selected_rows.contains(&row_index);
    let current_interaction_mode = state.current_interaction_mode;
    let render_cell_data_opt =
        render_cache.get_cell_data(category, sheet_name, row_index, col_index);
    let (current_display_text, cell_validation_state) = match render_cell_data_opt {
        Some(data) => (data.display_text.as_str(), data.validation_state),
        None => {
            warn!(
                "Render cache miss for cell [{}/{}, {}/{}]. Defaulting.",
                category.as_deref().unwrap_or("root"),
                sheet_name,
                row_index,
                col_index
            );
            ("", ValidationState::default())
        }
    };
    let basic_type = registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index))
        .map_or(ColumnDataType::String, |col_def| col_def.data_type);
    let prefetch = prefetch_linked_column_values(validator_opt, registry, state);
    let prefetch_allowed_values = prefetch.raw_values;
    let prefetch_allowed_values_norm = prefetch.normalized_values;

    // Check if this is a technical column and render it with special styling
    if render_technical_column(ui, col_index, current_display_text, registry, state, category, sheet_name) {
        return None;
    }
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (frame_id, frame_rect) = ui.allocate_space(desired_size);
    let effective_validation_state = determine_effective_validation_state(
        current_display_text,
        &prefetch_allowed_values_norm,
        cell_validation_state,
    );
    let col_ai_included = is_column_ai_included(state, category, sheet_name, col_index);
    let is_structure_column = matches!(
        validator_opt,
        Some(ColumnValidator::Structure)
    );
    let is_linked_column = matches!(
        validator_opt,
        Some(ColumnValidator::Linked { .. })
    );
    let struct_ai_included =
        is_structure_column_ai_included(state, category, sheet_name, col_index, is_structure_column);
    let bg_color = determine_cell_background_color(
        is_column_selected_for_deletion,
        is_row_selected,
        current_interaction_mode,
        is_structure_column,
        struct_ai_included,
        col_ai_included,
        effective_validation_state,
        is_linked_column,
        basic_type,
    );
    let frame = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(2, 1))
        .fill(bg_color);
    let inner_response = ui
        .allocate_new_ui(egui::UiBuilder::new().max_rect(frame_rect), |frame_ui| {
            frame.show(frame_ui, |widget_ui| {
                    let mut response_opt: Option<Response> = None;
                    let mut temp_new_value: Option<String> = None;
                    match validator_opt {
                            Some(ColumnValidator::Structure) => {
                                let (resp, new_val) = render_structure_column(
                                    widget_ui,
                                    col_index,
                                    row_index,
                                    current_display_text,
                                    registry,
                                    state,
                                    category,
                                    sheet_name,
                                );
                                response_opt = resp;
                                temp_new_value = new_val;
                            }
                            Some(ColumnValidator::Linked {
                                target_sheet_name,
                                target_column_index,
                            }) => {
                                let empty_backing_local;
                                let allowed_values: &HashSet<String> =
                                    if let Some(values) = prefetch_allowed_values.as_ref() {
                                        values.as_ref()
                                    } else {
                                        empty_backing_local = HashSet::new();
                                        &empty_backing_local
                                    };
                                let (new_val, resp) = widget_ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(
                                            widget_ui.available_width(),
                                            widget_ui.style().spacing.interact_size.y,
                                        ),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |row_ui| {
                                            row_ui.horizontal(|ui_h| {
                                                let nav_button_size = ui_h.style().spacing.interact_size.y;
                                                let spacing = ui_h.spacing().item_spacing.x;
                                                let available_for_edit =
                                                    (ui_h.available_width() - nav_button_size - spacing)
                                                        .max(8.0);
                                                let (new_val, resp) = ui_h
                                                    .allocate_ui_with_layout(
                                                        egui::vec2(available_for_edit, nav_button_size),
                                                        egui::Layout::left_to_right(egui::Align::Center),
                                                        |edit_ui| {
                                                            edit_ui.vertical_centered(|vc| {
                                                                handle_linked_column_edit(
                                                                    vc,
                                                                    id,
                                                                    current_display_text,
                                                                    target_sheet_name,
                                                                    *target_column_index,
                                                                    registry,
                                                                    allowed_values,
                                                                )
                                                            })
                                                            .inner
                                                        },
                                                    )
                                                    .inner;
                                                let nav_btn = ui_h
                                                    .add_sized([
                                                        nav_button_size,
                                                        nav_button_size,
                                                    ], egui::Button::new(">"))
                                                    .on_hover_text(format!(
                                                        "Navigate to sheet: {}",
                                                        target_sheet_name
                                                    ));
                                                if nav_btn.clicked() {
                                                    state.selected_category = category.clone();
                                                    state.selected_sheet_name =
                                                        Some(target_sheet_name.clone());
                                                    state.reset_interaction_modes_and_selections();
                                                    state.force_filter_recalculation = true;
                                                }
                                                (new_val, resp)
                                            })
                                            .inner
                                        },
                                    )
                                    .inner;
                                temp_new_value = new_val;
                                let resp = add_cell_context_menu(
                                    resp,
                                    category,
                                    sheet_name,
                                    row_index,
                                    col_index,
                                    copy_events,
                                    paste_events,
                                    clipboard_buffer,
                                    &mut temp_new_value,
                                );
                                response_opt = Some(resp);
                            }
                            Some(ColumnValidator::Basic(_)) | None => {
                                match basic_type {
                                    ColumnDataType::String => {
                                        let mut temp_string = current_display_text.to_string();
                                        let resp = widget_ui.add_sized(
                                            widget_ui.available_size(),
                                            egui::TextEdit::singleline(&mut temp_string)
                                                .frame(false),
                                        );
                                        if resp.changed() {
                                            temp_new_value = Some(temp_string);
                                        }
                                        let resp = add_cell_context_menu(
                                            resp,
                                            category,
                                            sheet_name,
                                            row_index,
                                            col_index,
                                            copy_events,
                                            paste_events,
                                            clipboard_buffer,
                                            &mut temp_new_value,
                                        );
                                        response_opt = Some(resp);
                                    }
                                    ColumnDataType::Bool => {
                                        let mut value_for_widget = matches!(
                                            current_display_text.to_lowercase().as_str(),
                                            "true" | "1"
                                        );
                                        let resp = add_centered_checkbox(widget_ui, &mut value_for_widget);
                                        if resp.changed() {
                                            temp_new_value = Some(value_for_widget.to_string());
                                        }
                                        let resp = add_cell_context_menu(
                                            resp,
                                            category,
                                            sheet_name,
                                            row_index,
                                            col_index,
                                            copy_events,
                                            paste_events,
                                            clipboard_buffer,
                                            &mut temp_new_value,
                                        );
                                        response_opt = Some(resp);
                                    }
                                    ColumnDataType::I64 => {
                                        let mut value_for_widget: i64 =
                                            current_display_text.parse().unwrap_or(0);
                                        let resp = add_numeric_drag_value(widget_ui, &mut value_for_widget, 1.0);
                                        if resp.changed() {
                                            temp_new_value = Some(value_for_widget.to_string());
                                        }
                                        let resp = add_cell_context_menu(
                                            resp,
                                            category,
                                            sheet_name,
                                            row_index,
                                            col_index,
                                            copy_events,
                                            paste_events,
                                            clipboard_buffer,
                                            &mut temp_new_value,
                                        );
                                        response_opt = Some(resp);
                                    }
                                    ColumnDataType::F64 => {
                                        let mut value_for_widget: f64 =
                                            current_display_text.parse().unwrap_or(0.0);
                                        let resp = add_numeric_drag_value(widget_ui, &mut value_for_widget, 0.1);
                                        if resp.changed() {
                                            temp_new_value = Some(value_for_widget.to_string());
                                        }
                                        let resp = add_cell_context_menu(
                                            resp,
                                            category,
                                            sheet_name,
                                            row_index,
                                            col_index,
                                            copy_events,
                                            paste_events,
                                            clipboard_buffer,
                                            &mut temp_new_value,
                                        );
                                        response_opt = Some(resp);
                                    }
                                }
                            }
                        }
                    (response_opt, temp_new_value)
                })
                .inner
        })
        .inner;
    let (_widget_resp_opt, final_new_value) = inner_response;
    if effective_validation_state == ValidationState::Invalid {
        let hover_text = format!(
            "Invalid Value! '{}' is not allowed here.",
            current_display_text
        );
        ui.interact(frame_rect, frame_id.with("hover_invalid"), Sense::hover())
            .on_hover_text(hover_text);
    }
    final_new_value
}
