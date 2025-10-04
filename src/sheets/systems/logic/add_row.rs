// src/sheets/systems/logic/add_row.rs
use crate::sheets::{
    definitions::{
        AiSchemaGroup, ColumnDefinition, ColumnValidator, SheetMetadata, StructureFieldDefinition,
    },
    events::{
        AddSheetRowRequest, RequestCreateAiSchemaGroup, RequestDeleteAiSchemaGroup,
        RequestRenameAiSchemaGroup, RequestSelectAiSchemaGroup, RequestToggleAiRowGeneration,
        RequestUpdateAiSendSchema, RequestUpdateAiStructureSend, SheetDataModifiedInRegistryEvent,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use std::collections::HashSet;

pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut editor_state: Option<ResMut<EditorWindowState>>,
) {
    for event in events.read() {
        let mut category = event.category.clone();
        let mut sheet_name = event.sheet_name.clone();
        if let Some(state) = editor_state.as_ref() {
            if let Some(vctx) = state.virtual_structure_stack.last() {
                sheet_name = vctx.virtual_sheet_name.clone();
                category = vctx.parent.parent_category.clone();
                // virtual context just redirects target
            }
        }

        let mut metadata_cache: Option<SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.columns.len();
                // Unified behavior: always insert at top for consistency
                sheet_data.grid.insert(0, vec![String::new(); num_cols]);
                // If initial values provided, set them now to avoid race with subsequent events
                if let Some(init) = &event.initial_values {
                    if let Some(row0) = sheet_data.grid.get_mut(0) {
                        for (col, val) in init {
                            if *col < row0.len() {
                                row0[*col] = val.clone();
                            }
                        }
                    }
                }

                let msg = format!(
                    "Added new row at the top of sheet '{:?}/{}'.",
                    category, sheet_name
                );
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                // Invalidate any cached filtered indices for this sheet to force UI refresh
                if let Some(state_mut) = editor_state.as_mut() {
                    state_mut.force_filter_recalculation = true;
                    let keys_to_remove: Vec<_> = state_mut
                        .filtered_row_indices_cache
                        .keys()
                        .filter(|(cat_opt, s_name)| cat_opt == &category && s_name == &sheet_name)
                        .cloned()
                        .collect();
                    for k in keys_to_remove {
                        state_mut.filtered_row_indices_cache.remove(&k);
                    }
                }

                metadata_cache = Some(metadata.clone());

                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            } else {
                let msg = format!(
                    "Cannot add row to sheet '{:?}/{}': Metadata missing.",
                    category, sheet_name
                );
                warn!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        } else {
            let msg = format!(
                "Cannot add row: Sheet '{:?}/{}' not found in registry.",
                category, sheet_name
            );
            warn!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
        }

        if let Some(meta_to_save) = metadata_cache {
            info!(
                "Row added to '{:?}/{}', triggering immediate save.",
                category, sheet_name
            );
            let registry_immut = registry.as_ref();
            save_single_sheet(registry_immut, &meta_to_save);
        }
    }
}

pub fn handle_toggle_ai_row_generation(
    mut ev: EventReader<RequestToggleAiRowGeneration>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found for AI row generation toggle",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when toggling AI row generation",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        let outcome = match &e.structure_path {
            Some(path) if !path.is_empty() => {
                match update_structure_row_generation(meta, path, e.structure_override) {
                    Ok((did_change, label_opt)) => {
                        let scope_label = label_opt.unwrap_or_else(|| format!("path {:?}", path));
                        let message = match e.structure_override {
                            Some(true) => format!(
                                "AI row generation override ENABLED for structure '{}'",
                                scope_label
                            ),
                            Some(false) => format!(
                                "AI row generation override DISABLED for structure '{}'",
                                scope_label
                            ),
                            None => format!(
                                "AI row generation reverted to GENERAL setting for structure '{}'",
                                scope_label
                            ),
                        };
                        let group_changed = meta.set_active_ai_schema_group_structure_override(
                            path.as_slice(),
                            e.structure_override,
                        );
                        Some((did_change || group_changed, message))
                    }
                    Err(err) => {
                        let message = format!(
                            "Failed to update structure AI row generation for {:?}/{}: {}",
                            e.category, e.sheet_name, err
                        );
                        feedback.write(SheetOperationFeedback {
                            message,
                            is_error: true,
                        });
                        None
                    }
                }
            }
            _ => {
                let changed_setting = update_general_row_generation(meta, e.enabled);
                let group_changed = meta.set_active_ai_schema_group_allow_rows(e.enabled);
                let changed = changed_setting || group_changed;
                let message = format!(
                    "AI row generation {} for {:?}/{}",
                    if e.enabled { "ENABLED" } else { "DISABLED" },
                    e.category,
                    e.sheet_name
                );
                Some((changed, message))
            }
        };

        let Some((changed, message)) = outcome else {
            continue;
        };

        if changed {
            let meta_clone = meta.clone();
            save_single_sheet(registry.as_ref(), &meta_clone);
            
            // Update virtual sheets if this was a structure-level change
            if let Some(path) = &e.structure_path {
                if !path.is_empty() {
                    update_virtual_sheets_from_parent_structure(
                        &mut registry,
                        &e.category,
                        &e.sheet_name,
                        path,
                    );
                }
            }
            
            feedback.write(SheetOperationFeedback {
                message: message.clone(),
                is_error: false,
            });
            data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                category: e.category.clone(),
                sheet_name: e.sheet_name.clone(),
            });
        } else {
            feedback.write(SheetOperationFeedback {
                message,
                is_error: false,
            });
        }
    }
}

pub fn handle_update_ai_send_schema(
    mut ev: EventReader<RequestUpdateAiSendSchema>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found for AI send schema update",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when updating AI send schema",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        let included: HashSet<usize> = e.included_columns.iter().copied().collect();
        let included_count = included.len();
        let sheet_label = format!("{:?}/{}", e.category, e.sheet_name);
        let mut group_changed = false;

        let outcome = match &e.structure_path {
            Some(path) if !path.is_empty() => apply_send_schema_to_structure(meta, path, &included)
                .map(|(changed, labels)| (changed, Some(labels))),
            _ => {
                let changed = apply_send_schema_to_root(meta, &included);
                group_changed =
                    meta.set_active_ai_schema_group_included_columns(&e.included_columns);
                Ok((changed, None))
            }
        };

        match outcome {
            Ok((changed, labels_opt)) => {
                let scope_description = labels_opt.as_ref().map(|labels| labels.join(" -> "));
                let base_message = if let Some(labels) = scope_description.as_ref() {
                    format!(
                        "AI send columns for structure '{}' in {}",
                        labels, sheet_label
                    )
                } else {
                    format!("AI send columns for {}", sheet_label)
                };

                if changed || group_changed {
                    let feedback_text = if changed {
                        format!("{} updated ({} columns).", base_message, included_count)
                    } else {
                        format!("{} group state updated (no column changes).", base_message)
                    };
                    let meta_clone = meta.clone();
                    save_single_sheet(registry.as_ref(), &meta_clone);
                    
                    // Update virtual sheets if this is a structure path update
                    if let Some(path) = &e.structure_path {
                        update_virtual_sheets_from_parent_structure(
                            &mut registry,
                            &e.category,
                            &e.sheet_name,
                            path,
                        );
                    }
                    
                    feedback.write(SheetOperationFeedback {
                        message: feedback_text,
                        is_error: false,
                    });
                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                        category: e.category.clone(),
                        sheet_name: e.sheet_name.clone(),
                    });
                } else {
                    feedback.write(SheetOperationFeedback {
                        message: format!(
                            "{} already up to date ({} columns).",
                            base_message, included_count
                        ),
                        is_error: false,
                    });
                }
            }
            Err(err) => {
                feedback.write(SheetOperationFeedback {
                    message: format!(
                        "Failed to update AI send columns for {}: {}",
                        sheet_label, err
                    ),
                    is_error: true,
                });
            }
        }
    }
}

pub fn handle_update_ai_structure_send(
    mut ev: EventReader<RequestUpdateAiStructureSend>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for e in ev.read() {
        let Some(sheet) = registry.get_sheet_mut(&e.category, &e.sheet_name) else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Sheet {:?}/{} not found when updating AI structure send",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        let Some(meta) = sheet.metadata.as_mut() else {
            feedback.write(SheetOperationFeedback {
                message: format!(
                    "Metadata missing for {:?}/{} when updating AI structure send",
                    e.category, e.sheet_name
                ),
                is_error: true,
            });
            continue;
        };

        meta.ensure_ai_schema_groups_initialized();

        match set_structure_send_flag(meta, &e.structure_path, e.include) {
            Ok((changed, labels)) => {
                let include_paths = meta.ai_included_structure_paths();
                let group_changed =
                    meta.set_active_ai_schema_group_included_structures(&include_paths);
                let scope_description = if labels.is_empty() {
                    String::from("structure")
                } else {
                    labels.join(" -> ")
                };
                let base_message = format!(
                    "AI structure send for '{}' in {:?}/{}",
                    scope_description, e.category, e.sheet_name
                );

                if changed || group_changed {
                    let feedback_text = if changed {
                        format!("{} updated (include={}).", base_message, e.include)
                    } else {
                        format!("{} group state updated.", base_message)
                    };
                    let meta_clone = meta.clone();
                    save_single_sheet(registry.as_ref(), &meta_clone);
                    
                    // Update virtual sheets that might be displaying this structure
                    update_virtual_sheets_from_parent_structure(
                        &mut registry,
                        &e.category,
                        &e.sheet_name,
                        &e.structure_path,
                    );
                    
                    feedback.write(SheetOperationFeedback {
                        message: feedback_text,
                        is_error: false,
                    });
                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                        category: e.category.clone(),
                        sheet_name: e.sheet_name.clone(),
                    });
                } else {
                    feedback.write(SheetOperationFeedback {
                        message: format!("{} already set to include={}.", base_message, e.include),
                        is_error: false,
                    });
                }
            }
            Err(err) => {
                feedback.write(SheetOperationFeedback {
                    message: format!(
                        "Failed to update AI structure send for {:?}/{}: {}",
                        e.category, e.sheet_name, err
                    ),
                    is_error: true,
                });
            }
        }
    }
}

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
        save_single_sheet(registry.as_ref(), &meta_clone);
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
        save_single_sheet(registry.as_ref(), &meta_clone);
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
        save_single_sheet(registry.as_ref(), &meta_clone);
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
                save_single_sheet(registry.as_ref(), &meta_clone);
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

fn apply_send_schema_to_root(meta: &mut SheetMetadata, included: &HashSet<usize>) -> bool {
    apply_send_schema_to_columns(&mut meta.columns, included)
}

fn apply_send_schema_to_structure(
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

fn set_structure_send_flag(
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

fn set_include_option(flag: &mut Option<bool>, include: bool) -> bool {
    let desired = if include { Some(true) } else { Some(false) };
    if *flag != desired {
        *flag = desired;
        return true;
    }
    false
}

fn update_general_row_generation(meta: &mut SheetMetadata, enabled: bool) -> bool {
    if meta.ai_enable_row_generation != enabled {
        meta.ai_enable_row_generation = enabled;
        true
    } else {
        false
    }
}

fn update_structure_row_generation(
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

fn set_option(slot: &mut Option<bool>, new_value: Option<bool>) -> bool {
    if *slot != new_value {
        *slot = new_value;
        true
    } else {
        false
    }
}

/// Update virtual structure sheets' metadata after parent structure column AI config changes.
/// This ensures that virtual sheets reflect the latest AI checkbox states from the parent schema.
fn update_virtual_sheets_from_parent_structure(
    registry: &mut SheetRegistry,
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
                        if virt_col.structure_schema.is_some() && schema_field.structure_schema.is_some() {
                            virt_col.structure_schema = schema_field.structure_schema.clone();
                        }
                    }
                }
            }
        }
    }
}
