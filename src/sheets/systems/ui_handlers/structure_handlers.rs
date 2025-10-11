// src/sheets/systems/ui_handlers/structure_handlers.rs
//! Handlers for structure-related logic: context derivation, structure checkbox handling.

use crate::sheets::events::RequestUpdateAiStructureSend;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::log::warn;
use bevy::prelude::EventWriter;

/// Derive root context (category, sheet) and current structure path from the virtual stack.
/// Returns (root_category, root_sheet, structure_path).
pub fn derive_structure_context(
    state: &EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
) -> (Option<String>, String, Vec<usize>) {
    let structure_path: Vec<usize> = state
        .virtual_structure_stack
        .iter()
        .map(|ctx| ctx.parent.parent_col)
        .collect();

    let (root_category, root_sheet) = if let Some(first_ctx) = state.virtual_structure_stack.first()
    {
        (
            first_ctx.parent.parent_category.clone(),
            first_ctx.parent.parent_sheet.clone(),
        )
    } else {
        (category.clone(), sheet_name.to_string())
    };

    (root_category, root_sheet, structure_path)
}

/// Handle checkbox change for Structure validator columns.
pub fn handle_structure_checkbox_change(
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    c_idx: usize,
    is_included: bool,
    structure_send_writer: &mut EventWriter<RequestUpdateAiStructureSend>,
) {
    let mut structure_path: Vec<usize> = state
        .virtual_structure_stack
        .iter()
        .map(|ctx| ctx.parent.parent_col)
        .collect();

    let (root_category, root_sheet) = if let Some(first_ctx) = state.virtual_structure_stack.first()
    {
        (
            first_ctx.parent.parent_category.clone(),
            first_ctx.parent.parent_sheet.clone(),
        )
    } else {
        (category.clone(), sheet_name.to_string())
    };

    if root_sheet.is_empty() {
        warn!(
            "Skipping AI structure send update: missing root sheet for {:?}/{}",
            root_category, sheet_name
        );
        return;
    }

    structure_path.push(c_idx);
    structure_send_writer.write(RequestUpdateAiStructureSend {
        category: root_category.clone(),
        sheet_name: root_sheet.clone(),
        structure_path: structure_path.clone(),
        include: is_included,
    });
    state.mark_ai_included_columns_dirty();
}
