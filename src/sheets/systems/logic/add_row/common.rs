// src/sheets/systems/logic/add_row_handlers/common.rs
// Common helper functions used across add_row handlers

use crate::sheets::{
    definitions::{ColumnDefinition, ColumnValidator, SheetMetadata, StructureFieldDefinition},
    events::{SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashSet};

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

    // Validate path exists before attempting to modify
    if !meta.structure_path_exists(path) {
        return Err(format!("invalid structure path: {:?}", path));
    }

    // Use existing helper to get the label
    let label = meta.describe_structure_path(path);

    // Navigate and update using the existing AI schema group method
    let changed = meta.set_active_ai_schema_group_structure_override(path, override_opt);

    Ok((changed, label))
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

    // Validate that path points to a structure
    let column_index = path[0];
    let column = meta
        .columns
        .get(column_index)
        .ok_or_else(|| format!("column index {} out of bounds", column_index))?;
    if !matches!(column.validator, Some(ColumnValidator::Structure)) {
        return Err(format!("column '{}' is not a structure", column.header));
    }

    // Use existing helper to get the path labels
    let path_description = meta.describe_structure_path(path)
        .ok_or_else(|| "invalid structure path".to_string())?;
    let labels: Vec<String> = path_description.split(" -> ").map(|s| s.to_string()).collect();

    // Now we need to apply the changes to the mutable column
    // Navigate to get mutable reference
    let column = meta.columns.get_mut(column_index).unwrap();
    
    let changed = if path.len() == 1 {
        // Direct structure column - apply to its schema
        let schema = column
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("structure column '{}' missing schema", column.header))?;
        apply_send_schema_to_fields(schema, included)
    } else {
        // Navigate to nested field and apply
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
        }

        let target_schema = field
            .structure_schema
            .as_mut()
            .ok_or_else(|| format!("structure '{}' missing schema", field.header))?;
        apply_send_schema_to_fields(target_schema, included)
    };

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

    // Validate that path points to a structure
    let column_index = path[0];
    let column = meta
        .columns
        .get(column_index)
        .ok_or_else(|| format!("column index {} out of bounds", column_index))?;
    if !matches!(column.validator, Some(ColumnValidator::Structure)) {
        return Err(format!("column '{}' is not a structure", column.header));
    }

    // Use existing helper to get the path labels
    let path_description = meta.describe_structure_path(path)
        .ok_or_else(|| "invalid structure path".to_string())?;
    let labels: Vec<String> = path_description.split(" -> ").map(|s| s.to_string()).collect();

    // Navigate and set the flag
    let column = meta.columns.get_mut(column_index).unwrap();
    
    let changed = if path.len() == 1 {
        // Direct structure column
        set_include_option(&mut column.ai_include_in_send, include)
    } else {
        // Navigate to nested field
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
        }

        if !matches!(field.validator, Some(ColumnValidator::Structure)) {
            return Err(format!(
                "path does not reference a structure node (ended at '{}')",
                field.header
            ));
        }

        set_include_option(&mut field.ai_include_in_send, include)
    };

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
    // Get the structure schema using existing helper
    let structure_schema_clone = {
        let Some(parent_sheet_data) = registry.get_sheet(parent_category, parent_sheet) else {
            return;
        };
        let Some(parent_meta) = &parent_sheet_data.metadata else {
            return;
        };

        // Use existing helper to get the structure schema
        parent_meta.structure_fields_for_path(structure_path)
    };

    let Some(structure_schema_clone) = structure_schema_clone else {
        return;
    };

    // Find all virtual sheets that have this as their parent
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

/// Applies AI include updates to columns, handling both single and batch updates
pub(super) fn apply_ai_include_updates(
    registry: &mut SheetRegistry,
    feedback: &mut EventWriter<SheetOperationFeedback>,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
    category: &Option<String>,
    sheet_name: &str,
    updates: &[(usize, bool)],
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) {
    if updates.is_empty() {
        return;
    }

    let (meta_snapshot, changed_indices) = {
        let Some(sheet) = registry.get_sheet_mut(category, sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found for AI include update",
                    category, sheet_name
                ),
                is_error: true,
            });
            return;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when updating AI include",
                    category, sheet_name
                ),
                is_error: true,
            });
            return;
        };

        meta.ensure_ai_schema_groups_initialized();

        let mut dedup: BTreeMap<usize, bool> = BTreeMap::new();
        for (idx, include) in updates.iter().copied() {
            dedup.insert(idx, include);
        }

        let mut changed_indices: Vec<usize> = Vec::new();
        for (idx, include) in dedup {
            if let Some(column) = meta.columns.get_mut(idx) {
                let previously_included = !matches!(column.ai_include_in_send, Some(false));
                if include != previously_included {
                    column.ai_include_in_send = if include { None } else { Some(false) };
                    changed_indices.push(idx);
                }
            } else {
                feedback.write(SheetOperationFeedback {
                    message: format!(
                        "Column index {} out of bounds for {:?}/{}",
                        idx + 1,
                        category,
                        sheet_name
                    ),
                    is_error: true,
                });
            }
        }

        if changed_indices.is_empty() {
            return;
        }

        let included_indices: Vec<usize> = meta
            .columns
            .iter()
            .enumerate()
            .filter_map(|(idx, column)| {
                if matches!(column.validator, Some(ColumnValidator::Structure)) {
                    return None;
                }
                if matches!(column.ai_include_in_send, Some(false)) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect();
        let _ = meta.set_active_ai_schema_group_included_columns(&included_indices);

        (meta.clone(), changed_indices)
    };

    if meta_snapshot.category.is_none() {
        super::json_persistence::save_to_json(&*registry, &meta_snapshot);
    } else {
        for idx in &changed_indices {
            let include_flag =
                !matches!(meta_snapshot.columns[*idx].ai_include_in_send, Some(false));
            let _ = super::db_persistence::update_column_ai_include_db(category, sheet_name, *idx, include_flag, daemon_client);
        }
    }

    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
        category: category.clone(),
        sheet_name: sheet_name.to_string(),
    });

    feedback.write(SheetOperationFeedback {
        message: format!(
            "Updated AI send for {} column(s) in {:?}/{}",
            changed_indices.len(),
            category,
            sheet_name
        ),
        is_error: false,
    });
}
