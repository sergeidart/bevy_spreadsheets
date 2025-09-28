// src/ui/elements/popups/column_options_on_close.rs
use crate::sheets::definitions::StructureFieldDefinition;
use crate::sheets::{resources::SheetRegistry, systems::io::save::save_single_sheet};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

/// Handles cleanup and potential saving when the column options popup is closed.
pub(super) fn handle_on_close(
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry, // Mut so we can persist parent schema immediately
    needs_save: bool,             // Flag indicating if filter/context change requires save
) {
    let popup_category = state.options_column_target_category.clone();
    let popup_sheet_name = state.options_column_target_sheet.clone();

    // --- Clear State ---
    state.show_column_options_popup = false;
    state.options_column_target_category = None;
    state.options_column_target_sheet.clear();
    state.options_column_target_index = 0;
    state.options_column_rename_input.clear();
    state.options_column_filter_input.clear();
    state.options_column_ai_context_input.clear(); // NEW: Clear AI context input
    state.column_options_popup_needs_init = false; // Should already be false
    state.options_validator_type = None;
    state.options_link_target_sheet = None;
    state.options_link_target_column_index = None;
    // Clear key-related ephemeral state so it always reloads from metadata next open
    state.options_existing_structure_key_parent_column = None;
    state.options_structure_key_parent_column_temp = None;

    // --- Trigger Manual Save ONLY if non-event changes occurred ---
    if needs_save {
        // If the active popup targeted a virtual sheet, propagate edits back to parent schema and save parent instead.
        if popup_sheet_name.starts_with("__virtual__") {
            // 1) Copy needed info from virtual sheet to avoid borrow conflicts
            if let Some(virtual_data) = registry.get_sheet(&popup_category, &popup_sheet_name) {
                if let Some(vmeta) = &virtual_data.metadata {
                    if let Some(parent_link) = &vmeta.structure_parent {
                        let parent_cat = parent_link.parent_category.clone();
                        let parent_sheet_name = parent_link.parent_sheet.clone();
                        let parent_col_index = parent_link.parent_column_index;
                        let schema_updates: Vec<StructureFieldDefinition> = vmeta
                            .columns
                            .iter()
                            .map(|vcol| StructureFieldDefinition {
                                header: vcol.header.clone(),
                                validator: vcol.validator.clone(),
                                data_type: vcol.data_type,
                                filter: vcol.filter.clone(),
                                ai_context: vcol.ai_context.clone(),
                                ai_enable_row_generation: vcol.ai_enable_row_generation,
                                ai_include_in_send: vcol.ai_include_in_send,
                                width: None,
                                structure_schema: vcol.structure_schema.clone(),
                                structure_column_order: vcol.structure_column_order.clone(),
                                structure_key_parent_column_index: vcol
                                    .structure_key_parent_column_index,
                                structure_ancestor_key_parent_column_indices: vcol
                                    .structure_ancestor_key_parent_column_indices
                                    .clone(),
                            })
                            .collect();

                        // 2) Update parent sheet metadata in-place
                        {
                            if let Some(parent_sheet) =
                                registry.get_sheet_mut(&parent_cat, &parent_sheet_name)
                            {
                                if let Some(parent_meta) = &mut parent_sheet.metadata {
                                    if let Some(parent_col) =
                                        parent_meta.columns.get_mut(parent_col_index)
                                    {
                                        if parent_col.structure_schema.is_none() {
                                            parent_col.structure_schema = Some(Vec::new());
                                        }
                                        let mut schema =
                                            parent_col.structure_schema.take().unwrap_or_default();
                                        if schema.len() < schema_updates.len() {
                                            schema.resize_with(schema_updates.len(), || StructureFieldDefinition {
                                                header: String::new(),
                                                validator: None,
                                                data_type: crate::sheets::definitions::ColumnDataType::String,
                                                filter: None,
                                                ai_context: None,
                                                ai_enable_row_generation: None,
                                                ai_include_in_send: None,
                                                width: None,
                                                structure_schema: None,
                                                structure_column_order: None,
                                                structure_key_parent_column_index: None,
                                                structure_ancestor_key_parent_column_indices: None,
                                            });
                                        }
                                        for (i, upd) in schema_updates.iter().enumerate() {
                                            if let Some(field) = schema.get_mut(i) {
                                                field.header = upd.header.clone();
                                                field.validator = upd.validator.clone();
                                                field.data_type = upd.data_type;
                                                field.filter = upd.filter.clone();
                                                field.ai_context = upd.ai_context.clone();
                                                field.ai_enable_row_generation =
                                                    upd.ai_enable_row_generation;
                                                field.ai_include_in_send = upd.ai_include_in_send;
                                                field.structure_schema =
                                                    upd.structure_schema.clone();
                                                field.structure_column_order =
                                                    upd.structure_column_order.clone();
                                                field.structure_key_parent_column_index =
                                                    upd.structure_key_parent_column_index;
                                                field
                                                    .structure_ancestor_key_parent_column_indices =
                                                    upd.structure_ancestor_key_parent_column_indices
                                                        .clone();
                                            }
                                        }
                                        parent_col.structure_schema = Some(schema);
                                    }
                                }
                            }
                        }

                        // 3) Save parent sheet metadata (fresh immutable borrow)
                        if let Some(parent_ro) = registry.get_sheet(&parent_cat, &parent_sheet_name)
                        {
                            if let Some(meta_to_save) = &parent_ro.metadata {
                                info!(
                                    "Propagated virtual column edits to parent '{:?}/{}'. Saving parent.",
                                    parent_cat, parent_sheet_name
                                );
                                save_single_sheet(registry, meta_to_save);
                                return; // Done
                            }
                        }
                    }
                }
            }
            warn!(
                "Edited virtual sheet '{}' but could not propagate changes to parent.",
                popup_sheet_name
            );
        } else {
            // Normal sheet: save directly
            if let Some(data_to_save) = registry.get_sheet(&popup_category, &popup_sheet_name) {
                if let Some(meta_to_save) = &data_to_save.metadata {
                    info!(
                        "Filter/Context changed for '{:?}/{}', triggering save.",
                        popup_category, popup_sheet_name
                    );
                    save_single_sheet(registry, meta_to_save);
                } else {
                    warn!(
                        "Cannot save after filter/context change for '{:?}/{}': Metadata missing.",
                        popup_category, popup_sheet_name
                    );
                }
            } else {
                warn!(
                    "Cannot save after filter/context change for '{:?}/{}': Sheet not found.",
                    popup_category, popup_sheet_name
                );
            }
        }
    }
}
