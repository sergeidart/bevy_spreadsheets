// src/sheets/systems/ai/structure_results.rs
// Helpers for processing structure batch results

use bevy::prelude::*;

use crate::sheets::definitions::{SheetGridData, StructureFieldDefinition};
use crate::sheets::events::SheetOperationFeedback;
use crate::ui::elements::editor::state::{EditorWindowState, StructureNewRowContext, StructureReviewEntry};

use super::row_helpers::skip_key_prefix;
use super::utils::parse_structure_rows_from_cell;

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

    let parent_row = build_parent_row(
        &new_row_context,
        sheet,
        parent_row_index,
        schema_fields.len(),
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
            &original_rows_aligned.get(local_idx).cloned().unwrap_or_else(|| vec![String::new(); schema_len]),
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

    // Extract schema headers from structure fields
    let schema_headers: Vec<String> = schema_fields
        .iter()
        .map(|f| f.header.clone())
        .collect();

    state.ai_structure_reviews.push(StructureReviewEntry {
        root_category: root_category.clone(),
        root_sheet: root_sheet.to_string(),
        parent_row_index,
        parent_new_row_index,
        structure_path: structure_path.to_vec(),
        has_changes,
        accepted: !has_changes,
        rejected: false,
        decided: !has_changes,
        original_rows: original_rows_aligned,
        ai_rows,
        merged_rows,
        differences,
        row_operations: Vec::new(),
        schema_headers,
    });

    if new_row_context.is_some() {
        state.ai_structure_new_row_contexts.remove(&parent_row_index);
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

/// Build parent row from context (synthetic for new rows or from sheet)
fn build_parent_row(
    new_row_context: &Option<StructureNewRowContext>,
    sheet: &SheetGridData,
    parent_row_index: usize,
    num_columns: usize,
) -> Vec<String> {
    if let Some(ctx) = new_row_context {
        let mut synthetic_row = vec![String::new(); num_columns];
        for (col_idx, value) in &ctx.non_structure_values {
            if let Some(slot) = synthetic_row.get_mut(*col_idx) {
                *slot = value.clone();
            }
        }
        synthetic_row
    } else {
        let mut row = sheet.grid.get(parent_row_index).cloned().unwrap_or_default();
        if row.len() < num_columns {
            row.resize(num_columns, String::new());
        }
        row
    }
}

/// Process a single structure suggestion row, returning (ai_row, merged_row, diff_row, has_changes)
fn process_structure_suggestion_row(
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
            sheet.metadata.as_ref().map(|m| m.columns.len()).unwrap_or(0),
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
        });

        if new_row_context.is_some() {
            state.ai_structure_new_row_contexts.remove(parent_row_index);
        }
    }
}
