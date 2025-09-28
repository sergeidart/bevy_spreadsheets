// src/sheets/systems/logic/create_sheet.rs
use crate::{
    sheets::{
        definitions::{SheetGridData, SheetMetadata},
        events::{RequestCreateNewSheet, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
        resources::SheetRegistry,
        systems::io::{
            save::save_single_sheet,
            validator, // For name validation
        },
    },
    ui::elements::editor::state::EditorWindowState, // To potentially set as selected
};
use bevy::prelude::*;

pub fn handle_create_new_sheet_request(
    _commands: Commands,
    mut events: EventReader<RequestCreateNewSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut editor_state_opt: Option<ResMut<EditorWindowState>>, // Make optional if direct state change isn't critical path
) {
    for event in events.read() {
        let category = &event.category;
        let desired_name = event.desired_name.trim();

        // Validate name (using existing validator logic if possible, or a new one)
        // For now, using a simple check similar to startup scan validation
        if let Err(e) = validator::validate_derived_sheet_name(desired_name) {
            let msg = format!(
                "Failed to create sheet: Invalid name '{}'. {}",
                desired_name, e
            );
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        // Check if sheet already exists in this category
        if registry.get_sheet(category, desired_name).is_some() {
            let msg = format!(
                "Failed to create sheet: Name '{}' already exists in category '{:?}'.",
                desired_name, category
            );
            warn!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        // Create dummy metadata (0 columns, 0 rows)
        let data_filename = format!("{}.json", desired_name);
        let new_metadata = SheetMetadata::create_generic(
            desired_name.to_string(),
            data_filename,
            0, // 0 columns for a dummy sheet
            category.clone(),
        );

        let new_sheet_data = SheetGridData {
            metadata: Some(new_metadata.clone()),
            grid: Vec::new(), // 0 rows
        };

        // Add to registry
        registry.add_or_replace_sheet(category.clone(), desired_name.to_string(), new_sheet_data);

        info!(
            "Successfully created new sheet '{:?}/{}' in registry.",
            category, desired_name
        );

        // Save the new sheet (creates .json and .meta.json files)
        // save_single_sheet needs an immutable borrow of registry
        let registry_immut = registry.as_ref();
        save_single_sheet(registry_immut, &new_metadata);
        info!("Saved new sheet '{:?}/{}' to disk.", category, desired_name);

        feedback_writer.write(SheetOperationFeedback {
            message: format!("Sheet '{:?}/{}' created.", category, desired_name),
            is_error: false,
        });

        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: category.clone(),
            sheet_name: desired_name.to_string(),
        });

        // Optionally, set this new sheet as selected in the UI
        if let Some(editor_state) = editor_state_opt.as_mut() {
            editor_state.selected_category = category.clone();
            editor_state.selected_sheet_name = Some(desired_name.to_string());
            editor_state.reset_interaction_modes_and_selections(); // Reset modes
            editor_state.force_filter_recalculation = true; // Ensure UI updates
                                                            // Legacy AI config popup removed; no init flag needed
            info!(
                "Set newly created sheet '{:?}/{}' as active.",
                category, desired_name
            );
        }
    }
}
