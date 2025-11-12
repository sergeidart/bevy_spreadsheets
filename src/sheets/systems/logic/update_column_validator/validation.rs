// validation.rs
// Validation logic for column validator updates

use crate::sheets::{
    definitions::ColumnValidator,
    resources::SheetRegistry,
};

/// Validates that a column validator update is legal.
/// Returns Ok(()) if valid, Err(message) if invalid.
pub fn validate_column_update(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    col_index: usize,
    new_validator_opt: &Option<ColumnValidator>,
) -> Result<(), String> {
    let sheet_data = registry
        .get_sheet(category, sheet_name)
        .ok_or_else(|| format!("Sheet '{:?}/{}' not found.", category, sheet_name))?;
    
    let metadata = sheet_data
        .metadata
        .as_ref()
        .ok_or_else(|| "Metadata missing.".to_string())?;
    
    if col_index >= metadata.columns.len() {
        return Err(format!(
            "Column index {} out of bounds ({} columns).",
            col_index,
            metadata.columns.len()
        ));
    }
    
    if let Some(v) = new_validator_opt {
        match v {
            ColumnValidator::Basic(_) => {}
            ColumnValidator::Linked {
                target_sheet_name,
                target_column_index,
            } => {
                validate_linked_validator(
                    registry,
                    category,
                    sheet_name,
                    col_index,
                    target_sheet_name,
                    *target_column_index,
                )?;
            }
            ColumnValidator::Structure => {
                // Schema validated separately when schema provided
            }
        }
    }
    
    Ok(())
}

/// Validates that a Linked validator configuration is legal.
fn validate_linked_validator(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    col_index: usize,
    target_sheet_name: &str,
    target_column_index: usize,
) -> Result<(), String> {
    // Look for target sheet anywhere (category-agnostic) for convenience
    let mut found_sheet_meta = None;
    for (_cat, name, data) in registry.iter_sheets() {
        if name == target_sheet_name {
            found_sheet_meta = data.metadata.as_ref();
            break;
        }
    }
    
    let target_meta = found_sheet_meta
        .ok_or_else(|| format!("Target sheet '{}' not found.", target_sheet_name))?;
    
    if target_column_index >= target_meta.columns.len() {
        return Err(format!(
            "Target column index {} out of bounds for sheet '{}' ({} columns).",
            target_column_index,
            target_sheet_name,
            target_meta.columns.len()
        ));
    }
    
    // Prevent linking to itself (same category, same sheet, same column)
    if target_sheet_name == sheet_name
        && target_column_index == col_index
        && target_meta.category == *category
    {
        return Err("Cannot link column to itself.".to_string());
    }
    
    Ok(())
}
