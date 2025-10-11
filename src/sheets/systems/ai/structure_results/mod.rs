// src/sheets/systems/ai/structure_results/mod.rs
// Main assembly functions for processing structure batch results
// Split into submodules: builder, cache, processor, schema

mod builder;
mod cache;
mod processor;
mod schema;

use bevy::prelude::*;

use crate::sheets::definitions::{SheetGridData, StructureFieldDefinition};
use crate::sheets::events::SheetOperationFeedback;
use crate::ui::elements::editor::state::{
    EditorWindowState, StructureReviewEntry,
};

use super::row_helpers::skip_key_prefix;
use super::utils::parse_structure_rows_from_cell;

// Re-export submodule functions
use builder::build_parent_row;
use cache::populate_parent_row_cache;
use processor::process_structure_suggestion_row;
use schema::extract_original_nested_structure_rows;

/// Process a single parent row's structure partition results
pub fn process_structure_partition(
    parent_row_index: usize,
    partition_rows: &[Vec<String>],
    original_count: usize,
    root_category: &Option<String>,
    root_sheet: &str,
    structure_path: &[usize],
    column_index: usize,
    schema_fields: &[StructureFieldDefinition],
    schema_len: usize,
    included: &[usize],
    key_prefix_count: usize,
    sheet: &SheetGridData,
    state: &mut EditorWindowState,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    let new_row_context = state
        .ai_structure_new_row_contexts
        .get(&parent_row_index)
        .cloned();
    let parent_new_row_index = new_row_context.as_ref().map(|ctx| ctx.new_row_index);

    // For merge rows, look up the matched existing row for structure data
    let duplicate_match_row = if let Some(ref ctx) = new_row_context {
        state
            .ai_new_row_reviews
            .get(ctx.new_row_index)
            .and_then(|nr| nr.duplicate_match_row)
    } else {
        None
    };

    let parent_row = build_parent_row(
        &new_row_context,
        sheet,
        parent_row_index,
        schema_fields.len(),
    );

    // Fix: For merge rows, use the matched existing row's structure data
    // For new rows, use the parent_row which now includes structure data from full_ai_row
    let cell_value = if let Some(matched_idx) = duplicate_match_row {
        sheet
            .grid
            .get(matched_idx)
            .and_then(|row| row.get(column_index))
            .cloned()
            .unwrap_or_default()
    } else {
        // For both existing rows AND new rows, use parent_row
        // (new rows now have structure data from full_ai_row via build_parent_row)
        parent_row.get(column_index).cloned().unwrap_or_default()
    };

    // --- Build original rows ---
    // For top-level structures we can parse the JSON cell directly.
    // For nested structures (structure_path len > 1) we need to traverse into the nested arrays.
    let mut original_rows = if structure_path.len() == 1 {
        let mut rows = parse_structure_rows_from_cell(&cell_value, schema_fields);
        if rows.is_empty() {
            if !cell_value.is_empty() && cell_value != "[]" {
                warn!(
                    "Structure parse returned empty rows for non-empty cell (parent_row={}, cell_len={}): {:?}",
                    parent_row_index,
                    cell_value.len(),
                    &cell_value.chars().take(100).collect::<String>()
                );
            }
            rows.push(vec![String::new(); schema_len]);
        }
        rows
    } else {
        extract_original_nested_structure_rows(
            &cell_value,
            structure_path,
            sheet,
            schema_fields,
            schema_len,
        )
    };

    // Normalize row lengths
    for row in &mut original_rows {
        if row.len() < schema_len {
            row.resize(schema_len, String::new());
        }
    }

    let mut original_rows_aligned = original_rows.clone();
    if original_rows_aligned.len() < original_count {
        original_rows_aligned.resize(original_count, vec![String::new(); schema_len]);
    }

    let mut ai_rows: Vec<Vec<String>> = Vec::new();
    let mut merged_rows: Vec<Vec<String>> = Vec::new();
    let mut differences: Vec<Vec<bool>> = Vec::new();
    let mut has_changes = false;

    // Process original rows
    for local_idx in 0..original_count {
        if let Some((ai_row, merged_row, diff_row, changed)) = process_structure_suggestion_row(
            partition_rows.get(local_idx),
            &original_rows_aligned
                .get(local_idx)
                .cloned()
                .unwrap_or_else(|| vec![String::new(); schema_len]),
            included,
            schema_len,
            key_prefix_count,
            parent_row_index,
            local_idx,
        ) {
            ai_rows.push(ai_row);
            merged_rows.push(merged_row);
            differences.push(diff_row);
            has_changes = has_changes || changed;
        }
    }

    // Process new AI-added rows
    for (new_row_idx, suggestion_full) in partition_rows.iter().skip(original_count).enumerate() {
        info!(
            "Processing AI-added row {}/{} for parent {}: suggestion_full.len()={}, key_prefix_count={}",
            new_row_idx + 1,
            partition_rows.len() - original_count,
            parent_row_index,
            suggestion_full.len(),
            key_prefix_count
        );

        let suggestion = skip_key_prefix(suggestion_full, key_prefix_count);
        info!(
            "After key_prefix skip: suggestion.len()={}, included.len()={}",
            suggestion.len(),
            included.len()
        );

        if suggestion.len() < included.len() {
            warn!(
                "Skipping malformed new structure suggestion row parent={} suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
                parent_row_index,
                suggestion.len(),
                included.len(),
                suggestion_full.len(),
                key_prefix_count
            );
            continue;
        }

        let mut ai_row = vec![String::new(); schema_len];
        let mut merged_row = vec![String::new(); schema_len];
        let mut diff_row = vec![false; schema_len];

        for (logical_i, col_index) in included.iter().enumerate() {
            let ai_value = suggestion.get(logical_i).cloned().unwrap_or_default();
            if let Some(slot) = ai_row.get_mut(*col_index) {
                *slot = ai_value.clone();
            }
            if let Some(slot) = merged_row.get_mut(*col_index) {
                *slot = ai_value.clone();
            }
            diff_row[*col_index] = true;
        }

        has_changes = true;
        ai_rows.push(ai_row.clone());
        merged_rows.push(merged_row);
        differences.push(diff_row);
        original_rows_aligned.push(vec![String::new(); schema_len]);

        info!(
            "Successfully added AI-generated row {}: ai_row={:?}",
            ai_rows.len() - 1,
            ai_row
        );
    }

    if original_rows_aligned.len() > merged_rows.len() {
        original_rows_aligned.truncate(merged_rows.len());
    }

    // Remove old entries for this parent
    state.ai_structure_reviews.retain(|entry| {
        !(entry.root_category == *root_category
            && entry.root_sheet == root_sheet
            && entry.parent_row_index == parent_row_index
            && entry.parent_new_row_index == parent_new_row_index
            && entry.structure_path == structure_path)
    });

    // Keep ALL columns in the row data, not just the included ones
    // This ensures excluded columns (not sent to AI) are preserved when serializing back to JSON
    // The differences vector will indicate which columns were actually changed by AI

    // Extract schema headers for ALL columns
    let schema_headers: Vec<String> = schema_fields.iter().map(|f| f.header.clone()).collect();

    state.ai_structure_reviews.push(StructureReviewEntry {
        root_category: root_category.clone(),
        root_sheet: root_sheet.to_string(),
        parent_row_index,
        parent_new_row_index,
        structure_path: structure_path.to_vec(),
        has_changes,
        // Issue #6 fix: Don't auto-accept even if no changes - let user decide
        accepted: false,
        rejected: false,
        decided: false,
        original_rows: original_rows_aligned.clone(),
        ai_rows,
        merged_rows,
        differences,
        row_operations: Vec::new(),
        schema_headers,
        // Use original_count, not original_rows_aligned.len() which includes AI-added rows
        original_rows_count: original_count,
    });

    // Issue #5 fix: Populate cache for structure parent row so previews work
    populate_parent_row_cache(state, parent_row_index, parent_new_row_index, parent_row.clone());

    if new_row_context.is_some() {
        state
            .ai_structure_new_row_contexts
            .remove(&parent_row_index);
    }

    feedback_writer.write(SheetOperationFeedback {
        message: format!(
            "AI structure review ready for {:?}/{} row {} ({} suggestion rows)",
            root_category,
            root_sheet,
            parent_row_index,
            partition_rows.len()
        ),
        is_error: false,
    });
}

/// Handle structure error by creating rejected entries
pub fn handle_structure_error(
    target_rows: &[usize],
    root_category: &Option<String>,
    root_sheet: &str,
    structure_path: &[usize],
    column_index: usize,
    schema_fields: &[StructureFieldDefinition],
    schema_len: usize,
    sheet: &SheetGridData,
    state: &mut EditorWindowState,
) {
    for parent_row_index in target_rows.iter() {
        let new_row_context = state
            .ai_structure_new_row_contexts
            .get(parent_row_index)
            .cloned();
        let parent_new_row_index = new_row_context.as_ref().map(|ctx| ctx.new_row_index);

        let parent_row = build_parent_row(
            &new_row_context,
            sheet,
            *parent_row_index,
            sheet
                .metadata
                .as_ref()
                .map(|m| m.columns.len())
                .unwrap_or(0),
        );

        let cell_value = if new_row_context.is_some() {
            String::new()
        } else {
            parent_row.get(column_index).cloned().unwrap_or_default()
        };

        let mut original_rows = parse_structure_rows_from_cell(&cell_value, schema_fields);
        if original_rows.is_empty() {
            original_rows.push(vec![String::new(); schema_len]);
        }
        for row in &mut original_rows {
            if row.len() < schema_len {
                row.resize(schema_len, String::new());
            }
        }

        state.ai_structure_reviews.retain(|entry| {
            !(entry.root_category == *root_category
                && entry.root_sheet == root_sheet
                && entry.parent_row_index == *parent_row_index
                && entry.parent_new_row_index == parent_new_row_index
                && entry.structure_path == structure_path)
        });

        let original_count = original_rows.len();
        state.ai_structure_reviews.push(StructureReviewEntry {
            root_category: root_category.clone(),
            root_sheet: root_sheet.to_string(),
            parent_row_index: *parent_row_index,
            parent_new_row_index,
            structure_path: structure_path.to_vec(),
            has_changes: false,
            accepted: false,
            rejected: true,
            decided: true,
            original_rows,
            ai_rows: Vec::new(),
            merged_rows: Vec::new(),
            differences: Vec::new(),
            row_operations: Vec::new(),
            schema_headers: Vec::new(),
            original_rows_count: original_count,
        });

        if new_row_context.is_some() {
            state.ai_structure_new_row_contexts.remove(parent_row_index);
        }
    }
}
