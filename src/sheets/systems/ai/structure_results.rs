// src/sheets/systems/ai/structure_results.rs
// Helpers for processing structure batch results

use bevy::prelude::*;

use crate::sheets::definitions::{SheetGridData, StructureFieldDefinition};
use crate::sheets::events::SheetOperationFeedback;
use crate::ui::elements::editor::state::{EditorWindowState, StructureNewRowContext, StructureReviewEntry};

use super::row_helpers::skip_key_prefix;
use super::utils::parse_structure_rows_from_cell;
use serde_json::Value as JsonValue;

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
        state.ai_new_row_reviews.get(ctx.new_row_index)
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
        sheet.grid.get(matched_idx)
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
        extract_original_nested_structure_rows(&cell_value, structure_path, sheet, schema_fields, schema_len)
    };

    // Normalize row lengths
    for row in &mut original_rows {
        if row.len() < schema_len { row.resize(schema_len, String::new()); }
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

    // Keep ALL columns in the row data, not just the included ones
    // This ensures excluded columns (not sent to AI) are preserved when serializing back to JSON
    // The differences vector will indicate which columns were actually changed by AI
    
    // Extract schema headers for ALL columns
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
    // Cache key format: (Some(parent_row_index), None) for existing rows
    // or (None, Some(new_row_index)) for new rows
    let cache_key = if let Some(new_idx) = parent_new_row_index {
        (None, Some(new_idx))
    } else {
        (Some(parent_row_index), None)
    };
    
    // Store the full parent row in cache if not already present
    if !state.ai_original_row_snapshot_cache.contains_key(&cache_key) {
        state.ai_original_row_snapshot_cache.insert(cache_key, parent_row.clone());
    }

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

/// Extract original nested structure rows for a structure path with depth > 1.
/// Walks the JSON in the parent cell and collects rows from the nested array at the target depth.
/// Falls back to a single empty row if traversal fails.
///
/// NOTE: Current implementation supports exactly one additional nested level (root -> nested field).
/// Deeper paths will return a single empty row so the UI can still render a consistent preview.
/// This is sufficient for restoring original previews where previously nested levels showed as (empty).
pub(crate) fn extract_original_nested_structure_rows(
    cell_value: &str,
    structure_path: &[usize],
    sheet: &crate::sheets::definitions::SheetGridData,
    target_schema_fields: &[StructureFieldDefinition],
    schema_len: usize,
) -> Vec<Vec<String>> {
    // If cell empty, return one blank row
    let trimmed = cell_value.trim();
    if trimmed.is_empty() { return vec![vec![String::new(); schema_len]]; }

    let Some(meta) = sheet.metadata.as_ref() else {
        return vec![vec![String::new(); schema_len]];
    };

    // Parse the root JSON (array of objects expected for structure column)
    let Ok(root_json) = serde_json::from_str::<JsonValue>(trimmed) else {
        return vec![vec![String::new(); schema_len]];
    };

    // Helper to get schema vector for current level (after first column)
    // We navigate using the indices in structure_path beyond root (which references sheet column itself)
    let first_col_def = meta.columns.get(structure_path[0]);
    let mut current_schema_opt = first_col_def.and_then(|c| c.structure_schema.as_ref());

    // Collect arrays representing the current level's rows (start with the root array)
    let mut current_level_arrays: Vec<&[JsonValue]> = Vec::new();
    if let JsonValue::Array(arr) = &root_json { current_level_arrays.push(arr.as_slice()); }
    else { return vec![vec![String::new(); schema_len]]; }

    // Traverse each nested index except the final one: we need to arrive at the parent whose field holds the target array
    for (depth, &nested_idx) in structure_path.iter().enumerate().skip(1) {
        let Some(schema) = current_schema_opt else { return vec![vec![String::new(); schema_len]]; };
        let Some(field_def) = schema.get(nested_idx) else { return vec![vec![String::new(); schema_len]]; };
        let field_header = &field_def.header;

        // If this is the last index in the path, we extract the array(s) at this field as final rows
        let is_last = depth == structure_path.len() - 1;
        let mut next_level_row_objects: Vec<&serde_json::Map<String, JsonValue>> = Vec::new();
        let mut final_row_objects: Vec<&serde_json::Map<String, JsonValue>> = Vec::new();

        for arr in &current_level_arrays {
            for item in *arr {
                if let JsonValue::Object(obj) = item {
                    if let Some(field_val) = obj.get(field_header) {
                        if is_last {
                            if let JsonValue::Array(nested_rows) = field_val {
                                for row_item in nested_rows {
                                    if let JsonValue::Object(row_obj) = row_item { final_row_objects.push(row_obj); }
                                }
                            }
                        } else {
                            if let JsonValue::Array(nested_arr) = field_val {
                                // Collect all objects at this intermediate level to traverse further
                                for row_item in nested_arr { if let JsonValue::Object(row_obj) = row_item { next_level_row_objects.push(row_obj); } }
                                // We'll rebuild current_level_arrays from the nested arrays for deeper traversal
                            }
                        }
                    }
                }
            }
        }

        if is_last {
            if final_row_objects.is_empty() { return vec![vec![String::new(); schema_len]]; }
            // Map final objects into rows using target schema fields
            let mut out_rows = Vec::with_capacity(final_row_objects.len());
            for obj in final_row_objects {
                let mut row = Vec::with_capacity(schema_len);
                for field in target_schema_fields { row.push(obj.get(&field.header).and_then(|v| v.as_str()).unwrap_or("").to_string()); }
                if row.len() < schema_len { row.resize(schema_len, String::new()); }
                out_rows.push(row);
            }
            return out_rows;
        } else {
            // Prepare for next loop: build arrays from next_level_row_objects by collecting nested arrays again.
            // But we actually need the nested arrays, not the objects; since we already iterated arrays we can rebuild from objects containing further nested structure later.
            // For simplicity, rebuild current_level_arrays from any nested structure arrays contained in the gathered objects for the next nested_idx.
            // To do that we peek at the next schema after current field.
            let next_schema_opt = field_def.structure_schema.as_ref();
            if next_schema_opt.is_none() { return vec![vec![String::new(); schema_len]]; }
            current_schema_opt = next_schema_opt;

            // Rebuild current_level_arrays by pulling arrays from the collected objects (for the next nested iteration)
            let rebuilt: Vec<&[JsonValue]> = Vec::new();
            for obj in next_level_row_objects {
                // The next iteration will look up a field by header, but we don't yet know which header; that's fine because we rebuild when we know nested_idx.
                // So we keep the entire object array contents by finding all child arrays in this object whose key matches the upcoming field when iterating.
                // Instead of guessing now, we'll postpone and reconstruct from the original traversal logic; thus we leave current_level_arrays empty forcing early return if deeper than one level.
                // NOTE: For now we only support a single nested level reliably. If deeper nesting is required, a more elaborate traversal should be implemented.
                let _ = obj; // silence unused warning if feature not extended
            }
            if structure_path.len() > 2 { // deeper than one nested level not yet fully supported
                debug!("Nested structure original extraction: deeper than 2 levels not fully supported (path {:?})", structure_path);
                return vec![vec![String::new(); schema_len]];
            }
            current_level_arrays = rebuilt; // likely empty -> triggers fallback
        }
    }

    vec![vec![String::new(); schema_len]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets::definitions::{ColumnDefinition, ColumnValidator, StructureFieldDefinition, ColumnDataType, SheetMetadata};

    #[test]
    fn test_extract_original_nested_structure_rows_one_level() {
        // Simulate a structure cell with nested structure field "items"
        // Root schema: [{ name: String, items: Structure[...] }]
        let json = r#"[
            {"name":"A", "items":[{"val":"1"},{"val":"2"}]},
            {"name":"B", "items":[{"val":"3"}]}
        ]"#;

        // Build fake sheet metadata with one structure column at index 0
        let nested_leaf_field = StructureFieldDefinition {
            header: "val".to_string(),
            validator: Some(ColumnValidator::Basic(ColumnDataType::String)),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let nested_field = StructureFieldDefinition {
            header: "items".to_string(),
            validator: Some(ColumnValidator::Structure),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: Some(vec![nested_leaf_field.clone()]),
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let name_field = StructureFieldDefinition {
            header: "name".to_string(),
            validator: Some(ColumnValidator::Basic(ColumnDataType::String)),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let root_col = ColumnDefinition {
            header: "root".to_string(),
            validator: Some(ColumnValidator::Structure),
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            width: None,
            structure_schema: Some(vec![name_field.clone(), nested_field.clone()]),
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        };
        let meta = SheetMetadata {
            sheet_name: "test".to_string(),
            category: None,
            data_filename: "test.json".to_string(),
            columns: vec![root_col],
            ai_general_rule: None,
            ai_model_id: crate::sheets::definitions::default_ai_model_id(),
            ai_temperature: None,
            ai_top_k: None,
            ai_top_p: None,
            requested_grounding_with_google_search: crate::sheets::definitions::default_grounding_with_google_search(),
            ai_enable_row_generation: true,
            ai_schema_groups: Vec::new(),
            ai_active_schema_group: None,
            random_picker: None,
            structure_parent: None,
        };
        let sheet = crate::sheets::definitions::SheetGridData { grid: vec![vec![json.to_string()]], metadata: Some(meta) };

        // Target schema is the nested field schema (val)
        let target_schema = nested_field.structure_schema.as_ref().unwrap();
        let rows = extract_original_nested_structure_rows(&sheet.grid[0][0], &[0,1], &sheet, target_schema, target_schema.len());
        // NOTE: Current implementation returns a flattened sequence of nested rows per parent object.
        assert_eq!(rows.len(), 3, "Expected 3 flattened nested rows");
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[1][0], "2");
        assert_eq!(rows[2][0], "3");
    }
}

/// Build parent row from context (synthetic for new rows or from sheet)
fn build_parent_row(
    new_row_context: &Option<StructureNewRowContext>,
    sheet: &SheetGridData,
    parent_row_index: usize,
    num_columns: usize,
) -> Vec<String> {
    if let Some(ctx) = new_row_context {
        // Start with full_ai_row if available (includes structure columns as JSON)
        // Otherwise create empty row and populate with non-structure values
        let mut synthetic_row = if let Some(ref full_row) = ctx.full_ai_row {
            let mut row = full_row.clone();
            if row.len() < num_columns {
                row.resize(num_columns, String::new());
            }
            row
        } else {
            vec![String::new(); num_columns]
        };
        
        // Override with non_structure_values (these are the user-facing values)
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
