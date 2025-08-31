// src/sheets/systems/logic/add_row.rs
use crate::sheets::{
    definitions::SheetMetadata, 
    events::{AddSheetRowRequest, SheetOperationFeedback, SheetDataModifiedInRegistryEvent}, 
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use crate::ui::elements::editor::state::EditorWindowState;

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
        let mut is_virtual = false;
    if let Some(state) = editor_state.as_ref() {
            if let Some(vctx) = state.virtual_structure_stack.last() {
                sheet_name = vctx.virtual_sheet_name.clone();
                category = vctx.parent.parent_category.clone();
                is_virtual = true;
            }
        }

        let mut metadata_cache: Option<SheetMetadata> = None;

    if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.columns.len();
                // Unified behavior: always insert at top for consistency
                sheet_data.grid.insert(0, vec![String::new(); num_cols]);

                let msg = format!("Added new row at the top of sheet '{:?}/{}'.", category, sheet_name);
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
                        .filter(|(cat_opt, s_name, _)| cat_opt == &category && s_name == &sheet_name)
                        .cloned()
                        .collect();
                    for k in keys_to_remove { state_mut.filtered_row_indices_cache.remove(&k); }
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
                let msg = format!("Cannot add row: Sheet '{:?}/{}' not found in registry.", category, sheet_name);
            warn!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
        }

        if let Some(meta_to_save) = metadata_cache {
            info!("Row added to '{:?}/{}', triggering immediate save.", category, sheet_name);
            let registry_immut = registry.as_ref();
            save_single_sheet(registry_immut, &meta_to_save); 
        }
    }
}