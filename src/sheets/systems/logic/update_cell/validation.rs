// src/sheets/systems/logic/update_cell/validation.rs
//! Validation logic for cell update operations

use crate::sheets::resources::SheetRegistry;

/// Validates that the cell location is within bounds
pub fn validate_cell_location(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    row_idx: usize,
    col_idx: usize,
) -> Result<(), String> {
    if let Some(sheet_data) = registry.get_sheet(category, sheet_name) {
        if let Some(row) = sheet_data.grid.get(row_idx) {
            if row.get(col_idx).is_some() {
                Ok(())
            } else {
                Err(format!(
                    "Column index {} out of bounds ({} columns).",
                    col_idx,
                    row.len()
                ))
            }
        } else {
            Err(format!(
                "Row index {} out of bounds ({} rows).",
                row_idx,
                sheet_data.grid.len()
            ))
        }
    } else {
        Err(format!("Sheet '{:?}/{}' not found.", category, sheet_name))
    }
}
