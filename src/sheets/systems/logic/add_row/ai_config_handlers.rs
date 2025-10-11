// src/sheets/systems/logic/add_row_handlers/ai_config_handlers.rs
// AI Schema Group configuration handlers (create, rename, delete, select)

use crate::sheets::{
    definitions::AiSchemaGroup,
    events::{
        RequestCreateAiSchemaGroup, RequestDeleteAiSchemaGroup, RequestRenameAiSchemaGroup,
        RequestSelectAiSchemaGroup, SheetDataModifiedInRegistryEvent, SheetOperationFeedback,
    },
    resources::SheetRegistry,
};
use bevy::prelude::*;

use super::json_persistence::save_to_json;

/// Handles AI schema group creation requests
pub fn handle_create_ai_schema_group(
    mut ev: EventReader<RequestCreateAiSchemaGroup>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found when creating AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when creating AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        let desired = e
            .desired_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Group");
        let unique_name = meta.ensure_unique_schema_group_name(desired);

        let included = meta.ai_included_column_indices();
        let allow_add = meta.ai_enable_row_generation;

        meta.ai_schema_groups.push(AiSchemaGroup {
            name: unique_name.clone(),
            included_columns: included,
            allow_add_rows: allow_add,
            structure_row_generation_overrides: meta.collect_structure_row_generation_overrides(),
            included_structures: meta.ai_included_structure_paths(),
        });
        meta.ai_active_schema_group = Some(unique_name.clone());

        let meta_clone = meta.clone();
        save_to_json(registry.as_ref(), &meta_clone);
        
        feedback.write(SheetOperationFeedback {
            message: format!(
                "Created AI schema group '{}' for {:?}/{}",
                unique_name, e.category, e.sheet_name
            ),
            is_error: false,
        });
        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: e.category.clone(),
            sheet_name: e.sheet_name.clone(),
        });
    }
}

/// Handles AI schema group rename requests
pub fn handle_rename_ai_schema_group(
    mut ev: EventReader<RequestRenameAiSchemaGroup>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found when renaming AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when renaming AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        let Some(group_index) = meta
            .ai_schema_groups
            .iter()
            .position(|g| g.name == e.old_name)
        else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "AI schema group '{}' not found in {:?}/{}",
                    e.old_name, e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let trimmed = e.new_name.trim();
        if trimmed.is_empty() {
            feedback.write(SheetOperationFeedback {
                message: "AI schema group name cannot be empty.".to_string(),
                is_error: true,
            });
            continue;
        }

        let conflict_exists = meta
            .ai_schema_groups
            .iter()
            .enumerate()
            .any(|(idx, g)| idx != group_index && g.name.eq_ignore_ascii_case(trimmed));

        let unique_name = if conflict_exists {
            meta.ensure_unique_schema_group_name(trimmed)
        } else {
            trimmed.to_string()
        };

        let current_name = meta.ai_schema_groups[group_index].name.clone();
        if current_name == unique_name {
            feedback.write(SheetOperationFeedback {
                message: format!("AI schema group '{}' already has that name.", unique_name),
                is_error: false,
            });
            continue;
        }

        meta.ai_schema_groups[group_index].name = unique_name.clone();

        if meta
            .ai_active_schema_group
            .as_ref()
            .map(|name| name == &e.old_name)
            .unwrap_or(false)
        {
            meta.ai_active_schema_group = Some(unique_name.clone());
        }

        let meta_clone = meta.clone();
        save_to_json(registry.as_ref(), &meta_clone);
        
        feedback.write(SheetOperationFeedback {
            message: format!(
                "Renamed AI schema group '{}' to '{}' for {:?}/{}",
                e.old_name, unique_name, e.category, e.sheet_name
            ),
            is_error: false,
        });
        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: e.category.clone(),
            sheet_name: e.sheet_name.clone(),
        });
    }
}

/// Handles AI schema group deletion requests
pub fn handle_delete_ai_schema_group(
    mut ev: EventReader<RequestDeleteAiSchemaGroup>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found when deleting AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when deleting AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        if meta.ai_schema_groups.len() <= 1 {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Cannot delete the last AI schema group for {:?}/{}.",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        }

        let Some(index) = meta
            .ai_schema_groups
            .iter()
            .position(|g| g.name == e.group_name)
        else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "AI schema group '{}' not found in {:?}/{}",
                    e.group_name, e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let removed = meta.ai_schema_groups.remove(index);

        meta.ensure_ai_schema_groups_initialized();

        if let Some(active_name) = meta.ai_active_schema_group.clone() {
            if let Err(err) = meta.apply_ai_schema_group(&active_name) {
                feedback.write(SheetOperationFeedback {
                    message: format!(
                        "Failed to reapply AI schema group '{}' after deletion in {:?}/{}: {}",
                        active_name, e.category, e.sheet_name, err
                    ),
                    is_error: true,
                });
                meta.ai_schema_groups
                    .insert(index.min(meta.ai_schema_groups.len()), removed);
                meta.ensure_ai_schema_groups_initialized();
                continue;
            }
        }

        let meta_clone = meta.clone();
        save_to_json(registry.as_ref(), &meta_clone);
        
        feedback.write(SheetOperationFeedback {
            message: format!(
                "Deleted AI schema group '{}' from {:?}/{}",
                removed.name, e.category, e.sheet_name
            ),
            is_error: false,
        });
        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: e.category.clone(),
            sheet_name: e.sheet_name.clone(),
        });
    }
}

/// Handles AI schema group selection requests
pub fn handle_select_ai_schema_group(
    mut ev: EventReader<RequestSelectAiSchemaGroup>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found when selecting AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when selecting AI schema group",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        match meta.apply_ai_schema_group(&e.group_name) {
            Ok(changed) => {
                let meta_clone = meta.clone();
                save_to_json(registry.as_ref(), &meta_clone);
                
                if changed {
                    feedback.write(SheetOperationFeedback {
                        message: format!(
                            "Applied AI schema group '{}' to {:?}/{}",
                            e.group_name, e.category, e.sheet_name
                        ),
                        is_error: false,
                    });
                } else {
                    feedback.write(SheetOperationFeedback {
                        message: format!(
                            "AI schema group '{}' already active for {:?}/{}",
                            e.group_name, e.category, e.sheet_name
                        ),
                        is_error: false,
                    });
                }
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: e.category.clone(),
                    sheet_name: e.sheet_name.clone(),
                });
            }
            Err(err) => {
                feedback.write(SheetOperationFeedback {
                    message: format!(
                        "Failed to apply AI schema group '{}' for {:?}/{}: {}",
                        e.group_name, e.category, e.sheet_name, err
                    ),
                    is_error: true,
                });
            }
        }
    }
}
