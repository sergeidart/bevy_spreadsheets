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
    _state: &EditorWindowState,
    _registry: &SheetRegistry,
    _selected_name: &str,
) -> Vec<(String, String)> {
    // Virtual structures deprecated; return empty vector
    Vec::new()
}
