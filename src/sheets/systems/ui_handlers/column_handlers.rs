// src/sheets/systems/ui_handlers/column_handlers.rs
//! Handlers for column-related logic: width calculation, ancestor keys, etc.

use crate::sheets::definitions::{ColumnDataType, ColumnValidator};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Calculate appropriate column width based on validator and data type.
/// This fixes the bug where child sheets would inherit parent column sizes incorrectly.
pub fn calculate_column_width(
    validator: Option<&ColumnValidator>,
    data_type: ColumnDataType,
) -> (f32, f32) {
    match (validator, data_type) {
        // Bool: smaller, since only a checkbox is shown
        (_, ColumnDataType::Bool) => (48.0, 36.0),
        // Linked text columns: allow wider default
        (Some(ColumnValidator::Linked { .. }), _) => (140.0, 60.0),
        // Structure columns render as a button with text; give them wider room
        (Some(ColumnValidator::Structure), _) => (140.0, 60.0),
        // Numbers: keep compact
        (_, ColumnDataType::I64) => (70.0, 36.0),
        (_, ColumnDataType::F64) => (78.0, 36.0),
        // Default/text: a bit wider than base to improve readability
        _ => (120.0, 48.0),
    }
}

/// Build ancestor key columns for virtual structure sheets.
/// Returns a vector of (header_text, value_text) tuples.
pub fn build_ancestor_key_columns(
    state: &EditorWindowState,
    registry: &SheetRegistry,
    selected_name: &str,
) -> Vec<(String, String)> {
    let mut ancestor_key_columns: Vec<(String, String)> = Vec::new();
    
    if let Some(last_ctx) = state.virtual_structure_stack.last() {
        if last_ctx.virtual_sheet_name == selected_name {
            // Iterate through stack in order (oldest -> newest)
            for vctx in &state.virtual_structure_stack {
                if let Some(parent_sheet) = registry
                    .get_sheet(&state.selected_category, &vctx.parent.parent_sheet)
                {
                    if let (Some(parent_meta), Some(parent_row)) = (
                        &parent_sheet.metadata,
                        parent_sheet.grid.get(vctx.parent.parent_row),
                    ) {
                        if let Some(struct_col_def) =
                            parent_meta.columns.get(vctx.parent.parent_col)
                        {
                            // Prefer parent-selected key; fallback to first non-structure
                            if let Some(key_col_idx) =
                                struct_col_def.structure_key_parent_column_index
                            {
                                if let Some(key_col_def) =
                                    parent_meta.columns.get(key_col_idx)
                                {
                                    let value = parent_row
                                        .get(key_col_idx)
                                        .cloned()
                                        .unwrap_or_default();
                                    ancestor_key_columns
                                        .push((key_col_def.header.clone(), value));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    ancestor_key_columns
}
