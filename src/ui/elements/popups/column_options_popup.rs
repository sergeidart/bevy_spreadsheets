// src/ui/elements/popups/column_options_popup.rs
use crate::ui::elements::editor::state::EditorWindowState;
use crate::{
    sheets::{
        database::daemon_client::DaemonClient,
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

pub fn show_column_options_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    column_validator_writer: &mut EventWriter<RequestUpdateColumnValidator>,
    registry: &mut SheetRegistry,
    daemon_client: &DaemonClient,
) {
    if !state.show_column_options_popup {
        return;
    }
    if state.column_options_popup_needs_init {
        initialize_popup_state(state, registry);
        state.column_options_popup_needs_init = false;
    }
    
    // Track the previously selected target sheet before rendering UI
    let prev_target_sheet = state.options_link_target_sheet.clone();
    
    let ui_result = {
        let registry_immut = &*registry;
        show_column_options_window_ui(ctx, state, registry_immut)
    };
    
    // Check if the target sheet selection changed during UI rendering
    if state.options_link_target_sheet != prev_target_sheet {
        if let Some(target_sheet_name) = &state.options_link_target_sheet {
            load_target_sheet_if_needed(
                registry,
                daemon_client,
                target_sheet_name,
                &state.options_column_target_category,
            );
        }
    }
    let mut needs_manual_save = false;
    let mut actions_ok = true;
    let mut non_event_change_occurred = false;
    if ui_result.apply_clicked {
        let category = &state.options_column_target_category;
        let sheet_name = &state.options_column_target_sheet;
        let col_index = state.options_column_target_index;
        let mut rename_sent = false;
        let mut validator_sent = false;
        let (current_name, current_filter, current_context, current_validator, current_hidden) = {
            let maybe_col_def = registry
                .get_sheet(category, sheet_name)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.columns.get(col_index));
            if let Some(col_def) = maybe_col_def {
                let ui_name = col_def
                    .display_header
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| col_def.header.clone());
                (
                    Some(ui_name),
                    col_def.filter.clone(),
                    col_def.ai_context.clone(),
                    col_def.validator.clone(),
                    col_def.hidden,
                )
            } else {
                (None, None, None, None, false)
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
            debug!("Column rename check: new_name='{}', current_name={:?}", new_name_trimmed, current_name);
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
                                let comp = c
                                    .display_header
                                    .as_ref()
                                    .map(|s| s.as_str())
                                    .unwrap_or(c.header.as_str());
                                i != col_index && !c.deleted && comp.eq_ignore_ascii_case(new_name_trimmed)
                            })
                        });
                    debug!("Duplicate check result: is_duplicate={}", is_duplicate);
                    if !is_duplicate {
                        info!("Sending column rename request: {} -> {}", current_name.as_ref().unwrap_or(&"<none>".to_string()), new_name_trimmed);
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
            } else {
                debug!("Column rename skipped: name unchanged");
            }
        }
        if actions_ok {
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
                Some(joined_terms.clone())
            };
            let context_trimmed = state.options_column_ai_context_input.trim().to_string();
            // Store None in memory when empty, but pass an explicit empty string to DB update to clear persisted value
            let context_to_store: Option<String> = if context_trimmed.is_empty() {
                None
            } else {
                Some(context_trimmed.clone())
            };

            let filter_changed = current_filter != filter_to_store;
            let context_changed = current_context != context_to_store;
            let hidden_changed = current_hidden != state.options_column_hidden_input;

            if filter_changed || context_changed || hidden_changed {
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
                                // Persist filter to DB if this is DB-backed
                                if meta.category.is_some() {
                                    if let Some(cat) = category {
                                        // Use sheet_name as the table name (not data_filename which has .json extension)
                                        let table_name = &meta.sheet_name;
                                        // Use centralized helper which opens/creates DB and persists metadata
                                        if let Err(e) = crate::sheets::database::persist_column_metadata(
                                            cat,
                                            table_name,
                                            col_index,
                                            // Pass empty string when clearing to force DB NULL
                                            if let Some(s) = col_def.filter.as_deref() { Some(s) } else { Some("") },
                                            None,
                                            None,
                                            None,
                                            daemon_client,
                                        ) {
                                            error!("Persist column metadata (filter) failed: {}", e);
                                        }
                                    }
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
                                // Persist AI context to DB if this is DB-backed
                                if meta.category.is_some() {
                                    if let Some(cat) = category {
                                        // Use sheet_name as the table name (not data_filename which has .json extension)
                                        let table_name = &meta.sheet_name;
                                        // Use centralized helper to persist AI context
                                        if let Err(e) = crate::sheets::database::persist_column_metadata(
                                            cat,
                                            table_name,
                                            col_index,
                                            None,
                                            // Pass empty string when clearing to force DB NULL
                                            if let Some(s) = col_def.ai_context.as_deref() { Some(s) } else { Some("") },
                                            None,
                                            None,
                                            daemon_client,
                                        ) {
                                            error!("Persist column metadata (AI context) failed: {}", e);
                                        }
                                    }
                                }
                            }
                            if hidden_changed {
                                info!(
                                    "Updating hidden flag for col {} of '{:?}/{}': {} -> {}.",
                                    col_index + 1,
                                    category,
                                    sheet_name,
                                    current_hidden,
                                    state.options_column_hidden_input
                                );
                                col_def.hidden = state.options_column_hidden_input;
                                // Persist hidden flag to DB if this is DB-backed
                                if meta.category.is_some() {
                                    if let Some(cat) = category {
                                        let table_name = &meta.sheet_name;
                                        if let Err(e) = crate::sheets::database::persist_column_metadata(
                                            cat,
                                            table_name,
                                            col_index,
                                            None,
                                            None,
                                            None,
                                            Some(col_def.hidden),
                                            daemon_client,
                                        ) {
                                            error!("Persist column metadata (hidden) failed: {}", e);
                                        }
                                    }
                                }
                            }
                        } else {
                            warn!("Filter/Context/Hidden update failed: Index out of bounds.");
                            actions_ok = false;
                        }
                    } else {
                        warn!("Filter/Context/Hidden update failed: Metadata missing.");
                        actions_ok = false;
                    }
                } else {
                    warn!("Filter/Context/Hidden update failed: Sheet not found.");
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
                display_header: None,
                validator: f.validator.clone(),
                data_type: f.data_type,
                filter: f.filter.clone(),
                ai_context: f.ai_context.clone(),
                ai_enable_row_generation: f.ai_enable_row_generation,
                ai_include_in_send: f.ai_include_in_send,
                deleted: false,
                hidden: false, // Synthesized from parent, not a technical column
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
        state.options_column_rename_input = col_def
            .display_header
            .as_ref()
            .cloned()
            .unwrap_or_else(|| col_def.header.clone());
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
        // Initialize hidden checkbox from column definition
        state.options_column_hidden_input = col_def.hidden;

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

/// Loads a target sheet from the database if it's not already loaded in the registry.
/// This ensures that when a user selects a target sheet for a linked column, 
/// the sheet's structure and metadata are immediately available for selecting target columns.
fn load_target_sheet_if_needed(
    registry: &mut SheetRegistry,
    daemon_client: &DaemonClient,
    target_sheet_name: &str,
    category: &Option<String>,
) {
    // Check if target sheet needs loading (doesn't exist or is a stub/empty)
    let needs_load = registry.get_sheet(category, target_sheet_name)
        .map(|sheet| sheet.grid.is_empty() || sheet.metadata.is_none())
        .unwrap_or(true);
    
    if !needs_load {
        debug!("Target sheet '{}' already loaded with data", target_sheet_name);
        return;
    }
    
    // Get the database path for the current category
    let Some(cat_str) = category.as_ref() else {
        warn!("Cannot load target sheet '{}': no category selected", target_sheet_name);
        return;
    };
    
    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", cat_str));
    
    if !db_path.exists() {
        warn!("Database file not found for loading target sheet '{}': {:?}", target_sheet_name, db_path);
        return;
    }
    
    // Load the target sheet from the database
    info!("Loading target sheet '{}' for linked column selection", target_sheet_name);
    match rusqlite::Connection::open(&db_path) {
        Ok(conn) => {
            match crate::sheets::database::reader::DbReader::read_sheet(
                &conn, 
                target_sheet_name, 
                daemon_client, 
                Some(cat_str)
            ) {
                Ok(sheet_data) => {
                    info!(
                        "Loaded {} rows for target sheet '{}' with {} columns", 
                        sheet_data.grid.len(),
                        target_sheet_name,
                        sheet_data.metadata.as_ref().map(|m| m.columns.len()).unwrap_or(0)
                    );
                    registry.add_or_replace_sheet(
                        category.clone(),
                        target_sheet_name.to_string(),
                        sheet_data,
                    );
                }
                Err(e) => {
                    error!("Failed to load target sheet '{}': {}", target_sheet_name, e);
                }
            }
        }
        Err(e) => {
            error!("Failed to open database for loading target sheet '{}': {}", target_sheet_name, e);
        }
    }
}
