// src/sheets/systems/ai/results/duplicate_detection.rs
// Duplicate row detection logic for AI batch results

use std::collections::HashMap;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::ReviewChoice;

use crate::sheets::systems::ai::row_helpers::{
    extract_original_snapshot_for_merge, generate_review_choices, normalize_cell_value,
};

/// Check if a new row is a duplicate of an existing row
/// Returns (duplicate_row_index, choices, original_for_merge, merge_selected)
pub fn check_for_duplicate(
    ai_snapshot: &[String],
    first_col_value_to_row: &HashMap<String, usize>,
    included: &[usize],
    key_actual_col_opt: Option<usize>,
    cat_ctx: &Option<String>,
    sheet_ctx: &Option<String>,
    registry: &SheetRegistry,
) -> (
    Option<usize>,
    Option<Vec<ReviewChoice>>,
    Option<Vec<String>>,
    bool,
) {
    // Determine the ai_snapshot index corresponding to the chosen actual column
    let ai_index = if let Some(actual_col) = key_actual_col_opt {
        // Map actual_col to position within included[]
        included.iter().position(|&c| c == actual_col).unwrap_or(0)
    } else {
        0
    };

    let Some(first_val) = ai_snapshot.get(ai_index) else {
        return (None, None, None, false);
    };

    let normalized_first = normalize_cell_value(first_val);
    let Some(&matched_row_index) = first_col_value_to_row.get(&normalized_first) else {
        return (None, None, None, false);
    };

    let Some(sheet_name) = sheet_ctx else {
        return (Some(matched_row_index), None, None, false);
    };

    let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_name) else {
        return (Some(matched_row_index), None, None, false);
    };

    let Some(existing_row) = sheet_ref.grid.get(matched_row_index) else {
        return (Some(matched_row_index), None, None, false);
    };

    let orig_vec = extract_original_snapshot_for_merge(existing_row, included);
    // When generating choices, align the ai_snapshot with included[]; ai_snapshot is already in included order
    let choices = generate_review_choices(&orig_vec, ai_snapshot);

    (Some(matched_row_index), Some(choices), Some(orig_vec), true)
}
