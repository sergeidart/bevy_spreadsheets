use crate::ui::elements::editor::state::{EditorWindowState, NewRowReview, ReviewChoice, RowReview, StructureDetailContext, StructureReviewEntry};
use bevy::prelude::*;

/// Persist the temporary structure row reviews (in structure detail mode) back into the corresponding StructureReviewEntry.
/// We only store merged decisions (Original vs AI per column) into the entry.merged_rows and flag acceptance/rejection on bulk actions.
pub fn persist_structure_detail_changes(
    state: &mut EditorWindowState,
    detail_ctx: &StructureDetailContext,
) {
    // Locate the matching structure review entry mutably
    if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
        match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
            (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
            (None, None) => {
                sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                    && sr.structure_path == detail_ctx.structure_path
            }
            _ => false,
        }
    }) {
        // Reconstruct merged_rows from current temp ai_row_reviews / ai_new_row_reviews.
        // In structure detail mode only ai_row_reviews are populated (existing rows) and ai_new_row_reviews for appended ones.
        // Convert RowReview -> merged row using choices (Original keeps original cell, AI picks ai cell).
        let mut merged_rows: Vec<Vec<String>> = Vec::new();
        let mut differences: Vec<Vec<bool>> = Vec::new();
        for rr in &state.ai_row_reviews {
            // We need the original and ai row complete; they are stored entirely (not only diffs) in RowReview
            let mut merged = rr.original.clone();
            let mut diff_flags = vec![false; merged.len()];
            for (pos, &col_idx) in rr.non_structure_columns.iter().enumerate() {
                if let Some(choice) = rr.choices.get(pos) {
                    // Skip parent_key column from being applied/considered as merged value
                    if col_idx == 1 {
                        continue;
                    }
                    // Defensive bounds check: some structure schemas may have fewer columns
                    if col_idx >= merged.len() || col_idx >= diff_flags.len() {
                        continue;
                    }
                    match choice {
                        ReviewChoice::AI => {
                            if let (Some(ai_val), Some(slot)) =
                                (rr.ai.get(pos), merged.get_mut(col_idx))
                            {
                                *slot = ai_val.clone();
                            }
                            diff_flags[col_idx] = true;
                        }
                        ReviewChoice::Original => {
                            // keep original; mark diff flag if original != ai for trace
                            if let (Some(orig_val), Some(ai_val)) =
                                (rr.original.get(pos), rr.ai.get(pos))
                            {
                                if orig_val != ai_val {
                                    diff_flags[col_idx] = true;
                                }
                            }
                        }
                    }
                }
            }
            // Collect row-level actions injected via temporary vectors on context (planned extension). For now detect button presses inline.
            // Pseudocode placeholder hooking into existing row render calls if they push events.
            merged_rows.push(merged);
            differences.push(diff_flags);
        }
        // Append new rows (always AI rows)
        for (nr_idx, nr) in state.ai_new_row_reviews.iter().enumerate() {
            // Build merged row directly from AI values (no column index mapping needed)
            // The AI row already has the correct structure for this schema
            let mut merged = nr.ai.clone();
            info!(
                "persist_structure_detail: adding new row {}: len={}, data={:?}",
                nr_idx,
                merged.len(),
                merged
            );
            // Treat parent_key as non-editable: ensure we don't override it here and only mark other cells as AI-sourced
            let mut diff_flags = vec![false; merged.len()];
            for (pos, &col_idx) in nr.non_structure_columns.iter().enumerate() {
                if col_idx == 1 {
                    // skip parent_key: keep merged value as-is (should be original or already set)
                    continue;
                }
                if col_idx >= merged.len() || col_idx >= diff_flags.len() {
                    continue;
                }
                if let (Some(ai_val), Some(slot)) = (nr.ai.get(pos), merged.get_mut(col_idx)) {
                    *slot = ai_val.clone();
                    diff_flags[col_idx] = true;
                }
            }
            merged_rows.push(merged);
            differences.push(diff_flags);
        }
        if !merged_rows.is_empty() {
            info!("persist_structure_detail: updating entry with {} merged_rows, schema_headers.len={}", merged_rows.len(), entry.schema_headers.len());
            entry.merged_rows = merged_rows;
            entry.differences = differences; // reflect user's latest decision set
            entry.has_changes = entry.differences.iter().flatten().any(|b| *b);
        }
    }
}

/// Apply accept/decline for a single existing structure row inside detail mode.
pub fn structure_row_apply_existing(
    entry: &mut StructureReviewEntry,
    rr: &RowReview,
    accept: bool,
) {
    let row_idx = rr.row_index;
    if row_idx >= entry.merged_rows.len() || row_idx >= entry.original_rows.len() {
        return;
    }
    if accept {
        // Merge based on choices
        for (pos, &col_idx) in rr.non_structure_columns.iter().enumerate() {
            if let Some(choice) = rr.choices.get(pos) {
                // Skip parent_key (col 1) when applying merges/accepts
                if col_idx == 1 {
                    continue;
                }
                if col_idx >= entry.merged_rows[row_idx].len() {
                    continue;
                }
                if matches!(choice, ReviewChoice::AI) {
                    if let (Some(ai_val), Some(slot)) = (rr.ai.get(pos), entry.merged_rows[row_idx].get_mut(col_idx)) {
                        *slot = ai_val.clone();
                    }
                } else {
                    // Original - ensure merged matches original
                    if let (Some(orig_val), Some(slot)) = (rr.original.get(pos), entry.merged_rows[row_idx].get_mut(col_idx)) {
                        *slot = orig_val.clone();
                    }
                }
            }
        }
    } else {
        // Decline -> reset merged to original row
        if row_idx < entry.original_rows.len() {
            entry.merged_rows[row_idx] = entry.original_rows[row_idx].clone();
        }
    }
    // Clear differences for this row (resolved)
    if row_idx < entry.differences.len() {
        for flag in &mut entry.differences[row_idx] {
            *flag = false;
        }
    }
}

/// Apply accept/decline for a single new structure row (AI added) inside detail mode.
pub fn structure_row_apply_new(
    entry: &mut StructureReviewEntry,
    new_row_local_idx: usize, // index within temp ai_new_row_reviews list
    ai_new_row_reviews: &[NewRowReview],
    accept: bool,
) {
    // New rows in entry start after original_rows.len()
    let base = entry.original_rows.len();
    let target_idx = base + new_row_local_idx;
    if target_idx >= entry.merged_rows.len() {
        return;
    }
    if accept {
        if let Some(nr) = ai_new_row_reviews.get(new_row_local_idx) {
            // Build merged row from AI values
            let mut merged = entry.merged_rows[target_idx].clone();
            if merged.len() < nr.ai.len() {
                merged.resize(nr.ai.len(), String::new());
            }
            for (pos, &col_idx) in nr.non_structure_columns.iter().enumerate() {
                // Skip parent_key column (non-editable)
                if col_idx == 1 {
                    continue;
                }
                if col_idx >= merged.len() {
                    continue;
                }
                if let (Some(ai_val), Some(slot)) = (nr.ai.get(pos), merged.get_mut(col_idx)) {
                    *slot = ai_val.clone();
                }
            }
            entry.merged_rows[target_idx] = merged;
        }
    } else {
        // Decline new row: remove it entirely from structure suggestion arrays
        entry.ai_rows.remove(target_idx);
        entry.merged_rows.remove(target_idx);
        entry.differences.remove(target_idx);
        // Also need to adjust any placeholders: original_rows has no entry for new rows so nothing to remove there.
        return; // early return so we don't try to clear diff flags (already removed)
    }
    if target_idx < entry.differences.len() {
        for flag in &mut entry.differences[target_idx] {
            *flag = false;
        }
    }
}
