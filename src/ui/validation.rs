// src/ui/validation.rs
// (Ensure ValidationState enum exists and is adjusted as discussed)
use bevy::prelude::*;
use std::collections::HashSet;

use crate::sheets::{
    definitions::ColumnDataType,
    resources::SheetRegistry,
};
// IMPORTANT: EditorWindowState is needed here ONLY for the linked cache access
// If we refactor cache access later, this dependency might be removed from validation itself.
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::widgets::linked_column_cache::{self, CacheResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ValidationState {
    #[default]
    Valid, // Default state, assumes valid until checked or known otherwise
    Empty, // Valid but empty (e.g., Option types)
    Invalid,
    // Consider adding 'Unchecked' state if lazy population is desired later
}

/// Validates a cell value based on a basic data type.
/// Returns the validation state and a boolean indicating if a parse error occurred.
pub(crate) fn validate_basic_cell(
    current_cell_string: &str,
    basic_type: ColumnDataType,
) -> (ValidationState, bool) { // Return type is ValidationState
    if current_cell_string.is_empty() {
        let state = if matches!(basic_type, ColumnDataType::String) {
            ValidationState::Empty // empty string allowed only for String columns
        } else {
            ValidationState::Invalid
        };
        return (state, false);
    }

    let mut parse_error = false;
    match basic_type {
    ColumnDataType::String => {}
        ColumnDataType::Bool => {
             if !matches!(current_cell_string.to_lowercase().as_str(), "true" | "1" | "false" | "0") { parse_error = true; }
        }
        ColumnDataType::I64 => { if current_cell_string.parse::<i64>().is_err() { parse_error = true; } },
        ColumnDataType::F64 => { if current_cell_string.parse::<f64>().is_err() { parse_error = true; } },
    }


    let state = if parse_error {
        ValidationState::Invalid
    } else {
        ValidationState::Valid // Non-empty and parsed correctly
    };
    (state, parse_error)
}

/// Validates a cell value based on a linked column validator.
/// Returns the validation state and optionally a reference to the allowed values from the cache.
pub(crate) fn validate_linked_cell<'a>(
    current_cell_string: &str,
    target_sheet_name: &str,
    target_column_index: usize,
    registry: &SheetRegistry,
    state: &'a mut EditorWindowState, // Still need EditorWindowState for the linked cache access
) -> (ValidationState, Option<&'a HashSet<String>>) {
    if current_cell_string.is_empty() {
        // Empty string is considered Valid (Empty state) for linked columns
        return (ValidationState::Empty, None);
    }

    // Access the linked column cache (which is still needed for the dropdown)
    match linked_column_cache::get_or_populate_linked_options(
        target_sheet_name,
        target_column_index,
        registry,
        state, // Pass mutable state for cache population
    ) {
        CacheResult::Success(allowed_values) => {
            if allowed_values.contains(current_cell_string) {
                (ValidationState::Valid, Some(allowed_values)) // Valid and exists in target
            } else {
                (ValidationState::Invalid, Some(allowed_values)) // Invalid (not in target set)
            }
        }
        CacheResult::Error(_) => {
            // If cache population failed (link is broken), mark as invalid
            (ValidationState::Invalid, None)
        }
    }
}