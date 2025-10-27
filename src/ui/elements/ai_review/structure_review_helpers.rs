// src/ui/elements/ai_review/structure_review_helpers.rs
// Helper functions for structure review UI conversion and processing

use crate::sheets::definitions::{ColumnValidator, StructureFieldDefinition};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::{
    EditorWindowState, NewRowReview, ReviewChoice, RowReview, StructureReviewEntry,
};

/// Convert a StructureReviewEntry into temporary RowReview and NewRowReview entries
pub fn convert_structure_to_reviews(
    entry: &StructureReviewEntry,
) -> (Vec<RowReview>, Vec<NewRowReview>) {
    let mut row_reviews = Vec::new();
    let mut new_row_reviews = Vec::new();

    // Determine non-structure columns by analyzing the structure (assume all for now)
    let num_cols = entry
        .original_rows
        .first()
        .map(|r| r.len())
        .unwrap_or_else(|| entry.ai_rows.first().map(|r| r.len()).unwrap_or(0));
    let non_structure_columns: Vec<usize> = (0..num_cols).collect();

    // Build RowReview entries for ORIGINAL rows only (up to original_rows_count)
    // AI-added rows (beyond original_rows_count) should be treated as new rows
    for row_idx in 0..entry.original_rows_count.min(entry.ai_rows.len()) {
        let original_row = &entry.original_rows[row_idx];
        let ai_row = &entry.ai_rows[row_idx];
        let row_diffs = entry.differences.get(row_idx);

        let mut choices = Vec::new();
        for col_idx in 0..num_cols {
            let has_diff = row_diffs
                .and_then(|d| d.get(col_idx))
                .copied()
                .unwrap_or(false);
            // Treat parent_key (col_idx == 1) as non-editable and not part of merge decisions.
            // Default its choice to Original so it won't be considered for merges/edits.
            if col_idx == 1 {
                choices.push(ReviewChoice::Original);
            } else {
                // If there's a difference, default to AI (we're reviewing AI suggestions)
                // If no difference (original == ai), default to Original
                choices.push(if has_diff { ReviewChoice::AI } else { ReviewChoice::Original });
            }
        }

        row_reviews.push(RowReview {
            row_index: row_idx,
            non_structure_columns: non_structure_columns.clone(),
            original: original_row.clone(),
            ai: ai_row.clone(),
            choices,
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
        });
    }

    // Build NewRowReview entries for AI-added rows (beyond original_rows_count)
    // Perform duplicate detection to enable merge suggestions (similar to base-level)
    for row_idx in entry.original_rows_count..entry.ai_rows.len() {
        let ai_row = &entry.ai_rows[row_idx];

        // Detect potential duplicates by comparing a key column value that is NOT the technical parent_key
        let duplicate_match_row = if !ai_row.is_empty() && !entry.original_rows.is_empty() {
            // prefer a column other than parent_key (index 1); otherwise fallback to 0
            let key_idx = (0..ai_row.len()).find(|&i| i != 1).unwrap_or(0);
            let ai_key_val = ai_row.get(key_idx).map(|s| s.trim().to_lowercase()).unwrap_or_default();
            if !ai_key_val.is_empty() {
                entry.original_rows.iter().position(|orig_row| {
                    let orig_key_val = orig_row.get(key_idx).map(|s| s.trim().to_lowercase()).unwrap_or_default();
                    !orig_key_val.is_empty() && orig_key_val == ai_key_val
                })
            } else {
                None
            }
        } else {
            None
        };

        // If we found a match, prepare merge data
        let (original_for_merge, choices) = if let Some(matched_idx) = duplicate_match_row {
            let original_row = &entry.original_rows[matched_idx];
            let mut merge_choices = Vec::new();
            for col_idx in 0..num_cols {
                // Treat parent_key (column index 1) as non-editable and not part of merge decisions.
                if col_idx == 1 {
                    merge_choices.push(ReviewChoice::Original);
                    continue;
                }
                let orig_val = original_row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                let ai_val = ai_row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                // If values differ, default to AI suggestion
                merge_choices.push(if orig_val != ai_val {
                    ReviewChoice::AI
                } else {
                    ReviewChoice::Original
                });
            }
            (Some(original_row.clone()), Some(merge_choices))
        } else {
            (None, None)
        };

        new_row_reviews.push(NewRowReview {
            non_structure_columns: non_structure_columns.clone(),
            ai: ai_row.clone(),
            duplicate_match_row,
            original_for_merge,
            choices,
            merge_selected: duplicate_match_row.is_some(), // Default to merge if duplicate found
            merge_decided: false,
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
        });
    }

    (row_reviews, new_row_reviews)
}

/// Build column list from structure schema
/// If in_virtual_structure_review is true, filters out structure columns (nested structures)
pub fn build_structure_columns(
    union_cols: &[usize],
    detail_ctx: &Option<crate::ui::elements::editor::state::StructureDetailContext>,
    in_virtual_structure_review: bool,
    virtual_sheet_name: &str,
    _selected_category: &Option<String>,
    registry: &SheetRegistry,
) -> (Vec<ColumnEntry>, Vec<StructureFieldDefinition>) {
    // If in virtual structure review mode, filter out structure columns
    if in_virtual_structure_review {
        let mut result = Vec::new();
        // Find the virtual sheet to get its metadata
        if let Some(sheet) = registry
            .iter_sheets()
            .find(|(_, name, _)| *name == virtual_sheet_name)
            .and_then(|(_, _, sheet)| Some(sheet))
        {
            if let Some(meta) = &sheet.metadata {
                for &col_idx in union_cols {
                    if let Some(col_def) = meta.columns.get(col_idx) {
                        let is_structure =
                            matches!(col_def.validator, Some(ColumnValidator::Structure));
                        let is_included = !matches!(col_def.ai_include_in_send, Some(false));

                        // EXCLUDE structure columns in virtual structure review
                        // (they are nested structures and shouldn't be navigable)
                        // Also exclude columns not included by schema groups
                        if !is_structure && is_included {
                            result.push(ColumnEntry::Regular(col_idx));
                        }
                    }
                }
            }
        }
        return (result, Vec::new());
    }

    let detail_ctx = match detail_ctx {
        Some(ctx) => ctx,
        None => return (Vec::new(), Vec::new()),
    };

    // Get the structure schema from the CORRECT sheet using root_category and root_sheet
    // This prevents schema pollution from other sheets with structures at the same column index
    let structure_entry = registry
        .get_sheet(&detail_ctx.root_category, &detail_ctx.root_sheet)
        .and_then(|sheet| sheet.metadata.as_ref())
        .and_then(|meta| {
            if let Some(&first_col_idx) = detail_ctx.structure_path.first() {
                meta.columns.get(first_col_idx).and_then(|col| {
                    col.structure_schema
                        .as_ref()
                        .map(|schema| (col, schema.clone()))
                })
            } else {
                None
            }
        });

    let mut current_schema = match structure_entry {
        Some((_col_def, schema)) => schema,
        None => return (Vec::new(), Vec::new()),
    };

    // Navigate through nested structures
    for &nested_idx in detail_ctx.structure_path.iter().skip(1) {
        let temp_schema = current_schema.clone();
        if let Some(field) = temp_schema.get(nested_idx) {
            if let Some(nested_schema) = &field.structure_schema {
                current_schema = nested_schema.clone();
            }
        }
    }

    // Build column entries from structure schema
    let mut result = Vec::new();
    for (col_idx, field_def) in current_schema.iter().enumerate() {
        let is_structure = matches!(field_def.validator, Some(ColumnValidator::Structure));
        let is_included = !matches!(field_def.ai_include_in_send, Some(false));

        // Only show columns that are included (respecting schema groups)
        if !is_included {
            continue;
        }

        // Skip column 0 (id) in structure sheets - it's internal and should remain hidden
        if col_idx == 0 {
            continue;
        }

        if is_structure {
            result.push(ColumnEntry::Structure(col_idx));
        } else {
            // Show ALL regular columns from the structure schema, not just those in union_cols
            // This is necessary because linked columns might not have AI-generated data initially
            // but still need to be displayed for user input
            // Note: Column 1 (parent_key) will be shown as green non-editable in the renderer
            result.push(ColumnEntry::Regular(col_idx));
        }
    }

    (result, current_schema)
}

/// Build ancestor key columns for structure detail view
/// Gets keys from the structure schema and parent row data (from grid or reviews)
///
/// Note: This is a simplified implementation that returns an empty vector.
/// Full implementation would require parsing structure JSON and extracting key values.
pub fn build_structure_ancestor_keys(
    _detail_ctx: &crate::ui::elements::editor::state::StructureDetailContext,
    _state: &EditorWindowState,
    _selected_category: &Option<String>,
    _registry: &SheetRegistry,
    _saved_row_reviews: &[RowReview],
    _saved_new_row_reviews: &[NewRowReview],
) -> Vec<(String, String)> {
    // TODO: Implement full ancestor key extraction
    // This requires:
    // 1. Finding the structure entry for the given path
    // 2. Parsing structure JSON from parent rows
    // 3. Extracting key column values
    Vec::new()
}
