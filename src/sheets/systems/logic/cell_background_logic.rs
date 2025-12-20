// src/sheets/systems/logic/cell_background_logic.rs
//! Cell background color determination logic.
//! Handles background coloring based on selection state, validation, and cell type.

use bevy_egui::egui::Color32;
use crate::sheets::definitions::ColumnDataType;
use crate::ui::elements::editor::state::SheetInteractionState;
use crate::ui::validation::ValidationState;

/// Determines the background color for a cell based on its state.
/// 
/// # Arguments
/// * `is_column_selected_for_deletion` - Whether the column is selected for deletion
/// * `is_row_selected` - Whether the row is selected
/// * `current_interaction_mode` - Current interaction mode (Delete, AI, etc.)
/// * `is_structure_column` - Whether this is a structure column
/// * `is_structure_ai_included` - Whether structure column is included in AI
/// * `is_column_ai_included` - Whether column is included in AI
/// * `effective_validation_state` - Current validation state
/// * `is_linked_column` - Whether this is a linked column
/// * `basic_type` - The basic data type of the column
pub fn determine_cell_background_color(
    is_column_selected_for_deletion: bool,
    is_row_selected: bool,
    current_interaction_mode: SheetInteractionState,
    is_structure_column: bool,
    is_structure_ai_included: bool,
    is_column_ai_included: bool,
    effective_validation_state: ValidationState,
    is_linked_column: bool,
    basic_type: ColumnDataType,
) -> Color32 {
    if is_column_selected_for_deletion {
        Color32::from_rgba_unmultiplied(120, 20, 20, 200)
    } else if is_row_selected && current_interaction_mode == SheetInteractionState::DeleteModeActive {
        Color32::from_rgba_unmultiplied(120, 20, 20, 200)
    } else if is_row_selected && current_interaction_mode == SheetInteractionState::AiModeActive {
        if (is_structure_column && !is_structure_ai_included)
            || (!is_structure_column && !is_column_ai_included)
        {
            match effective_validation_state {
                ValidationState::Empty => Color32::TRANSPARENT,
                ValidationState::Valid => Color32::TRANSPARENT,
                ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180),
            }
        } else {
            Color32::from_rgba_unmultiplied(20, 60, 120, 200)
        }
    } else {
        get_validation_based_background(effective_validation_state, is_linked_column, basic_type)
    }
}

/// Get background color based on validation state and cell type.
fn get_validation_based_background(
    validation_state: ValidationState,
    is_linked_column: bool,
    basic_type: ColumnDataType,
) -> Color32 {
    let dark_cell_fill = Color32::from_rgb(45, 45, 45);
    // darker fill for text cells
    let text_cell_fill = Color32::from_rgb(30, 30, 30);
    
    match validation_state {
        ValidationState::Empty => {
            match basic_type {
                ColumnDataType::String | ColumnDataType::Link => text_cell_fill,
                ColumnDataType::I64 | ColumnDataType::F64 => dark_cell_fill,
                _ => Color32::TRANSPARENT,
            }
        }
        ValidationState::Valid => {
            if is_linked_column
                || matches!(basic_type, ColumnDataType::Bool | ColumnDataType::I64 | ColumnDataType::F64)
            {
                // numeric and linked columns use standard dark fill
                dark_cell_fill
            } else if matches!(basic_type, ColumnDataType::String | ColumnDataType::Link) {
                // text and link columns darker fill
                text_cell_fill
            } else {
                Color32::TRANSPARENT
            }
        }
        ValidationState::Invalid => Color32::from_rgba_unmultiplied(80, 20, 20, 180),
    }
}
