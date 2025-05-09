// src/ui/elements/top_panel/mod.rs
use bevy::prelude::*;
use bevy_egui::egui;

// Event imports for clarity, matching submodule needs
use crate::sheets::events::{
    AddSheetRowRequest, RequestAddColumn,
    RequestInitiateFileUpload,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
// Import the SheetEventWriters SystemParam struct
use crate::ui::elements::editor::main_editor::SheetEventWriters;
use crate::visual_copier::{
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, RequestAppExit, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
    },
    resources::VisualCopierManager,
};

// Declare sub-modules
mod sheet_management_bar;
mod quick_copy_bar;
mod sheet_interaction_modes;
pub mod controls {
    pub mod delete_mode_panel;
}

// Re-export the main function that will be called by main_editor.rs
pub use self::orchestrator::show_top_panel_orchestrator;

pub(super) fn truncate_path_string(path_str: &str, max_width_pixels: f32, ui: &egui::Ui) -> String {
    if path_str.is_empty() {
        return "".to_string();
    }
    let font_id_val = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            path_str.to_string(),
            font_id_val.clone(),
            egui::Color32::PLACEHOLDER,
        )
    });

    if galley.size().x <= max_width_pixels {
        return path_str.to_string();
    }

    let ellipsis = "...";
    let ellipsis_width = ui.fonts(|f| {
        f.layout_no_wrap(
            ellipsis.to_string(),
            font_id_val.clone(),
            egui::Color32::PLACEHOLDER,
        )
    })
    .size()
    .x;

    if ellipsis_width > max_width_pixels {
        let mut fitting_ellipsis = String::new();
        let mut current_ellipsis_width = 0.0;
        for c in ellipsis.chars() {
            let char_s = c.to_string();
            let char_w = ui.fonts(|f| {
                f.layout_no_wrap(char_s.clone(), font_id_val.clone(), egui::Color32::PLACEHOLDER)
            })
            .size()
            .x;
            if current_ellipsis_width + char_w <= max_width_pixels {
                fitting_ellipsis.push(c);
                current_ellipsis_width += char_w;
            } else {
                break;
            }
        }
        return fitting_ellipsis;
    }

    let mut truncated_len = 0;
    let mut current_width = 0.0;

    for (idx, char_instance) in path_str.char_indices() {
        let char_s = match path_str.get(idx..idx + char_instance.len_utf8()) {
            Some(s) => s,
            None => break,
        };
        let char_w = ui.fonts(|f| {
            f.layout_no_wrap(char_s.to_string(), font_id_val.clone(), egui::Color32::PLACEHOLDER)
        })
        .size()
        .x;

        if current_width + char_w + ellipsis_width > max_width_pixels {
            break;
        }
        current_width += char_w;
        truncated_len = idx + char_instance.len_utf8();
    }

    if truncated_len == 0 && !path_str.is_empty() {
        return ellipsis.to_string();
    } else if path_str.is_empty() {
        return "".to_string();
    }

    format!("{}{}", &path_str[..truncated_len], ellipsis)
}

mod orchestrator {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    pub fn show_top_panel_orchestrator<'w>(
        ui: &mut egui::Ui,
        state: &mut EditorWindowState,
        registry: &SheetRegistry,
        sheet_writers: &mut SheetEventWriters<'w>, // Received as &mut
        mut copier_manager: ResMut<VisualCopierManager>,
        // MODIFIED: Make these EventWriter parameters mutable
        mut pick_folder_writer: EventWriter<'w, PickFolderRequest>,
        mut queue_top_panel_copy_writer: EventWriter<'w, QueueTopPanelCopyEvent>,
        mut reverse_folders_writer: EventWriter<'w, ReverseTopPanelFoldersEvent>,
        mut request_app_exit_writer: EventWriter<'w, RequestAppExit>,
        mut state_changed_writer: EventWriter<'w, VisualCopierStateChanged>,
    ) {
        egui::TopBottomPanel::top("main_top_controls_panel_refactored")
            .show_inside(ui, |ui| {
                ui.horizontal(|ui_h| {
                    sheet_management_bar::show_sheet_management_controls(
                        ui_h,
                        state,
                        registry,
                        sheet_management_bar::SheetManagementEventWriters {
                             upload_req_writer: &mut sheet_writers.upload_req,
                             // MODIFIED: Pass &mut to the local mutable EventWriter
                             request_app_exit_writer: &mut request_app_exit_writer,
                        }
                    );
                });

                quick_copy_bar::show_quick_copy_controls(
                    ui,
                    state,
                    &mut copier_manager,
                    quick_copy_bar::QuickCopyEventWriters {
                        // MODIFIED: Pass &mut to local mutable EventWriters
                        pick_folder_writer: &mut pick_folder_writer,
                        queue_top_panel_copy_writer: &mut queue_top_panel_copy_writer,
                        reverse_folders_writer: &mut reverse_folders_writer,
                        state_changed_writer: &mut state_changed_writer,
                    },
                );
                ui.separator();

                ui.horizontal(|ui_h| {
                    sheet_interaction_modes::show_sheet_interaction_mode_buttons(
                        ui_h,
                        state,
                        sheet_interaction_modes::InteractionModeEventWriters {
                            add_row_event_writer: &mut sheet_writers.add_row,
                            add_column_event_writer: &mut sheet_writers.add_column,
                        }
                    );
                });
                ui.add_space(5.0);
            });
    }
}