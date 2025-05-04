// src/ui/validation.rs
// NEW FILE
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashSet;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::widgets::linked_column_cache::{self, CacheResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationState {
    Empty,
    Valid,
    Invalid,
}

/// Validates a cell value based on a basic data type.
/// Returns the validation state and a boolean indicating if a parse error occurred.
pub(crate) fn validate_basic_cell(
    current_cell_string: &str,
    basic_type: ColumnDataType,
) -> (ValidationState, bool) {
    if current_cell_string.is_empty() {
        return (ValidationState::Empty, false);
    }

    let mut parse_error = false;
    match basic_type {
        ColumnDataType::String | ColumnDataType::OptionString => {} // No parsing needed/possible for base string
        ColumnDataType::Bool | ColumnDataType::OptionBool => {
            if !matches!(
                current_cell_string.to_lowercase().as_str(),
                "true" | "1" | "false" | "0" | "" // Allow empty for OptionBool
            ) {
                parse_error = true;
            }
        }
        ColumnDataType::U8 | ColumnDataType::OptionU8 => {
            if current_cell_string.parse::<u8>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::U16 | ColumnDataType::OptionU16 => {
            if current_cell_string.parse::<u16>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::U32 | ColumnDataType::OptionU32 => {
            if current_cell_string.parse::<u32>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::U64 | ColumnDataType::OptionU64 => {
            if current_cell_string.parse::<u64>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::I8 | ColumnDataType::OptionI8 => {
            if current_cell_string.parse::<i8>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::I16 | ColumnDataType::OptionI16 => {
            if current_cell_string.parse::<i16>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::I32 | ColumnDataType::OptionI32 => {
            if current_cell_string.parse::<i32>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::I64 | ColumnDataType::OptionI64 => {
            if current_cell_string.parse::<i64>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::F32 | ColumnDataType::OptionF32 => {
            if current_cell_string.parse::<f32>().is_err() {
                parse_error = true;
            }
        }
        ColumnDataType::F64 | ColumnDataType::OptionF64 => {
            if current_cell_string.parse::<f64>().is_err() {
                parse_error = true;
            }
        }
    }

    let state = if parse_error {
        ValidationState::Invalid
    } else {
        ValidationState::Valid
    };
    (state, parse_error)
}

/// Validates a cell value based on a linked column validator.
/// Returns the validation state and optionally a reference to the allowed values from the cache.
/// Note: The lifetime 'a depends on the lifetime of the 'state' argument.
pub(crate) fn validate_linked_cell<'a>(
    current_cell_string: &str,
    target_sheet_name: &str,
    target_column_index: usize,
    registry: &SheetRegistry,
    state: &'a mut EditorWindowState,
) -> (ValidationState, Option<&'a HashSet<String>>) {
    if current_cell_string.is_empty() {
        // Empty string is considered Valid for linked columns (represents 'None' or unlinked)
        // The dropdown will still show options.
        return (ValidationState::Valid, None);
    }

    match linked_column_cache::get_or_populate_linked_options(
        target_sheet_name,
        target_column_index,
        registry,
        state,
    ) {
        CacheResult::Success(allowed_values) => {
            // Check if the current string is in the allowed set
            if allowed_values.contains(current_cell_string) {
                (ValidationState::Valid, Some(allowed_values))
            } else {
                // Value exists but is not in the allowed set
                (ValidationState::Invalid, Some(allowed_values))
            }
        }
        CacheResult::Error(_) => {
            // If cache population failed (e.g., target invalid), mark as invalid
            // Pass an empty set reference to the widget later if needed
            (ValidationState::Invalid, None)
        }
    }
}