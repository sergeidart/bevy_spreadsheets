// src/ui/elements/popups/column_options_popup.rs
use crate::ui::elements::editor::state::EditorWindowState;
use crate::{
    sheets::{
        definitions::ColumnValidator,
        events::{RequestUpdateColumnName, RequestUpdateColumnValidator},
        resources::SheetRegistry,
    },
    ui::elements::editor::state::ValidatorTypeChoice,
};
use bevy::prelude::*;
use bevy_egui::egui;

use super::column_options_on_close::handle_on_close;
use super::column_options_ui::show_column_options_window_ui;
use super::column_options_validator::apply_validator_update;

/// Main orchestrator function for the column options popup.
/// Handles initialization, calls UI, applies changes, and manages closing.
pub fn show_column_options_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    column_validator_writer: &mut EventWriter<RequestUpdateColumnValidator>,
    registry: &mut SheetRegistry,
) {
    if !state.show_column_options_popup {
        return;
    }

    if state.column_options_popup_needs_init {
        initialize_popup_state(state, registry);
        state.column_options_popup_needs_init = false;
    }

    let ui_result = {
        let registry_immut = &*registry;
        show_column_options_window_ui(ctx, state, registry_immut)
    };

    let mut needs_manual_save = false;
    let mut actions_ok = true;
    let mut non_event_change_occurred = false;

    if ui_result.apply_clicked {
        let category = &state.options_column_target_category;
        let sheet_name = &state.options_column_target_sheet;
        let col_index = state.options_column_target_index;
        let mut rename_sent = false;
        let mut validator_sent = false;

        let (current_name, current_filter, current_context, current_validator) = {
            let maybe_col_def = registry
                .get_sheet(category, sheet_name)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.columns.get(col_index));
            if let Some(col_def) = maybe_col_def {
                (
                    Some(col_def.header.clone()),
                    col_def.filter.clone(),
                    col_def.ai_context.clone(),
                    col_def.validator.clone(),
                )
            } else {
                (None, None, None, None)
            }
        };

        if current_name.is_none() {
            warn!(
                "Apply failed: Column index {} invalid for sheet '{:?}/{}'.",
                col_index, category, sheet_name
            );
            actions_ok = false;
        }

        if actions_ok {
            let new_name_trimmed = state.options_column_rename_input.trim();
            if Some(new_name_trimmed.to_string()) != current_name {
                if new_name_trimmed.is_empty() {
                    warn!("Column rename failed: New name empty.");
                    actions_ok = false;
                } else {
                    let is_duplicate = registry
                        .get_sheet(category, sheet_name)
                        .and_then(|s| s.metadata.as_ref())
                        .map_or(false, |m| {
                            m.columns.iter().enumerate().any(|(i, c)| {
                                i != col_index && c.header.eq_ignore_ascii_case(new_name_trimmed)
                            })
                        });
                    if !is_duplicate {
                        column_rename_writer.write(RequestUpdateColumnName {
                            category: category.clone(),
                            sheet_name: sheet_name.clone(),
                            column_index: col_index,
                            new_name: new_name_trimmed.to_string(),
                        });
                        rename_sent = true;
                    } else {
                        warn!(
                            "Column rename failed: Name '{}' duplicates existing.",
                            new_name_trimmed
                        );
                        actions_ok = false;
                    }
                }
            }
        }

        if actions_ok {
            // Build filter from multi-term list (ignore empties)
            let joined_terms: String = state
                .options_column_filter_terms
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("|");
            state.options_column_filter_input = joined_terms.clone();
            let filter_to_store: Option<String> = if joined_terms.is_empty() {
                None
            } else {
                Some(joined_terms)
            };
            let context_to_store: Option<String> =
                if state.options_column_ai_context_input.trim().is_empty() {
                    None
                } else {
                    Some(state.options_column_ai_context_input.trim().to_string())
                };

            let filter_changed = current_filter != filter_to_store;
            let context_changed = current_context != context_to_store;

            if filter_changed || context_changed {
                non_event_change_occurred = true;
                if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
                    if let Some(meta) = &mut sheet_data.metadata {
                        if let Some(col_def) = meta.columns.get_mut(col_index) {
                            if filter_changed {
                                info!(
                                    "Updating filter for col {} of '{:?}/{}'.",
                                    col_index + 1,
                                    category,
                                    sheet_name
                                );
                                col_def.filter = filter_to_store;
                                // --- ADDED: Invalidate cache on filter change ---
                                if state.selected_category == *category
                                    && state.selected_sheet_name.as_ref() == Some(sheet_name)
                                {
                                    state.force_filter_recalculation = true;
                                    debug!("Filter changed for current sheet, forcing recalc.");
                                }
                            }
                            if context_changed {
                                info!(
                                    "Updating AI context for col {} of '{:?}/{}'.",
                                    col_index + 1,
                                    category,
                                    sheet_name
                                );
                                col_def.ai_context = context_to_store;
                            }
                        } else {
                            warn!("Filter/Context update failed: Index out of bounds.");
                            actions_ok = false;
                        }
                    } else {
                        warn!("Filter/Context update failed: Metadata missing.");
                        actions_ok = false;
                    }
                } else {
                    warn!("Filter/Context update failed: Sheet not found.");
                    actions_ok = false;
                }
            }
        }

        if actions_ok {
            let registry_immut = &*registry;
            let (new_validator_opt, validation_ok) = match state.options_validator_type {
                Some(ValidatorTypeChoice::Basic) => (
                    Some(ColumnValidator::Basic(state.options_basic_type_select)),
                    true,
                ),
                Some(ValidatorTypeChoice::Linked) => {
                    if let (Some(ts), Some(tc)) = (
                        state.options_link_target_sheet.as_ref(),
                        state.options_link_target_column_index,
                    ) {
                        (
                            Some(ColumnValidator::Linked {
                                target_sheet_name: ts.clone(),
                                target_column_index: tc,
                            }),
                            true,
                        )
                    } else {
                        (None, false)
                    }
                }
                Some(ValidatorTypeChoice::Structure) => (Some(ColumnValidator::Structure), true),
                None => (None, false),
            };

            if !validation_ok {
                actions_ok = false;
                warn!("Validator update failed: Invalid selection state.");
            } else if current_validator != new_validator_opt {
                // Risk analysis now only triggers confirmation; actual apply waits for second Apply press.
                let old_was_structure =
                    matches!(current_validator, Some(ColumnValidator::Structure));
                let new_is_structure =
                    matches!(new_validator_opt, Some(ColumnValidator::Structure));
                let mut target_col_has_non_empty_cells = false;
                if !old_was_structure && new_is_structure {
                    if let Some(sheet) = registry.get_sheet(
                        &state.options_column_target_category,
                        &state.options_column_target_sheet,
                    ) {
                        for row in &sheet.grid {
                            if let Some(cell) = row.get(state.options_column_target_index) {
                                if !cell.trim().is_empty() {
                                    target_col_has_non_empty_cells = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                let risky = (old_was_structure && !new_is_structure)
                    || (!old_was_structure && new_is_structure && target_col_has_non_empty_cells);
                if risky && !state.pending_validator_change_requires_confirmation {
                    state.pending_validator_change_requires_confirmation = true;
                    state.pending_validator_new_validator_summary = Some(format!(
                        "{:?} -> {:?}",
                        current_validator, new_validator_opt
                    ));
                    state.pending_validator_target_is_structure = new_is_structure;
                    // Do NOT apply yet. User must press Apply again after confirmation cleared.
                } else {
                    if !apply_validator_update(state, registry_immut, column_validator_writer) {
                        actions_ok = false;
                        warn!("Validator update failed: apply_validator_update returned error.");
                    } else {
                        validator_sent = true;
                    }
                    state.pending_validator_change_requires_confirmation = false;
                    state.pending_validator_new_validator_summary = None;
                    state.pending_validator_target_is_structure = false;
                }
            } else {
                // No change; clear any stale confirmation flag
                state.pending_validator_change_requires_confirmation = false;
                state.pending_validator_new_validator_summary = None;
                state.pending_validator_target_is_structure = false;
            }
        }
        needs_manual_save =
            actions_ok && non_event_change_occurred && !rename_sent && !validator_sent;
    }

    // Confirmation UI now handled inside window UI; orchestration just checks flag to gate closing.

    let should_close = (ui_result.apply_clicked
        && actions_ok
        && !state.pending_validator_change_requires_confirmation)
        || ui_result.cancel_clicked
        || ui_result.close_via_x;

    if should_close {
        handle_on_close(state, registry, needs_manual_save);
    }
}

fn initialize_popup_state(state: &mut EditorWindowState, registry: &SheetRegistry) {
    let target_category = &state.options_column_target_category;
    let target_sheet = &state.options_column_target_sheet;
    let col_index = state.options_column_target_index;

    // If target is a virtual sheet, try to resolve its parent field definition for consistent editing
    let (column_def_opt, parent_field_opt) = {
        let sheet_opt = registry.get_sheet(target_category, target_sheet);
        if let Some(s) = sheet_opt {
            if let Some(m) = &s.metadata {
                let col = m.columns.get(col_index);
                if m.structure_parent.is_some() {
                    let parent = m.structure_parent.as_ref().unwrap();
                    if let Some(p_sheet) =
                        registry.get_sheet(&parent.parent_category, &parent.parent_sheet)
                    {
                        if let Some(p_meta) = &p_sheet.metadata {
                            if let Some(p_col) = p_meta.columns.get(parent.parent_column_index) {
                                let field = p_col
                                    .structure_schema
                                    .as_ref()
                                    .and_then(|fields| fields.get(col_index))
                                    .cloned();
                                (col, field)
                            } else {
                                (col, None)
                            }
                        } else {
                            (col, None)
                        }
                    } else {
                        (col, None)
                    }
                } else {
                    (col, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    };

    // Prefer real column_def; else synthesize from parent field definition for initialization
    let synthesized_from_parent: Option<crate::sheets::definitions::ColumnDefinition> =
        parent_field_opt
            .as_ref()
            .map(|f| crate::sheets::definitions::ColumnDefinition {
                header: f.header.clone(),
                validator: f.validator.clone(),
                data_type: f.data_type,
                filter: f.filter.clone(),
                ai_context: f.ai_context.clone(),
                ai_enable_row_generation: f.ai_enable_row_generation,
                ai_include_in_send: f.ai_include_in_send,
                width: None,
                structure_schema: f.structure_schema.clone(),
                structure_column_order: f.structure_column_order.clone(),
                structure_key_parent_column_index: f.structure_key_parent_column_index,
                structure_ancestor_key_parent_column_indices: f
                    .structure_ancestor_key_parent_column_indices
                    .clone(),
            });
    let col_def_owned;
    let col_def_ref: &crate::sheets::definitions::ColumnDefinition =
        if let Some(cd) = column_def_opt {
            cd
        } else if let Some(synth) = synthesized_from_parent.as_ref() {
            col_def_owned = synth.clone();
            &col_def_owned
        } else {
            None.expect("unreachable")
        };
    if let Some(_guard) = Some(()) {
        let col_def = col_def_ref;
        state.options_column_rename_input = col_def.header.clone();
        state.options_column_filter_input = col_def.filter.clone().unwrap_or_default();
        // Initialize multi-term filter vector from stored filter (split by '|')
        state.options_column_filter_terms = if state.options_column_filter_input.is_empty() {
            vec![String::new()]
        } else {
            state
                .options_column_filter_input
                .split('|')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        };
        if state.options_column_filter_terms.is_empty() {
            state.options_column_filter_terms.push(String::new());
        }
        state.options_column_ai_context_input = col_def.ai_context.clone().unwrap_or_default();

        match &col_def.validator {
            Some(ColumnValidator::Basic(data_type)) => {
                state.options_validator_type = Some(ValidatorTypeChoice::Basic);
                state.options_basic_type_select = *data_type;
                state.options_link_target_sheet = None;
                state.options_link_target_column_index = None;
                state.options_structure_source_columns = vec![None];
                // Not a structure: ensure key-related ephemeral state cleared
                state.options_existing_structure_key_parent_column = None;
                state.options_structure_key_parent_column_temp = None;
            }
            Some(ColumnValidator::Linked {
                target_sheet_name,
                target_column_index,
            }) => {
                state.options_validator_type = Some(ValidatorTypeChoice::Linked);
                state.options_link_target_sheet = Some(target_sheet_name.clone());
                state.options_link_target_column_index = Some(*target_column_index);
                state.options_basic_type_select = col_def.data_type;
                state.options_structure_source_columns = vec![None];
                state.options_existing_structure_key_parent_column = None;
                state.options_structure_key_parent_column_temp = None;
            }
            Some(ColumnValidator::Structure) => {
                state.options_validator_type = Some(ValidatorTypeChoice::Structure);
                state.options_structure_source_columns = vec![None];
                state.options_link_target_sheet = None;
                state.options_link_target_column_index = None;
                state.options_basic_type_select = col_def.data_type;
                // Always refresh existing structure key selection from authoritative metadata
                let refreshed = if let Some(f) = parent_field_opt.as_ref() {
                    // Use only parent field-level key; do not fall back to column-level
                    f.structure_key_parent_column_index
                } else {
                    registry
                        .get_sheet(target_category, target_sheet)
                        .and_then(|s| s.metadata.as_ref())
                        .and_then(|m| m.columns.get(col_index))
                        .and_then(|c| c.structure_key_parent_column_index)
                };
                debug!(
                    "Popup init: refreshed structure key selection for {:?}/{} col {} -> {:?}",
                    target_category, target_sheet, col_index, refreshed
                );
                state.options_existing_structure_key_parent_column = refreshed;
                // Clear creation-temp field (not used in existing structure editing)
                state.options_structure_key_parent_column_temp = None;
            }
            None => {
                warn!("Column '{}' missing validator during popup init for sheet '{:?}/{}'. Defaulting to Basic/String.", col_def.header, target_category, target_sheet);
                state.options_validator_type = Some(ValidatorTypeChoice::Basic);
                state.options_basic_type_select = col_def.data_type;
                state.options_link_target_sheet = None;
                state.options_link_target_column_index = None;
                state.options_structure_source_columns = vec![None];
                state.options_existing_structure_key_parent_column = None;
                state.options_structure_key_parent_column_temp = None;
            }
        }
    } else {
        error!(
            "Failed to initialize column options popup: Column {} not found for sheet '{:?}/{}'.",
            col_index, target_category, target_sheet
        );
        state.options_column_rename_input.clear();
        state.options_column_filter_input.clear();
        state.options_column_filter_terms = vec![String::new()];
        state.options_column_ai_context_input.clear();
        state.options_validator_type = None;
        state.options_link_target_sheet = None;
        state.options_link_target_column_index = None;
        state.options_existing_structure_key_parent_column = None;
        state.options_structure_key_parent_column_temp = None;
    }
}
