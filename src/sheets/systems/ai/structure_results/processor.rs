// src/sheets/systems/ai/structure_results/processor.rs
// Functions for processing structure suggestion rows

use bevy::prelude::*;

use crate::sheets::systems::ai::row_helpers::skip_key_prefix;

/// Process a single structure suggestion row, returning (ai_row, merged_row, diff_row, has_changes)
pub fn process_structure_suggestion_row(
    suggestion_full: Option<&Vec<String>>,
    base_row: &[String],
    included: &[usize],
    schema_len: usize,
    key_prefix_count: usize,
    parent_row_index: usize,
    local_idx: usize,
) -> Option<(Vec<String>, Vec<String>, Vec<bool>, bool)> {
    let suggestion_full = suggestion_full?;
    let suggestion = skip_key_prefix(suggestion_full, key_prefix_count);

    if suggestion.len() < included.len() {
        warn!(
            "Skipping malformed structure suggestion row parent={} local_idx={} suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
            parent_row_index,
            local_idx,
            suggestion.len(),
            included.len(),
            suggestion_full.len(),
            key_prefix_count
        );
        return None;
    }

    let mut ai_row = base_row.to_vec();
    let mut merged_row = base_row.to_vec();
    let mut diff_row = vec![false; schema_len];
    let mut has_changes = false;

    for (logical_i, col_index) in included.iter().enumerate() {
        let ai_value = suggestion.get(logical_i).cloned().unwrap_or_default();
        let orig_value = base_row.get(*col_index).cloned().unwrap_or_default();

        if ai_value != orig_value {
            diff_row[*col_index] = true;
            has_changes = true;
        }

        if let Some(slot) = ai_row.get_mut(*col_index) {
            *slot = ai_value.clone();
        }

        if diff_row[*col_index] {
            if let Some(slot) = merged_row.get_mut(*col_index) {
                *slot = ai_value;
            }
        }
    }

    Some((ai_row, merged_row, diff_row, has_changes))
}
