use crate::sheets::definitions::ColumnValidator;
use crate::sheets::events::{RequestUpdateColumnValidator, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use bevy_egui::egui;

// Build positional vector for a structure row according to ordered headers
#[allow(dead_code)]
fn build_structure_positional_row(
    row: &Vec<String>,
    _headers: &[String],
    source_indices: &[usize],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(source_indices.len());
    for src_idx in source_indices.iter() {
        out.push(row.get(*src_idx).cloned().unwrap_or_default());
    }
    out
}

pub fn show_validator_confirm_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    mut validator_writer: Option<&mut EventWriter<RequestUpdateColumnValidator>>,
    mut feedback_writer: Option<&mut EventWriter<SheetOperationFeedback>>,
) {
    if !state.pending_validator_change_requires_confirmation {
        return;
    }
    let mut open = true;
    egui::Window::new("Confirm Validator Change")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.colored_label(
                egui::Color32::from_rgb(220, 60, 60),
                "This change may overwrite or transform existing column data.",
            );
            if let Some(summary) = &state.pending_validator_new_validator_summary {
                ui.label(format!("Change: {}", summary));
            }
            ui.separator();
            ui.horizontal(|ui_h| {
                if ui_h.button("Confirm").clicked() {
                    // Determine target column info
                    let cat = state.options_column_target_category.clone();
                    let sheet = state.options_column_target_sheet.clone();
                    let col_index = state.options_column_target_index;
                    // Determine if target after confirmation should be Structure or revert (based on flag)
                    let target_is_structure = state.pending_validator_target_is_structure;

                    // Gather and filter source indices (exclude self to avoid circular dependency)
                    let structure_sources: Vec<usize> = state
                        .options_structure_source_columns
                        .iter()
                        .filter_map(|o| *o)
                        .collect(); // allow self column now

                    if !target_is_structure {
                        // Reverting away from structure: keep current text but clear validator (actual flattening handled in system)
                        if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
                            if let Some(meta) = &mut sheet_data.metadata {
                                if col_index < meta.columns.len() {
                                    meta.columns[col_index].validator = None;
                                }
                            }
                        }
                    }

                    if let (Some(vw), true) = (validator_writer.as_deref_mut(), true) {
                        // Capture original validator BEFORE conversion for self inclusion fix
                        let original_self_validator = registry
                            .get_sheet(&cat, &sheet)
                            .and_then(|s| s.metadata.as_ref())
                            .and_then(|m| m.columns.get(col_index))
                            .and_then(|c| c.validator.clone());
                        vw.write(RequestUpdateColumnValidator {
                            category: cat.clone(),
                            sheet_name: sheet.clone(),
                            column_index: col_index,
                            new_validator: if target_is_structure {
                                Some(ColumnValidator::Structure)
                            } else {
                                None
                            },
                            structure_source_columns: if target_is_structure
                                && !structure_sources.is_empty()
                            {
                                Some(structure_sources.clone())
                            } else {
                                None
                            },
                            key_parent_column_index: if target_is_structure {
                                state.options_structure_key_parent_column_temp
                            } else {
                                None
                            },
                            original_self_validator,
                        });
                    }
                    if let Some(fw) = feedback_writer.as_deref_mut() {
                        let msg = if target_is_structure {
                            "Structure validator applied."
                        } else {
                            "Validator change confirmed."
                        };
                        fw.write(SheetOperationFeedback {
                            message: msg.to_string(),
                            is_error: false,
                        });
                    }
                    state.pending_validator_change_requires_confirmation = false;
                    state.pending_validator_new_validator_summary = None;
                    state.pending_validator_target_is_structure = false;
                    state.show_column_options_popup = false; // close parent popup
                }
                if ui_h.button("Cancel").clicked() {
                    state.pending_validator_change_requires_confirmation = false;
                    state.pending_validator_new_validator_summary = None;
                    state.pending_validator_target_is_structure = false;
                }
            });
        });
    if !open {
        // user closed with X
        state.pending_validator_change_requires_confirmation = false;
    }
}
