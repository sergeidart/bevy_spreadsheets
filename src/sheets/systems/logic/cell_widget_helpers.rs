// src/sheets/systems/logic/cell_widget_helpers.rs
//! Helper functions for cell widget logic and state management.
//! These functions compute structure navigation paths and resolve AI generation settings.

use crate::sheets::definitions::SheetMetadata;
use crate::ui::elements::editor::state::EditorWindowState;

/// Compute the root sheet and structure path for a given column in the current context.
/// Returns None if there's no valid structure path, otherwise returns
/// (root_category, root_sheet_name, path_of_column_indices).
pub fn compute_structure_root_and_path(
    state: &EditorWindowState,
    current_sheet_name: &str,
    col_index: usize,
) -> Option<(Option<String>, String, Vec<usize>)> {
    let mut path: Vec<usize> = state
        .virtual_structure_stack
        .iter()
        .map(|ctx| ctx.parent.parent_col)
        .collect();
    path.push(col_index);
    if path.is_empty() {
        return None;
    }
    let (root_category, root_sheet) = if let Some(first_ctx) = state.virtual_structure_stack.first()
    {
        (
            first_ctx.parent.parent_category.clone(),
            first_ctx.parent.parent_sheet.clone(),
        )
    } else {
        (
            state.selected_category.clone(),
            state
                .selected_sheet_name
                .clone()
                .unwrap_or_else(|| current_sheet_name.to_string()),
        )
    };
    Some((root_category, root_sheet, path))
}

/// Resolve the AI row generation override setting for a structure at the given path.
/// Returns None if there's no override (uses sheet default), or Some(bool) if explicitly set.
pub fn resolve_structure_override_for_menu(meta: &SheetMetadata, path: &[usize]) -> Option<bool> {
    if path.is_empty() {
        return None;
    }
    let column = meta.columns.get(path[0])?;
    if path.len() == 1 {
        return column.ai_enable_row_generation;
    }
    let mut field = column.structure_schema.as_ref()?.get(path[1])?;
    if path.len() == 2 {
        return field.ai_enable_row_generation;
    }
    for idx in path.iter().skip(2) {
        field = field.structure_schema.as_ref()?.get(*idx)?;
    }
    field.ai_enable_row_generation
}
