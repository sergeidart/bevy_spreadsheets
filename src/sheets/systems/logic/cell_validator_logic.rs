// src/sheets/systems/logic/cell_validator_logic.rs
//! Cell validation state determination logic.
//! Handles validation state computation for linked columns and other validators.

use std::collections::HashSet;
use std::sync::Arc;
use crate::sheets::definitions::ColumnValidator;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::validation::{normalize_for_link_cmp, ValidationState};
use crate::ui::widgets::linked_column_cache::{self, CacheResult};

/// Result of prefetching linked column values.
pub struct LinkedColumnPrefetch {
    pub raw_values: Option<Arc<HashSet<String>>>,
    pub normalized_values: Option<Arc<HashSet<String>>>,
}

/// Prefetch allowed values for linked columns.
/// Returns both raw and normalized value sets for validation.
pub fn prefetch_linked_column_values(
    validator_opt: &Option<ColumnValidator>,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
) -> LinkedColumnPrefetch {
    let mut raw_values = None;
    let mut normalized_values = None;

    if let Some(ColumnValidator::Linked {
        target_sheet_name,
        target_column_index,
    }) = validator_opt
    {
        if let CacheResult::Success {
            raw: values,
            normalized,
        } = linked_column_cache::get_or_populate_linked_options(
            target_sheet_name,
            *target_column_index,
            registry,
            state,
        ) {
            raw_values = Some(values);
            normalized_values = Some(normalized);
        }
    }

    LinkedColumnPrefetch {
        raw_values,
        normalized_values,
    }
}

/// Determine the effective validation state for a cell.
/// Prefers fresh linked column validation over cached validation.
pub fn determine_effective_validation_state(
    current_display_text: &str,
    normalized_values: &Option<Arc<HashSet<String>>>,
    cached_validation_state: ValidationState,
) -> ValidationState {
    if let Some(values_norm) = normalized_values.as_ref() {
        if current_display_text.is_empty() {
            ValidationState::Empty
        } else {
            let needle = normalize_for_link_cmp(current_display_text);
            let exists = values_norm.contains(&needle);
            if exists {
                ValidationState::Valid
            } else {
                ValidationState::Invalid
            }
        }
    } else {
        cached_validation_state
    }
}

/// Check if a column is included in AI generation based on cached state.
pub fn is_column_ai_included(
    state: &EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    col_index: usize,
) -> bool {
    if state.ai_cached_included_columns_valid
        && state.ai_cached_included_columns_sheet.as_deref() == Some(sheet_name)
        && state.ai_cached_included_columns_category.as_ref() == category.as_ref()
    {
        state
            .ai_cached_included_columns
            .get(col_index)
            .copied()
            .unwrap_or(false)
    } else {
        false
    }
}

/// Check if a structure column is included in AI sends.
pub fn is_structure_column_ai_included(
    state: &EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    col_index: usize,
    is_structure_column: bool,
) -> bool {
    if is_structure_column
        && state.ai_cached_included_columns_valid
        && state.ai_cached_included_columns_sheet.as_deref() == Some(sheet_name)
        && state.ai_cached_included_columns_category.as_ref() == category.as_ref()
    {
        state
            .ai_cached_included_structure_columns
            .get(col_index)
            .copied()
            .unwrap_or(false)
    } else {
        false
    }
}
