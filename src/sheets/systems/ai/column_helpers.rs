// src/sheets/systems/ai/column_helpers.rs
// Helper functions for extracting column information from metadata

use std::collections::HashMap;
use crate::sheets::definitions::{SheetMetadata, ColumnValidator};

/// Extract linked column information from metadata for specified columns
/// Returns a HashMap mapping column_index -> (target_sheet_name, target_column_index)
pub fn extract_linked_column_info(
    metadata: &SheetMetadata,
    columns: &[usize],
) -> HashMap<usize, (String, usize)> {
    columns
        .iter()
        .copied()
        .filter_map(|col| {
            metadata.columns.get(col).and_then(|c| match &c.validator {
                Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                    Some((col, (target_sheet_name.clone(), *target_column_index)))
                }
                _ => None,
            })
        })
        .collect()
}

/// Calculate dynamic prefix count from full row length and included columns length
pub fn calculate_dynamic_prefix(full_row_len: usize, included_len: usize) -> usize {
    full_row_len.saturating_sub(included_len)
}
