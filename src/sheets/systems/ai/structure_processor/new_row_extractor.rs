// src/sheets/systems/ai/structure_processor/new_row_extractor.rs
//! Extracts structure data from new row contexts (AI-generated rows pending review)

use crate::sheets::definitions::{SheetGridData, StructureFieldDefinition};
use crate::sheets::systems::ai::control_handler::ParentKeyInfo;
use crate::sheets::systems::ai::utils::extract_nested_structure_json;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

/// Extract structure rows from a new row context
///
/// Returns (parent_key, group_rows, partition_size)
pub fn extract_from_new_row_context(
    target_row: usize,
    context: &crate::ui::elements::editor::state::StructureNewRowContext,
    state: &EditorWindowState,
    root_sheet: &SheetGridData,
    structure_fields: &[StructureFieldDefinition],
    included_indices: &[usize],
    nested_field_path: &[String],
    job_structure_path: &[usize],
    key_col_index: Option<usize>,
    key_header: &Option<String>,
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
) -> (ParentKeyInfo, Vec<Vec<String>>, usize) {
    // Extract key column value from NewRowReview data
    // key_col_index is the grid column index for the key (e.g., 1 for Name column)
    // We need to find the corresponding position in non_structure_columns to get the value from ai array
    
    let key_value = if let Some(new_row_review) = state.ai_new_row_reviews.get(context.new_row_index) {
        // Determine which key column to use:
        // - For root tables: default to column 1 (Name)
        // - For structure tables: default to column 2 (first data column after row_index, parent_key)
        let default_key_col = if root_meta.is_structure_table() { 2 } else { 1 };
        let effective_key_col = key_col_index.unwrap_or(default_key_col);
        
        // Find the position of the key column in non_structure_columns
        let key_position = new_row_review.non_structure_columns
            .iter()
            .position(|&col| col == effective_key_col);
        
        if let Some(pos) = key_position {
            // Extract from ai array at the found position
            new_row_review.ai.get(pos).cloned().unwrap_or_default()
        } else {
            // Key column not in non_structure_columns, try first ai value as fallback
            warn!(
                "Key column {} not found in non_structure_columns {:?}, using first ai value",
                effective_key_col, new_row_review.non_structure_columns
            );
            new_row_review.ai.first().cloned().unwrap_or_default()
        }
    } else {
        // Fallback: try full_ai_row if available
        if let Some(full_row) = &context.full_ai_row {
            // Calculate dynamic prefix to skip parent keys from ancestor levels
            let dynamic_prefix = 0; // No review available, assume no prefix
            full_row.get(dynamic_prefix).cloned().unwrap_or_default()
        } else {
            String::new()
        }
    };

    info!(
        "New row context {}: extracted parent key_value='{}' (key_col_index={:?})",
        target_row, key_value, key_col_index
    );

    // Build parent key info for this new row context
    let parent_key = ParentKeyInfo {
        context: if key_header.is_some() && key_col_index.is_some() {
            root_meta
                .columns
                .get(key_col_index.unwrap())
                .and_then(|col| col.ai_context.clone())
        } else {
            None
        },
        key: key_value,
    };

    // First check if this new row is a duplicate of an existing row
    let duplicate_match_row = state
        .ai_new_row_reviews
        .get(context.new_row_index)
        .and_then(|nr| nr.duplicate_match_row);

    // Check if there's an existing structure review entry for this new row
    let existing_structure_rows = state
        .ai_structure_reviews
        .iter()
        .find(|sr| {
            sr.parent_new_row_index == Some(context.new_row_index)
                && sr.structure_path == job_structure_path
                && !sr.decided
        })
        .map(|sr| {
            // Use merged_rows if they have content (user decisions applied), otherwise ai_rows
            let source_rows =
                if !sr.merged_rows.is_empty() && sr.merged_rows.len() >= sr.ai_rows.len() {
                    &sr.merged_rows
                } else {
                    &sr.ai_rows
                };

            source_rows
                .iter()
                .map(|row| {
                    included_indices
                        .iter()
                        .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                        .collect::<Vec<String>>()
                })
                .collect::<Vec<Vec<String>>>()
        });

    let (group_rows, partition_size) = if let Some(existing_rows) = existing_structure_rows {
        info!(
            "New row context {}: using {} existing undecided structure rows",
            target_row,
            existing_rows.len()
        );
        let size = existing_rows.len();
        (existing_rows, size)
    } else if let Some(matched_row_idx) = duplicate_match_row {
        extract_from_duplicate_match(
            target_row,
            matched_row_idx,
            root_sheet,
            structure_fields,
            included_indices,
            nested_field_path,
            job_structure_path,
        )
    } else if let Some(full_row) = &context.full_ai_row {
        extract_from_full_ai_row(
            target_row,
            full_row,
            included_indices,
            nested_field_path,
            job_structure_path,
        )
    } else {
        // No existing structure data and no full_ai_row - create single empty row
        info!(
            "New row context {}: no existing data and no full_ai_row, creating empty structure row",
            target_row
        );
        let row = vec![String::new(); included_indices.len()];
        (vec![row], 1)
    };

    (parent_key, group_rows, partition_size)
}

/// Extract structure data from a matched duplicate row
fn extract_from_duplicate_match(
    target_row: usize,
    matched_row_idx: usize,
    root_sheet: &SheetGridData,
    structure_fields: &[StructureFieldDefinition],
    included_indices: &[usize],
    nested_field_path: &[String],
    job_structure_path: &[usize],
) -> (Vec<Vec<String>>, usize) {
    info!(
        "New row context {}: detected duplicate of existing row {}, extracting structure data from matched row",
        target_row, matched_row_idx
    );

    let structure_col_idx = job_structure_path[0];
    if let Some(existing_grid_row) = root_sheet.grid.get(matched_row_idx) {
        if let Some(structure_cell_json) = existing_grid_row.get(structure_col_idx) {
            // Parse the structure JSON from the existing row
            let target_json = if job_structure_path.len() > 1 {
                extract_nested_structure_json(structure_cell_json, nested_field_path)
            } else {
                Some(structure_cell_json.clone())
            };

            if let Some(json_str) = target_json {
                let parsed_rows = crate::sheets::systems::ai::utils::parse_structure_rows_from_cell(
                    &json_str,
                    structure_fields,
                );

                if !parsed_rows.is_empty() {
                    info!(
                        "New row context {}: extracted {} structure rows from matched existing row {}",
                        target_row, parsed_rows.len(), matched_row_idx
                    );

                    // Filter to included columns
                    let filtered_rows: Vec<Vec<String>> = parsed_rows
                        .iter()
                        .map(|row| {
                            included_indices
                                .iter()
                                .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                                .collect()
                        })
                        .collect();

                    let size = filtered_rows.len();
                    return (filtered_rows, size);
                }
            }
        }
    }

    // Fallback: empty row
    info!(
        "New row context {}: could not extract from matched row {}, using empty row",
        target_row, matched_row_idx
    );
    let row = vec![String::new(); included_indices.len()];
    (vec![row], 1)
}

/// Extract structure data from full AI row response
fn extract_from_full_ai_row(
    target_row: usize,
    full_row: &[String],
    included_indices: &[usize],
    nested_field_path: &[String],
    job_structure_path: &[usize],
) -> (Vec<Vec<String>>, usize) {
    let structure_col_idx = job_structure_path[0];

    info!(
        "New row context {}: extract_from_full_ai_row - full_row.len()={}, structure_col_idx={}, nested_field_path={:?}",
        target_row,
        full_row.len(),
        structure_col_idx,
        nested_field_path
    );

    if let Some(structure_cell_json) = full_row.get(structure_col_idx) {
        info!(
            "New row context {}: structure cell content (first 200 chars): {:?}",
            target_row,
            structure_cell_json.chars().take(200).collect::<String>()
        );

        // Extract nested structure if needed (for nested paths)
        let target_json = if job_structure_path.len() > 1 {
            extract_nested_structure_json(structure_cell_json, nested_field_path)
        } else {
            Some(structure_cell_json.clone())
        };

        if let Some(json_str) = target_json {
            // Parse JSON to extract rows
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(serde_json::Value::Array(arr)) => {
                    info!(
                        "New row context {}: parsed structure JSON array with {} elements",
                        target_row,
                        arr.len()
                    );

                    let parsed_rows: Vec<Vec<String>> = arr
                        .iter()
                        .filter_map(|item| {
                            if let serde_json::Value::Array(row_arr) = item {
                                let row: Vec<String> = row_arr
                                    .iter()
                                    .map(|val| match val {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Number(n) => n.to_string(),
                                        serde_json::Value::Bool(b) => b.to_string(),
                                        serde_json::Value::Null => String::new(),
                                        _ => serde_json::to_string(val).unwrap_or_default(),
                                    })
                                    .collect();
                                
                                // Filter by included_indices
                                let filtered_row: Vec<String> = included_indices
                                    .iter()
                                    .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                                    .collect();
                                
                                Some(filtered_row)
                            } else {
                                None
                            }
                        })
                        .collect();

                    if !parsed_rows.is_empty() {
                        info!(
                            "New row context {}: extracted {} structure rows from AI response",
                            target_row,
                            parsed_rows.len()
                        );
                        let size = parsed_rows.len();
                        return (parsed_rows, size);
                    } else {
                        warn!(
                            "New row context {}: parsed JSON array but got 0 valid rows",
                            target_row
                        );
                    }
                }
                Ok(other) => {
                    warn!(
                        "New row context {}: structure cell JSON is not an array: {:?}",
                        target_row,
                        other
                    );
                }
                Err(e) => {
                    warn!(
                        "New row context {}: failed to parse structure cell JSON: {}",
                        target_row, e
                    );
                }
            }
        } else {
            warn!(
                "New row context {}: could not extract target_json from structure cell",
                target_row
            );
        }
    } else {
        warn!(
            "New row context {}: structure column {} not found in full_row (len={})",
            target_row, structure_col_idx, full_row.len()
        );
    }

    // Fallback: empty row
    info!(
        "New row context {}: could not extract structure data, using empty row with {} columns",
        target_row,
        included_indices.len()
    );
    let row = vec![String::new(); included_indices.len()];
    (vec![row], 1)
}
