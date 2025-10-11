// src/ui/elements/editor/display/display_helpers.rs
// Helper functions for sheet display validation and setup

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

/// Validates and clears virtual structure stack if user switched to a different sheet
pub fn validate_virtual_structure_stack(state: &mut EditorWindowState) {
    if !state.virtual_structure_stack.is_empty() {
        if let Some(current_sel) = &state.selected_sheet_name {
            // Root parent sheet is the parent sheet of the first (oldest) virtual context
            let root_parent_sheet = state
                .virtual_structure_stack
                .first()
                .map(|v| v.parent.parent_sheet.clone());
            let root_parent_category_opt = state
                .virtual_structure_stack
                .first()
                .and_then(|v| v.parent.parent_category.clone());
            // If user changed either sheet name or category away from the root parent, clear stack
            let category_changed = root_parent_category_opt != state.selected_category;
            if root_parent_sheet.as_ref() != Some(current_sel) || category_changed {
                state.virtual_structure_stack.clear();
            }
        } else {
            // No sheet selected anymore -> clear
            state.virtual_structure_stack.clear();
        }
    }
}

/// Attempts to override selected sheet with virtual sheet if active
/// Returns true if override was applied
pub fn try_apply_virtual_override(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
) -> bool {
    if let Some(vctx) = state.virtual_structure_stack.last() {
        if let Some(vsheet) = registry.get_sheet(&state.selected_category, &vctx.virtual_sheet_name)
        {
            if vsheet.metadata.is_some() {
                state.selected_sheet_name = Some(vctx.virtual_sheet_name.clone());
                return true;
            }
        }
    }
    false
}

/// Restores original sheet selection after virtual override
pub fn restore_original_selection(
    state: &mut EditorWindowState,
    backup_sheet: Option<String>,
    used_virtual_override: bool,
) {
    if used_virtual_override {
        if let Some(vctx) = state.virtual_structure_stack.last() {
            if let Some(orig_sheet) = &backup_sheet {
                if *orig_sheet != vctx.virtual_sheet_name {
                    state.selected_sheet_name = backup_sheet;
                }
            }
        } else {
            // No virtual sheet active anymore, restore previous selection
            state.selected_sheet_name = backup_sheet;
        }
    }
}

/// Computes visible columns for the sheet, filtering out deleted columns
pub fn compute_visible_columns(
    state: &EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    metadata: &crate::sheets::definitions::SheetMetadata,
) -> Vec<usize> {
    let num_cols = metadata.columns.len();
    state
        .get_visible_column_indices(category, sheet_name, num_cols)
        .into_iter()
        .filter(|&i| {
            metadata
                .columns
                .get(i)
                .map(|c| !c.deleted)
                .unwrap_or(true)
        })
        .collect()
}

/// Builds table column definitions with appropriate widths
pub fn build_table_columns<'a>(
    mut table_builder: egui_extras::TableBuilder<'a>,
    state: &mut EditorWindowState,
    metadata: &crate::sheets::definitions::SheetMetadata,
    visible_columns: &[usize],
    prefix_count: usize,
    total_cols: usize,
) -> egui_extras::TableBuilder<'a> {
    use crate::sheets::systems::ui_handlers;
    use bevy_egui::egui;
    use egui_extras::Column;

    let num_visible_cols = visible_columns.len();

    if total_cols == 0 {
        if state.scroll_to_row_index.is_some() {
            state.scroll_to_row_index = None;
        }
        // Always add a fixed left control column
        table_builder = table_builder
            .column(Column::initial(26.0).at_least(26.0).resizable(false))
            .column(Column::remainder().resizable(false));
    } else {
        // Add fixed left control column for checkboxes/buttons
        table_builder =
            table_builder.column(Column::initial(26.0).at_least(26.0).resizable(false));

        // Build prefix (read-only key) columns next (if any)
        for _ in 0..prefix_count {
            table_builder = table_builder
                .column(Column::initial(110.0).at_least(60.0).resizable(true).clip(true));
        }

        // Build data columns with appropriate widths based on column type
        for vis_idx in 0..num_visible_cols {
            let col_index = visible_columns[vis_idx];
            let col_def = &metadata.columns[col_index];
            let (init_w, min_w) = ui_handlers::calculate_column_width(
                col_def.validator.as_ref(),
                col_def.data_type,
            );
            let col = Column::initial(init_w)
                .at_least(min_w)
                .resizable(true)
                .clip(true);
            table_builder = table_builder.column(col);
        }
        // Add a non-resizable remainder filler column to prevent the last data column from stretching
        table_builder = table_builder.column(Column::remainder().resizable(false));
    }

    table_builder
}

/// Renders the left control cell (checkbox or empty space)
pub fn render_control_cell(
    row: &mut egui_extras::TableRow,
    state: &EditorWindowState,
    original_row_index: usize,
    row_height: f32,
) {
    use crate::ui::elements::editor::state::{AiModeState, SheetInteractionState};
    use bevy_egui::egui;

    row.col(|ui| {
        let ai_preparing = state.current_interaction_mode == SheetInteractionState::AiModeActive
            && state.ai_mode == AiModeState::Preparing;

        if state.current_interaction_mode == SheetInteractionState::DeleteModeActive || ai_preparing
        {
            let mut is_selected = state.ai_selected_rows.contains(&original_row_index);
            let _response = ui.add(egui::Checkbox::without_text(&mut is_selected));
            // Note: We can't mutate state here since it's immutable
            // The checkbox reflects the current state but doesn't update it
        } else {
            ui.allocate_exact_size(egui::vec2(18.0, row_height), egui::Sense::hover());
        }
    });
}

/// Renders a single data cell
#[allow(clippy::too_many_arguments)]
pub fn render_data_cell(
    ui: &mut bevy_egui::egui::Ui,
    state: &mut EditorWindowState,
    registry: &crate::sheets::resources::SheetRegistry,
    render_cache: &crate::sheets::resources::SheetRenderCache,
    current_category: &Option<String>,
    selected_name: &str,
    original_row_index: usize,
    c_idx: usize,
    validators: &[Option<crate::sheets::definitions::ColumnValidator>],
    cell_update_writer: &mut bevy::ecs::event::EventWriter<crate::sheets::events::UpdateCellEvent>,
    open_structure_writer: &mut bevy::ecs::event::EventWriter<crate::sheets::events::OpenStructureViewEvent>,
    toggle_add_rows_writer: &mut bevy::ecs::event::EventWriter<crate::sheets::events::RequestToggleAiRowGeneration>,
    copy_writer: &mut bevy::ecs::event::EventWriter<crate::sheets::events::RequestCopyCell>,
    paste_writer: &mut bevy::ecs::event::EventWriter<crate::sheets::events::RequestPasteCell>,
    clipboard_buffer: &crate::sheets::resources::ClipboardBuffer,
) {
    use crate::sheets::events::UpdateCellEvent;
    use bevy_egui::egui;

    let validator_opt = validators.get(c_idx).cloned().flatten();
    let cell_id = egui::Id::new("cell")
        .with(current_category.as_deref().unwrap_or("root"))
        .with(selected_name)
        .with(original_row_index)
        .with(c_idx);

    if let Some(new_value) = crate::ui::common::edit_cell_widget(
        ui,
        cell_id,
        &validator_opt,
        current_category,
        selected_name,
        original_row_index,
        c_idx,
        registry,
        render_cache,
        state,
        open_structure_writer,
        toggle_add_rows_writer,
        copy_writer,
        paste_writer,
        clipboard_buffer,
    ) {
        cell_update_writer.write(UpdateCellEvent {
            category: current_category.clone(),
            sheet_name: selected_name.to_string(),
            row_index: original_row_index,
            col_index: c_idx,
            new_value,
        });
    }
}
