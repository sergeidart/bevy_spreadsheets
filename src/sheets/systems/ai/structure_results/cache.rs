// src/sheets/systems/ai/structure_results/cache.rs
// Functions for managing cache operations for structure reviews

use crate::ui::elements::editor::state::EditorWindowState;

/// Populate cache for structure parent row so previews work
/// Cache key format: (Some(parent_row_index), None) for existing rows
/// or (None, Some(new_row_index)) for new rows
pub fn populate_parent_row_cache(
    state: &mut EditorWindowState,
    parent_row_index: usize,
    parent_new_row_index: Option<usize>,
    parent_row: Vec<String>,
) {
    let cache_key = if let Some(new_idx) = parent_new_row_index {
        (None, Some(new_idx))
    } else {
        (Some(parent_row_index), None)
    };

    // Store the full parent row in cache if not already present
    if !state
        .ai_original_row_snapshot_cache
        .contains_key(&cache_key)
    {
        state
            .ai_original_row_snapshot_cache
            .insert(cache_key, parent_row);
    }
}
