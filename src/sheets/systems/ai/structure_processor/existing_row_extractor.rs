// src/sheets/systems/ai/structure_processor/existing_row_extractor.rs
//! Extracts structure data from existing rows in the grid

use crate::sheets::definitions::SheetGridData;
use crate::sheets::systems::ai::control_handler::ParentKeyInfo;
use crate::sheets::systems::ai::utils::extract_nested_structure_json;
use bevy::prelude::*;

/// Extract structure rows from an existing grid row
///
/// Returns (parent_key, group_rows, partition_size)
pub fn extract_from_existing_row(
    target_row: usize,
    root_sheet: &SheetGridData,
    all_structure_headers: &[String],
    included_indices: &[usize],
    nested_field_path: &[String],
    job_structure_path: &[usize],
    key_col_index: Option<usize>,
    key_header: &Option<String>,
    root_meta: &crate::sheets::sheet_metadata::SheetMetadata,
    parse_structure_cell_to_rows: &dyn Fn(&str, &[String]) -> Vec<Vec<String>>,
) -> Option<(ParentKeyInfo, Vec<Vec<String>>, usize)> {
    let root_row = root_sheet.grid.get(target_row)?;

    info!(
        "Processing target_row {}: row has {} cells, first 3 cells: {:?}",
        target_row,
        root_row.len(),
        root_row.iter().take(3).collect::<Vec<_>>()
    );

    // Get the key column value for this row
    let key_value = if let Some(key_idx) = key_col_index {
        let val = root_row.get(key_idx).cloned().unwrap_or_default();
        info!(
            "Extracting from row {}: key_col_index={}, key_value='{}', row has {} cells",
            target_row, key_idx, val, root_row.len()
        );
        if val.is_empty() {
            warn!(
                "Row {} has empty key value at index {} (row length: {})",
                target_row,
                key_idx,
                root_row.len()
            );
        }
        val
    } else {
        info!("Extracting from row {}: no key column", target_row);
        String::new()
    };

    // Navigate through the structure path to get the correct structure cell data
    let structure_cell_data = if let Some(&first_col_idx) = job_structure_path.first() {
        if let Some(root_cell) = root_row.get(first_col_idx) {
            if nested_field_path.is_empty() {
                // Direct structure column (no nesting)
                Some(root_cell.clone())
            } else {
                // Nested structure - extract the nested field data
                extract_nested_structure_json(root_cell, nested_field_path)
            }
        } else {
            None
        }
    } else {
        None
    };

    // Build parent key info for this target row
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

    // Parse structure cell data to get rows for this parent
    let (group_rows, partition_size) = if let Some(structure_cell) = structure_cell_data {
        info!(
            "Row {}: extracted structure cell data (first 100 chars): {}",
            target_row,
            &structure_cell.chars().take(100).collect::<String>()
        );
        
        // Parse with all headers first
        let all_rows = parse_structure_cell_to_rows(&structure_cell, all_structure_headers);
        info!(
            "Row {}: parsed into {} structure rows",
            target_row,
            all_rows.len()
        );

        // Filter each row to only include columns that match included_indices
        let filtered_rows: Vec<Vec<String>> = all_rows
            .into_iter()
            .map(|row| {
                included_indices
                    .iter()
                    .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();

        let size = filtered_rows.len();
        (filtered_rows, size)
    } else {
        // No structure data - create single empty row with only included columns
        let row = vec![String::new(); included_indices.len()];
        (vec![row], 1)
    };

    Some((parent_key, group_rows, partition_size))
}
