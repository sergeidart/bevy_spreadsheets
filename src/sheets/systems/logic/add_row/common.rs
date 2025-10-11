// src/sheets/systems/logic/add_row_handlers/common.rs
// Common helper functions used across add_row handlers

use crate::sheets::definitions::{ColumnDefinition, ColumnValidator, SheetMetadata, StructureFieldDefinition};
use std::collections::HashSet;

/// Sets an Option<bool> to a new value and returns true if it changed
pub(super) fn set_option(slot: &mut Option<bool>, new_value: Option<bool>) -> bool {
    if *slot != new_value {
        *slot = new_value;
        true
    } else {
        false
    }
}

/// Sets an include option (true -> None, false -> Some(false)) and returns true if changed
pub(super) fn set_include_option(flag: &mut Option<bool>, include: bool) -> bool {
    let desired = if include { Some(true) } else { Some(false) };
    if *flag != desired {
        *flag = desired;
        return true;
    }
    false
}

/// Updates general AI row generation flag for a sheet
pub(super) fn update_general_row_generation(meta: &mut SheetMetadata, enabled: bool) -> bool {
    if meta.ai_enable_row_generation != enabled {
        meta.ai_enable_row_generation = enabled;
        true
    } else {
        false
    }
}

/// Updates structure-specific AI row generation override
pub(super) fn update_structure_row_generation(
    meta: &mut SheetMetadata,
    path: &[usize],
    override_opt: Option<bool>,
) -> Result<(bool, Option<String>), String> {
    if path.is_empty() {
        return Err("structure path missing".to_string());
    }

    let mut names: Vec<String> = Vec::new();
    let column_index = path[0];
    let column = meta
        .columns
        .get_mut(column_index)
        .ok_or_else(|| format!("column index {} out of bounds", column_index))?;
    let changed = set_structure_override_column(column, &path[1..], override_opt, &mut names)?;
    let label = if names.is_empty() {
        None
    } else {
        Some(names.join(" -> "))
    };
    Ok((changed, label))
}

fn set_structure_override_column(
    column: &mut ColumnDefinition,
    path: &[usize],
    override_opt: Option<bool>,
    names: &mut Vec<String>,
) -> Result<bool, String> {
    names.push(column.header.clone());
    if path.is_empty() {
        return Ok(set_option(
            &mut column.ai_enable_row_generation,
            override_opt,
        ));
    }
    let next_index = path[0];
    let schema = column
        .structure_schema
        .as_mut()
        .ok_or_else(|| format!("structure column '{}' missing schema", column.header))?;
    let field = schema.get_mut(next_index).ok_or_else(|| {
        format!(
            "structure column '{}' index {} out of bounds",
            column.header, next_index
        )
    })?;
    set_structure_override_field(field, &path[1..], override_opt, names)
}

fn set_structure_override_field(
    field: &mut StructureFieldDefinition,
    path: &[usize],
    override_opt: Option<bool>,
    names: &mut Vec<String>,
) -> Result<bool, String> {
    names.push(field.header.clone());
    if path.is_empty() {
        return Ok(set_option(
            &mut field.ai_enable_row_generation,
            override_opt,
        ));
    }
    let next_index = path[0];
    let schema = field
        .structure_schema
        .as_mut()
        .ok_or_else(|| format!("nested structure '{}' missing schema", field.header))?;
    let next_field = schema.get_mut(next_index).ok_or_else(|| {
        format!(
            "nested structure '{}' index {} out of bounds",
            field.header, next_index
        )
    })?;
    set_structure_override_field(next_field, &path[1..], override_opt, names)
}

/// Applies AI send schema to root-level columns
pub(super) fn apply_send_schema_to_root(meta: &mut SheetMetadata, included: &HashSet<usize>) -> bool {
    apply_send_schema_to_columns(&mut meta.columns, included)
}

/// Applies AI send schema to a specific structure path
pub(super) fn apply_send_schema_to_structure(
    meta: &mut SheetMetadata,
    path: &[usize],
    included: &HashSet<usize>,
) -> Result<(bool, Vec<String>), String> {
    if path.is_empty() {
        return Err("structure path missing".to_string());
    }

    let column_index = path[0];
    let column = meta
        .columns
        .get_mut(column_index)
        .ok_or_else(|| format!("column index {} out of bounds", column_index))?;
    if !matches!(column.validator, Some(ColumnValidator::Structure)) {
        return Err(format!("column '{}' is not a structure", column.header));
    }

    let mut labels = vec![column.header.clone()];

    if path.len() == 1 {
        let schema = column
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("structure column '{}' missing schema", column.header))?;
        let changed = apply_send_schema_to_fields(schema, included);
        return Ok((changed, labels));
    }

    let mut field = {
        let schema = column
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("structure column '{}' missing schema", column.header))?;
        schema.get_mut(path[1]).ok_or_else(|| {
            format!(
                "structure column '{}' index {} out of bounds",
                column.header, path[1]
            )
        })?
    };
    labels.push(field.header.clone());

    for next_index in path.iter().skip(2) {
        let next_schema = field
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("nested structure '{}' missing schema", field.header))?;
        field = next_schema.get_mut(*next_index).ok_or_else(|| {
            format!(
                "nested structure '{}' index {} out of bounds",
                field.header, next_index
            )
        })?;
        labels.push(field.header.clone());
    }

    let target_schema = field
        .structure_schema
        .as_mut()
        .ok_or_else(|| format!("structure '{}' missing schema", field.header))?;
    let changed = apply_send_schema_to_fields(target_schema, included);
    Ok((changed, labels))
}

fn apply_send_schema_to_columns(
    columns: &mut [ColumnDefinition],
    included: &HashSet<usize>,
) -> bool {
    let mut changed = false;
    for (idx, column) in columns.iter_mut().enumerate() {
        if matches!(column.validator, Some(ColumnValidator::Structure)) {
            continue;
        }
        let should_include = included.contains(&idx);
        if should_include {
            if column.ai_include_in_send.is_some() {
                column.ai_include_in_send = None;
                changed = true;
            }
        } else if column.ai_include_in_send != Some(false) {
            column.ai_include_in_send = Some(false);
            changed = true;
        }
    }
    changed
}

fn apply_send_schema_to_fields(
    fields: &mut [StructureFieldDefinition],
    included: &HashSet<usize>,
) -> bool {
    let mut changed = false;
    for (idx, field) in fields.iter_mut().enumerate() {
        if matches!(field.validator, Some(ColumnValidator::Structure)) {
            continue;
        }
        let should_include = included.contains(&idx);
        if should_include {
            if field.ai_include_in_send.is_some() {
                field.ai_include_in_send = None;
                changed = true;
            }
        } else if field.ai_include_in_send != Some(false) {
            field.ai_include_in_send = Some(false);
            changed = true;
        }
    }
    changed
}

/// Sets the AI structure send flag for a specific structure path
pub(super) fn set_structure_send_flag(
    meta: &mut SheetMetadata,
    path: &[usize],
    include: bool,
) -> Result<(bool, Vec<String>), String> {
    if path.is_empty() {
        return Err("structure path missing".to_string());
    }

    let mut labels: Vec<String> = Vec::new();
    let column_index = path[0];
    let column = meta
        .columns
        .get_mut(column_index)
        .ok_or_else(|| format!("column index {} out of bounds", column_index))?;
    if !matches!(column.validator, Some(ColumnValidator::Structure)) {
        return Err(format!("column '{}' is not a structure", column.header));
    }
    labels.push(column.header.clone());

    if path.len() == 1 {
        let changed = set_include_option(&mut column.ai_include_in_send, include);
        return Ok((changed, labels));
    }

    let mut field = {
        let schema = column
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("structure column '{}' missing schema", column.header))?;
        schema.get_mut(path[1]).ok_or_else(|| {
            format!(
                "structure column '{}' index {} out of bounds",
                column.header, path[1]
            )
        })?
    };
    labels.push(field.header.clone());

    for next_index in path.iter().skip(2) {
        let schema = field
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("nested structure '{}' missing schema", field.header))?;
        field = schema.get_mut(*next_index).ok_or_else(|| {
            format!(
                "nested structure '{}' index {} out of bounds",
                field.header, next_index
            )
        })?;
        labels.push(field.header.clone());
    }

    if !matches!(field.validator, Some(ColumnValidator::Structure)) {
        return Err(format!(
            "path does not reference a structure node (ended at '{}')",
            field.header
        ));
    }

    let changed = set_include_option(&mut field.ai_include_in_send, include);
    Ok((changed, labels))
}

/// Update virtual structure sheets' metadata after parent structure column AI config changes.
/// This ensures that virtual sheets reflect the latest AI checkbox states from the parent schema.
pub(super) fn update_virtual_sheets_from_parent_structure(
    registry: &mut crate::sheets::resources::SheetRegistry,
    parent_category: &Option<String>,
    parent_sheet: &str,
    structure_path: &[usize],
) {
    // Clone the structure schema we need before doing any mutable operations
    // This avoids borrow checker issues
    let structure_schema_clone = {
        let Some(parent_sheet_data) = registry.get_sheet(parent_category, parent_sheet) else {
            return;
        };
        let Some(parent_meta) = &parent_sheet_data.metadata else {
            return;
        };

        // Navigate to the structure column at the given path
        if structure_path.is_empty() {
            return;
        }

        let column_index = structure_path[0];
        let Some(parent_column) = parent_meta.columns.get(column_index) else {
            return;
        };

        // Get the structure schema (could be nested)
        let structure_schema = if structure_path.len() == 1 {
            // Direct structure column
            parent_column.structure_schema.as_ref()
        } else {
            // Navigate through nested structure fields
            let mut current_schema = parent_column.structure_schema.as_ref();
            for &field_idx in structure_path.iter().skip(1) {
                if let Some(schema) = current_schema {
                    if let Some(field) = schema.get(field_idx) {
                        current_schema = field.structure_schema.as_ref();
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            current_schema
        };

        let Some(schema) = structure_schema else {
            return;
        };

        // Clone the schema so we can use it after releasing the immutable borrow
        schema.clone()
    };

    // Find all virtual sheets that have this as their parent
    // We need to collect the virtual sheet names first to avoid borrow conflicts
    let mut virtual_sheets_to_update: Vec<(Option<String>, String)> = Vec::new();
    let column_index = structure_path[0];

    for (cat, name, sheet_data) in registry.iter_sheets() {
        if let Some(meta) = &sheet_data.metadata {
            if let Some(parent_link) = &meta.structure_parent {
                // Check if this virtual sheet is a child of the structure we just updated
                if parent_link.parent_category == *parent_category
                    && parent_link.parent_sheet == parent_sheet
                    && parent_link.parent_column_index == column_index
                {
                    virtual_sheets_to_update.push((cat.clone(), name.to_string()));
                }
            }
        }
    }

    // Now update each virtual sheet's metadata using the cloned schema
    for (virt_cat, virt_name) in virtual_sheets_to_update {
        if let Some(virt_sheet) = registry.get_sheet_mut(&virt_cat, &virt_name) {
            if let Some(virt_meta) = &mut virt_sheet.metadata {
                // Update each column's AI flags from the parent structure schema
                for (idx, virt_col) in virt_meta.columns.iter_mut().enumerate() {
                    if let Some(schema_field) = structure_schema_clone.get(idx) {
                        // Update ai_include_in_send and ai_enable_row_generation from parent schema
                        virt_col.ai_include_in_send = schema_field.ai_include_in_send;
                        virt_col.ai_enable_row_generation = schema_field.ai_enable_row_generation;

                        // If this column is itself a structure, update its schema recursively
                        if virt_col.structure_schema.is_some()
                            && schema_field.structure_schema.is_some()
                        {
                            virt_col.structure_schema = schema_field.structure_schema.clone();
                        }
                    }
                }
            }
        }
    }
}
