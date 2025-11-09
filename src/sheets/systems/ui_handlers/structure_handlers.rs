// src/sheets/systems/ui_handlers/structure_handlers.rs
//! Handlers for structure-related logic: context derivation, structure checkbox handling.

use crate::sheets::events::RequestUpdateAiStructureSend;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::log::warn;
use bevy::prelude::EventWriter;

/// Derive root context (category, sheet) and current structure path from the virtual stack.
/// Returns (root_category, root_sheet, structure_path).
pub fn derive_structure_context(
    _state: &EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
) -> (Option<String>, String, Vec<usize>) {
    // Virtual structures deprecated; return current sheet as root
    let structure_path: Vec<usize> = Vec::new();
    let (root_category, root_sheet) = (category.clone(), sheet_name.to_string());
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
    // Virtual structures deprecated; use current sheet as root
    let mut structure_path: Vec<usize> = Vec::new();
    let (root_category, root_sheet) = (category.clone(), sheet_name.to_string());

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
