// src/ui/widgets/linked_column_cache.rs
use bevy::prelude::*;
use std::collections::HashSet;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Represents the result of trying to get or populate the cache.
pub(super) enum CacheResult<'a> {
    /// Successfully retrieved or populated cache, contains reference to allowed values.
    Success(&'a HashSet<String>),
    /// An error occurred during cache population (e.g., target not found).
    Error(String),
}

/// Gets the allowed values for a linked column, populating the cache if necessary.
///
/// # Arguments
///
/// * `target_sheet_name` - The name of the sheet the linked column points to.
/// * `target_column_index` - The index of the column within the target sheet.
/// * `registry` - Immutable reference to the `SheetRegistry`.
/// * `state` - Mutable reference to the `EditorWindowState` containing the cache.
///
/// # Returns
///
/// * `CacheResult::Success(&HashSet<String>)` - On success, containing a reference to the set of allowed values.
/// * `CacheResult::Error(String)` - On failure, containing an error message.
pub(super) fn get_or_populate_linked_options<'a>(
    target_sheet_name: &str,
    target_column_index: usize,
    registry: &SheetRegistry,
    state: &'a mut EditorWindowState, // Return lifetime tied to state
) -> CacheResult<'a> {
    let cache_key = (target_sheet_name.to_string(), target_column_index);

    // --- Check if cache needs population ---
    if !state.linked_column_cache.contains_key(&cache_key) {
        let mut error_msg: Option<String> = None;
        if let Some(target_sheet) = registry.get_sheet(target_sheet_name) {
            if let Some(meta) = &target_sheet.metadata {
                if target_column_index < meta.column_headers.len() {
                    // Collect unique, non-empty values directly into HashSet
                    let unique_values: HashSet<String> = target_sheet
                        .grid
                        .iter()
                        .filter_map(|row| row.get(target_column_index))
                        .filter(|cell| !cell.is_empty()) // Don't include empty strings as valid options
                        .cloned()
                        .collect();

                    state
                        .linked_column_cache
                        .insert(cache_key.clone(), unique_values); // Insert the HashSet
                    trace!(
                        "Cached linked options for ({}, {})",
                        target_sheet_name,
                        target_column_index
                    );
                } else {
                    error_msg = Some(format!(
                        "Target column index {} out of bounds for sheet '{}' ({} columns).",
                        target_column_index + 1, // User-facing index
                        target_sheet_name,
                        meta.column_headers.len()
                    ));
                }
            } else {
                error_msg = Some(format!(
                    "Target sheet '{}' is missing metadata.",
                    target_sheet_name
                ));
            }
        } else {
            error_msg = Some(format!("Target sheet '{}' not found.", target_sheet_name));
        }

        // Insert empty set if there was an error during generation to prevent repeated attempts
        if error_msg.is_some() {
            state
                .linked_column_cache
                .entry(cache_key.clone()) // Use entry API to avoid potential race condition if called concurrently (though unlikely here)
                .or_insert_with(HashSet::new);
            return CacheResult::Error(error_msg.unwrap()); // Return the error
        }
    }

    // --- Retrieve from cache ---
    // We use get again here, as the entry might have been inserted above.
    // The unwrap is safe because we ensured the key exists.
    if let Some(values) = state.linked_column_cache.get(&cache_key) {
        CacheResult::Success(values)
    } else {
        // This case should be unreachable due to the logic above.
        error!("Logic error: Cache key ({:?}) not found after population attempt.", cache_key);
        CacheResult::Error("Internal cache error".to_string())
    }
}
