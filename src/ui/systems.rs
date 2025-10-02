// UI-centric systems (event forwarding, feedback display, structure key selection logic).
// AI processing systems have been migrated to `sheets::systems::ai`.
use crate::{
    sheets::events::SheetOperationFeedback,
    sheets::resources::SheetRegistry,
    sheets::systems::io::save::save_single_sheet,
    ui::{elements::editor::state::EditorWindowState, UiFeedbackState},
};
use bevy::prelude::*;
use std::any;

// ----------------------------------
// Generic helper to forward spawned events via ECS entities
// ----------------------------------
#[derive(Component)]
pub struct SendEvent<E: Event> {
    pub event: E,
}

pub fn forward_events<E: Event + Clone + std::fmt::Debug>(
    mut commands: Commands,
    mut writer: EventWriter<E>,
    query: Query<(Entity, &SendEvent<E>)>,
    mut event_type_name: Local<String>,
) {
    if event_type_name.is_empty() {
        *event_type_name = any::type_name::<E>()
            .rsplit("::")
            .next()
            .unwrap_or("UnknownEvent")
            .to_string();
    }
    for (entity, send_event_component) in query.iter() {
        writer.write(send_event_component.event.clone());
        commands.entity(entity).remove::<SendEvent<E>>();
        commands.entity(entity).despawn();
    }
}

// ----------------------------------
// Sheet operation feedback -> UI state
// ----------------------------------
pub fn handle_ui_feedback(
    mut feedback_events: EventReader<SheetOperationFeedback>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
    mut state: ResMut<EditorWindowState>,
) {
    let mut last_message = None;
    for event in feedback_events.read() {
        last_message = Some((event.message.clone(), event.is_error));
        if !state.ai_raw_output_display.is_empty() {
            state.ai_raw_output_display.push('\n');
        }
        state.ai_raw_output_display.push_str(&event.message);
        if event.is_error {
            state.ai_output_panel_visible = true;
        }
        if !event.is_error {
            // Stop on first success so errors from earlier events still show until a success arrives
            break;
        }
    }
    if let Some((msg, is_error)) = last_message {
        ui_feedback_state.last_message = msg;
        ui_feedback_state.is_error = is_error;
    }
}

// ----------------------------------
// Clear feedback when switching sheet selection
// ----------------------------------
pub fn clear_ui_feedback_on_sheet_change(
    state: Res<EditorWindowState>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
    mut last_selection: Local<Option<(Option<String>, Option<String>)>>,
) {
    let current_sel = (
        state.selected_category.clone(),
        state.selected_sheet_name.clone(),
    );
    if let Some(prev) = last_selection.as_ref() {
        if prev != &current_sel {
            ui_feedback_state.last_message.clear();
            ui_feedback_state.is_error = false;
        }
    }
    *last_selection = Some(current_sel);
}

// ----------------------------------
// Apply user selection of key column inside a structure field
// ----------------------------------
pub fn apply_pending_structure_key_selection(
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
) {
    if let Some((cat, sheet, structure_col_index, new_key_opt)) =
        state.pending_structure_key_apply.take()
    {
        let mut root_parent_link: Option<crate::sheets::definitions::StructureParentLink> = None;
        let mut changed = false;
        let mut is_virtual = false;
        if let Some(sheet_data) = registry.get_sheet(&cat, &sheet) {
            if let Some(meta_ro) = &sheet_data.metadata {
                is_virtual = meta_ro.structure_parent.is_some();
                root_parent_link = meta_ro.structure_parent.clone();
            }
        }
        if is_virtual {
            if let Some(parent_link) = &root_parent_link {
                if let Some(parent_sheet) =
                    registry.get_sheet_mut(&parent_link.parent_category, &parent_link.parent_sheet)
                {
                    if let Some(parent_meta) = &mut parent_sheet.metadata {
                        if let Some(parent_col) =
                            parent_meta.columns.get_mut(parent_link.parent_column_index)
                        {
                            if let Some(fields) = parent_col.structure_schema.as_mut() {
                                if let Some(field) = fields.get_mut(structure_col_index) {
                                    if field.structure_key_parent_column_index != new_key_opt {
                                        changed = true;
                                    }
                                    field.structure_key_parent_column_index = new_key_opt;
                                }
                            }
                        }
                        if changed {
                            let meta_clone = parent_meta.clone();
                            save_single_sheet(registry.as_ref(), &meta_clone);
                        }
                    }
                }
            }
            if let Some(vsheet) = registry.get_sheet_mut(&cat, &sheet) {
                if let Some(vmeta) = &mut vsheet.metadata {
                    if let Some(vcol) = vmeta.columns.get_mut(structure_col_index) {
                        vcol.structure_key_parent_column_index = new_key_opt;
                    }
                }
            }
        } else {
            if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
                if let Some(meta) = &mut sheet_data.metadata {
                    if let Some(col) = meta.columns.get_mut(structure_col_index) {
                        if col.structure_key_parent_column_index != new_key_opt {
                            changed = true;
                        }
                        col.structure_key_parent_column_index = new_key_opt;
                        col.structure_ancestor_key_parent_column_indices = Some(Vec::new());
                    }
                }
            }
        }
        let mut collected: Vec<usize> = Vec::new();
        let mut current_parent = root_parent_link;
        let mut safety = 0;
        while let Some(parent_link) = current_parent.clone() {
            if safety > 32 {
                break;
            }
            safety += 1;
            if let Some(parent_sheet) =
                registry.get_sheet(&parent_link.parent_category, &parent_link.parent_sheet)
            {
                if let Some(parent_meta) = &parent_sheet.metadata {
                    if let Some(parent_col) =
                        parent_meta.columns.get(parent_link.parent_column_index)
                    {
                        if let Some(kidx) = parent_col.structure_key_parent_column_index {
                            collected.push(kidx);
                        }
                    }
                    current_parent = parent_meta.structure_parent.clone();
                    continue;
                }
            }
            break;
        }
        collected.reverse();
        if !is_virtual {
            if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
                if let Some(meta) = &mut sheet_data.metadata {
                    if let Some(col) = meta.columns.get_mut(structure_col_index) {
                        let existing = col
                            .structure_ancestor_key_parent_column_indices
                            .clone()
                            .unwrap_or_default();
                        if existing != collected {
                            changed = true;
                        }
                        col.structure_ancestor_key_parent_column_indices = Some(collected);
                    }
                    if changed {
                        let meta_clone = meta.clone();
                        save_single_sheet(registry.as_ref(), &meta_clone);
                    }
                }
            }
        }
    }
}
