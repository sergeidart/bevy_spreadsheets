// src/sheets/systems/ai/structure_processor/data_preparation.rs
//! Data preparation helpers for structure AI processing

use bevy::prelude::*;
use crate::sheets::definitions::{SheetGridData, StructureFieldDefinition};
use crate::sheets::sheet_metadata::SheetMetadata;
use crate::sheets::systems::ai::control_handler::{ParentGroup, ParentKeyInfo};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::elements::ai_review::ai_context_utils::decorate_context_with_type;
use super::new_row_extractor;

/// Read structure child rows from database for existing grid rows
pub fn read_structure_data_from_db(
    category: &Option<String>,
    parent_table_name: &str,
    structure_col_name: &str,
    target_row: usize,
    root_sheet: &SheetGridData,
    all_structure_headers: &[String],
) -> Vec<Vec<String>> {
    // Get database path
    let Some(cat) = category.as_ref() else {
        error!("No category for structure data extraction");
        return vec![vec![String::new(); all_structure_headers.len()]];
    };
    
    let base = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        warn!("Database {:?} does not exist for structure data extraction", db_path);
        return vec![vec![String::new(); all_structure_headers.len()]];
    }
    
    // Open database connection
    let conn = match crate::sheets::database::connection::DbConnection::open_existing(&db_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to open database {:?}: {}", db_path, e);
            return vec![vec![String::new(); all_structure_headers.len()]];
        }
    };
    
    // Get parent row's row_index value from grid
    let parent_row = match root_sheet.grid.get(target_row) {
        Some(r) => r,
        None => {
            error!("Target row {} not found in grid", target_row);
            return vec![vec![String::new(); all_structure_headers.len()]];
        }
    };
    
    // row_index is always at index 0
    let parent_row_index: i64 = match parent_row.get(0).and_then(|s| s.parse().ok()) {
        Some(idx) => idx,
        None => {
            error!("Failed to parse row_index from target row {}", target_row);
            return vec![vec![String::new(); all_structure_headers.len()]];
        }
    };
    
    info!(
        "Reading structure data: table={}, parent_row_index={}, headers={}",
        format!("{}_{}", parent_table_name, structure_col_name),
        parent_row_index,
        all_structure_headers.len()
    );
    
    // Read structure child rows using parent_row_index (structure tables use parent_key = row_index)
    match crate::sheets::database::reader::queries::read_structure_child_rows(
        &conn,
        parent_table_name,
        structure_col_name,
        parent_row_index,
        all_structure_headers,
    ) {
        Ok(rows) => {
            if rows.is_empty() {
                info!(
                    "No structure child rows found for parent_row_index={} in {}",
                    parent_row_index,
                    format!("{}_{}", parent_table_name, structure_col_name)
                );
                // Return empty vec so partition_size is 0 (all AI rows will be treated as new)
                Vec::new()
            } else {
                info!(
                    "Loaded {} structure child rows from database for parent_row_index={}",
                    rows.len(),
                    parent_row_index
                );
                rows
            }
        }
        Err(e) => {
            error!(
                "Failed to read structure child rows for parent_row_index={}: {}",
                parent_row_index, e
            );
            // Return empty vec on error too - don't create fake empty row
            Vec::new()
        }
    }
}

/// Build parent groups from target rows
#[allow(clippy::too_many_arguments)]
pub fn build_parent_groups(
    target_rows: &[usize],
    state: &EditorWindowState,
    root_sheet: &SheetGridData,
    structure_fields: &[StructureFieldDefinition],
    all_structure_headers: &[String],
    included_indices: &[usize],
    nested_field_path: &[String],
    job_structure_path: &[usize],
    key_col_index: Option<usize>,
    key_header: &Option<String>,
    root_meta: &SheetMetadata,
    category: &Option<String>,
    parent_table_name: &str,
    structure_col_name: &str,
) -> (Vec<ParentGroup>, Vec<usize>) {
    let mut parent_groups = Vec::new();
    let mut row_partitions = Vec::new();

    for &target_row in target_rows {
        // Check if this is a new row context or existing row
        if let Some(context) = state.ai_structure_new_row_contexts.get(&target_row) {
            let (parent_key, group_rows, partition_size) = new_row_extractor::extract_from_new_row_context(
                target_row,
                context,
                state,
                root_sheet,
                structure_fields,
                included_indices,
                nested_field_path,
                job_structure_path,
                key_col_index,
                key_header,
                root_meta,
            );

            row_partitions.push(partition_size);
            parent_groups.push(ParentGroup { parent_key, rows: group_rows });
        } else {
            // Existing row - check if we have AI review data first, then fall back to grid
            let root_row = match root_sheet.grid.get(target_row) {
                Some(r) => r,
                None => {
                    error!("Target row {} not found in grid", target_row);
                    continue;
                }
            };
            
            // Get the key column value for this row
            // Use configured key_col_index if available, otherwise:
            // - For root tables: default to column 1 (Name)
            // - For structure tables: default to column 2 (first data column after row_index, parent_key)
            let default_key_idx = if root_meta.is_structure_table() { 2 } else { 1 };
            let effective_key_idx = key_col_index.unwrap_or(default_key_idx);
            
            // Debug: log available reviews
            info!(
                "Looking for review: target_row={}, ai_row_reviews count={}, review row_indices={:?}",
                target_row,
                state.ai_row_reviews.len(),
                state.ai_row_reviews.iter().map(|r| r.row_index).collect::<Vec<_>>()
            );
            
            // IMPORTANT: Check if this row has AI review data - use AI values instead of original grid
            // This ensures structure calls use the AI-modified parent identifiers from the root batch
            let key_value = if let Some(review) = state.ai_row_reviews.iter().find(|r| r.row_index == target_row) {
                // Find the key column in the review's non_structure_columns mapping
                info!(
                    "Found review for row {}: non_structure_columns={:?}, ai={:?}",
                    target_row, review.non_structure_columns, review.ai
                );
                if let Some(pos) = review.non_structure_columns.iter().position(|&col| col == effective_key_idx) {
                    let ai_val = review.ai.get(pos).cloned().unwrap_or_default();
                    info!(
                        "Existing row {}: using AI review key_value='{}' from position {} (col {})",
                        target_row, ai_val, pos, effective_key_idx
                    );
                    ai_val
                } else {
                    // Key column not in AI review, fall back to grid
                    let grid_val = root_row.get(effective_key_idx).cloned().unwrap_or_default();
                    info!(
                        "Existing row {}: key col {} not in review, using grid key_value='{}'",
                        target_row, effective_key_idx, grid_val
                    );
                    grid_val
                }
            } else {
                // No AI review for this row, use original grid
                root_row.get(effective_key_idx).cloned().unwrap_or_default()
            };
            
            info!(
                "Existing row {}: final parent key_value='{}' from column {} (key_col_index={:?})",
                target_row, key_value, effective_key_idx, key_col_index
            );
            
            // Build parent key info
            let parent_key = ParentKeyInfo {
                context: if key_header.is_some() && key_col_index.is_some() {
                    root_meta
                        .columns
                        .get(key_col_index.unwrap())
                        .and_then(|col| col.ai_context.clone())
                } else {
                    None
                },
                key: key_value.clone(),
            };
            
            // Read structure data from database (not from grid count strings!)
            let all_rows = read_structure_data_from_db(
                category,
                parent_table_name,
                structure_col_name,
                target_row,
                root_sheet,
                all_structure_headers,
            );
            
            // Filter rows to only include columns that match included_indices
            let filtered_rows: Vec<Vec<String>> = all_rows
                .into_iter()
                .map(|row| {
                    included_indices
                        .iter()
                        .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                        .collect()
                })
                .collect();
            
            let partition_size = filtered_rows.len();
            row_partitions.push(partition_size);
            parent_groups.push(ParentGroup {
                parent_key,
                rows: filtered_rows,
            });
        }
    }

    (parent_groups, row_partitions)
}

/// Build column contexts and included indices
pub fn build_column_contexts(
    structure_fields: &[StructureFieldDefinition],
) -> (Vec<usize>, Vec<Option<String>>) {
    let mut included_indices = Vec::new();
    let mut column_contexts = Vec::new();

    for (idx, field) in structure_fields.iter().enumerate() {
        // Skip structure columns (nested structures)
        if matches!(
            field.validator,
            Some(crate::sheets::definitions::ColumnValidator::Structure)
        ) {
            continue;
        }
        // Skip columns explicitly excluded
        if matches!(field.ai_include_in_send, Some(false)) {
            continue;
        }
        included_indices.push(idx);
        column_contexts.push(decorate_context_with_type(
            field.ai_context.as_ref(),
            field.data_type,
        ));
    }

    (included_indices, column_contexts)
}
