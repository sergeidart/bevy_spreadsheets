// src/ui/elements/top_panel/quick_copy_bar.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::ui::elements::editor::state::EditorWindowState;
use crate::visual_copier::{
    resources::VisualCopierManager,
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
    },
};
use super::truncate_path_string; 

// MODIFIED: Helper struct generic over borrow lifetime 'a, and EventWriter world lifetime 'w
pub(super) struct QuickCopyEventWriters<'a, 'w> {
    pub pick_folder_writer: &'a mut EventWriter<'w, PickFolderRequest>,
    pub queue_top_panel_copy_writer: &'a mut EventWriter<'w, QueueTopPanelCopyEvent>,
    pub reverse_folders_writer: &'a mut EventWriter<'w, ReverseTopPanelFoldersEvent>,
    pub state_changed_writer: &'a mut EventWriter<'w, VisualCopierStateChanged>,
}

// MODIFIED: Function generic over 'a and 'w
pub(super) fn show_quick_copy_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &EditorWindowState, 
    copier_manager: &mut VisualCopierManager, 
    mut event_writers: QuickCopyEventWriters<'a, 'w>,
) {
    if state.show_quick_copy_bar {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                const MAX_PATH_DISPLAY_WIDTH: f32 = 250.0; 
                ui.label("Quick Copy:");

                if ui.button("FROM").on_hover_text("Select source folder").clicked() {
                    event_writers.pick_folder_writer.send(PickFolderRequest {
                        for_task_id: None,
                        is_start_folder: true,
                    });
                }
                let from_path_str = copier_manager
                    .top_panel_from_folder
                    .as_ref()
                    .map_or_else(|| "None".to_string(), |p| p.display().to_string());
                ui.label(truncate_path_string(&from_path_str, MAX_PATH_DISPLAY_WIDTH, ui))
                    .on_hover_text(&from_path_str);

                if ui.button("TO").on_hover_text("Select destination folder").clicked() {
                    event_writers.pick_folder_writer.send(PickFolderRequest {
                        for_task_id: None,
                        is_start_folder: false,
                    });
                }
                let to_path_str = copier_manager
                    .top_panel_to_folder
                    .as_ref()
                    .map_or_else(|| "None".to_string(), |p| p.display().to_string());
                ui.label(truncate_path_string(&to_path_str, MAX_PATH_DISPLAY_WIDTH, ui))
                    .on_hover_text(&to_path_str);

                if ui.button("Swap ↔").clicked() {
                    event_writers.reverse_folders_writer.send(ReverseTopPanelFoldersEvent);
                }

                let can_quick_copy = copier_manager.top_panel_from_folder.is_some()
                    && copier_manager.top_panel_to_folder.is_some();
                if ui.add_enabled(can_quick_copy, egui::Button::new("COPY")).clicked() {
                    event_writers.queue_top_panel_copy_writer.send(QueueTopPanelCopyEvent);
                }
                ui.label(&copier_manager.top_panel_copy_status);
                ui.separator();
                let checkbox_response = ui
                    .checkbox(&mut copier_manager.copy_top_panel_on_exit, "Copy on Exit")
                    .on_hover_text(
                        "If checked, performs this Quick Copy operation synchronously just before the application closes via 'App Exit'.",
                    );
                if checkbox_response.changed() {
                    event_writers.state_changed_writer.send(VisualCopierStateChanged);
                    info!(
                        "'Copy on Exit' checkbox changed to: {}",
                        copier_manager.copy_top_panel_on_exit
                    );
                }
            });
        });
    }
}
