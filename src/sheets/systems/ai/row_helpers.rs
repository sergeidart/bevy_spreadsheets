// src/sheets/systems/ai/row_helpers.rs
// Helper functions for processing AI result rows, handling key prefixes, snapshots, and review choices

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::ReviewChoice;

/// Skip key prefix from a full row, returning the data portion
pub fn skip_key_prefix<'a>(row_full: &'a [String], key_prefix_count: usize) -> &'a [String] {
    if key_prefix_count > 0 && row_full.len() >= key_prefix_count {
        &row_full[key_prefix_count..]
    } else {
        row_full
    }
}

/// Create original and AI snapshots for a single row based on included columns
pub fn create_row_snapshots(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
    ai_suggestion: &[String],
    included_cols: &[usize],
) -> (Vec<String>, Vec<String>) {
    let mut original_snapshot: Vec<String> = Vec::with_capacity(included_cols.len());
    let mut ai_snapshot: Vec<String> = Vec::with_capacity(included_cols.len());

    let original_row_opt = registry
        .get_sheet(category, sheet_name)
        .and_then(|sheet| sheet.grid.get(row_index).cloned());

    for (logical_i, actual_col) in included_cols.iter().enumerate() {
        let orig_val = original_row_opt
            .as_ref()
            .and_then(|r| r.get(*actual_col))
            .cloned()
            .unwrap_or_default();
        original_snapshot.push(orig_val);

        let ai_val = ai_suggestion.get(logical_i).cloned().unwrap_or_default();
        ai_snapshot.push(ai_val);
    }

    (original_snapshot, ai_snapshot)
}

/// Generate review choices based on differences between original and AI snapshots
pub fn generate_review_choices(
    original_snapshot: &[String],
    ai_snapshot: &[String],
) -> Vec<ReviewChoice> {
    original_snapshot
        .iter()
        .zip(ai_snapshot.iter())
        .map(|(orig, ai)| {
            if orig != ai {
                ReviewChoice::AI
            } else {
                ReviewChoice::Original
            }
        })
        .collect()
}

/// Normalize a cell value for duplicate detection (remove whitespace, lowercase)
pub fn normalize_cell_value(value: &str) -> String {
    value.replace(['\r', '\n'], "").trim().to_lowercase()
}

/// Extract AI snapshot from a new row suggestion
pub fn extract_ai_snapshot_from_new_row(
    new_row: &[String],
    included_cols: &[usize],
) -> Vec<String> {
    let mut ai_snapshot: Vec<String> = Vec::with_capacity(included_cols.len());
    for (logical_i, _actual_col) in included_cols.iter().enumerate() {
        ai_snapshot.push(new_row.get(logical_i).cloned().unwrap_or_default());
    }
    ai_snapshot
}

/// Create original snapshot from an existing row for merge comparison
pub fn extract_original_snapshot_for_merge(
    existing_row: &[String],
    included_cols: &[usize],
) -> Vec<String> {
    let mut orig_vec: Vec<String> = Vec::with_capacity(included_cols.len());
    for actual_col in included_cols {
        orig_vec.push(existing_row.get(*actual_col).cloned().unwrap_or_default());
    }
    orig_vec
}
