// src/sheets/systems/logic/add_row_handlers/ai_schema_handlers.rs
// AI send schema and structure send handlers

use crate::sheets::{
    definitions::ColumnValidator,
    events::{
        RequestBatchUpdateColumnAiInclude, RequestToggleAiRowGeneration,
        RequestUpdateAiSendSchema, RequestUpdateAiStructureSend, RequestUpdateColumnAiInclude,
        SheetDataModifiedInRegistryEvent, SheetOperationFeedback,
    },
    resources::SheetRegistry,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashSet};

use super::{
    common::{
        apply_send_schema_to_root, apply_send_schema_to_structure, set_structure_send_flag,
        update_general_row_generation, update_structure_row_generation,
        update_virtual_sheets_from_parent_structure,
    },
    db_persistence::{update_column_ai_include_db, update_column_metadata_db, update_table_ai_settings_db},
    json_persistence::save_to_json,
};

/// Handles AI row generation toggle requests
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
            save_to_json(registry.as_ref(), &meta_clone);

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
            // Persist to DB if this is a DB-backed sheet and we're toggling the root table-level flag
            if meta_clone.category.is_some() && e.structure_path.is_none() {
                let _ = update_table_ai_settings_db(&e.category, &e.sheet_name, Some(e.enabled));
            } else if meta_clone.category.is_none() {
                // Legacy JSON
                save_to_json(registry.as_ref(), &meta_clone);
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

/// Handles AI column include update requests (both single and batch)
pub fn handle_update_column_ai_include(
    mut single_events: EventReader<RequestUpdateColumnAiInclude>,
    mut batch_events: EventReader<RequestBatchUpdateColumnAiInclude>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for event in single_events.read() {
        apply_ai_include_updates(
            &mut registry,
            &mut feedback,
            &mut data_modified_writer,
            &event.category,
            &event.sheet_name,
            &[(event.column_index, event.include)],
        );
    }

    for event in batch_events.read() {
        if event.updates.is_empty() {
            continue;
        }
        apply_ai_include_updates(
            &mut registry,
            &mut feedback,
            &mut data_modified_writer,
            &event.category,
            &event.sheet_name,
            &event.updates,
        );
    }
}

fn apply_ai_include_updates(
    registry: &mut SheetRegistry,
    feedback: &mut EventWriter<SheetOperationFeedback>,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
    category: &Option<String>,
    sheet_name: &str,
    updates: &[(usize, bool)],
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
        save_to_json(&*registry, &meta_snapshot);
    } else {
        for idx in &changed_indices {
            let include_flag =
                !matches!(meta_snapshot.columns[*idx].ai_include_in_send, Some(false));
            let _ = update_column_ai_include_db(category, sheet_name, *idx, include_flag);
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

/// Handles AI send schema update requests
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
                    if meta_clone.category.is_none() {
                        save_to_json(registry.as_ref(), &meta_clone);
                    } else {
                        // Persist ai_include_in_send per column to DB
                        for (idx, c) in meta_clone.columns.iter().enumerate() {
                            if matches!(c.validator, Some(ColumnValidator::Structure)) {
                                continue;
                            }
                            let _ = update_column_metadata_db(
                                &e.category,
                                &e.sheet_name,
                                idx,
                                c.ai_include_in_send,
                            );
                        }
                    }

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

/// Handles AI structure send update requests
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
                    if meta_clone.category.is_none() {
                        save_to_json(registry.as_ref(), &meta_clone);
                    } else {
                        // Persist structure-level include flag at the structure root or field level where applicable
                        // If targeting a direct structure column (path len 1), update that column's include
                        if e.structure_path.len() == 1 {
                            let idx = e.structure_path[0];
                            if let Some(col) = meta_clone.columns.get(idx) {
                                let _ = update_column_metadata_db(
                                    &e.category,
                                    &e.sheet_name,
                                    idx,
                                    col.ai_include_in_send,
                                );
                            }
                        }
                        // For nested fields, persistence would target the virtual structure sheet's metadata table (future work)
                    }

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
