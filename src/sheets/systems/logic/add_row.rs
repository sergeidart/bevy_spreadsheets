// src/sheets/systems/logic/add_row.rs
use crate::sheets::{
    definitions::{ColumnDefinition, SheetMetadata, StructureFieldDefinition},
    events::{
        AddSheetRowRequest, RequestToggleAiRowGeneration, SheetDataModifiedInRegistryEvent,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

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
                        .filter(|(cat_opt, s_name, _)| {
                            cat_opt == &category && s_name == &sheet_name
                        })
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

        let mut changed = false;
        let mut message = String::new();

        match &e.structure_path {
            Some(path) if !path.is_empty() => {
                match update_structure_row_generation(meta, path, e.structure_override) {
                    Ok((did_change, label_opt)) => {
                        changed = did_change;
                        let scope_label = label_opt.unwrap_or_else(|| format!("path {:?}", path));
                        message = match e.structure_override {
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
                    }
                    Err(err) => {
                        message = format!(
                            "Failed to update structure AI row generation for {:?}/{}: {}",
                            e.category, e.sheet_name, err
                        );
                        feedback.write(SheetOperationFeedback {
                            message,
                            is_error: true,
                        });
                        continue;
                    }
                }
            }
            _ => {
                changed = update_general_row_generation(meta, e.enabled);
                message = format!(
                    "AI row generation {} for {:?}/{}",
                    if e.enabled { "ENABLED" } else { "DISABLED" },
                    e.category,
                    e.sheet_name
                );
            }
        }

        if changed {
            let meta_clone = meta.clone();
            save_single_sheet(registry.as_ref(), &meta_clone);
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
