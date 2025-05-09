// src/ui/elements/editor/editor_ai_log.rs
use bevy::prelude::*;
use bevy_egui::egui;
use super::state::EditorWindowState;

pub(super) fn show_ai_output_log(
    ui: &mut egui::Ui,
    state: &EditorWindowState,
) {
    ui.separator();
    ui.strong("AI Output / Log:");
    egui::ScrollArea::vertical()
        .id_salt("ai_raw_output_log_scroll_area")
        .max_height(100.0)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let mut display_text_clone = state.ai_raw_output_display.clone();
            ui.add_sized(
                ui.available_size(),
                egui::TextEdit::multiline(&mut display_text_clone)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false)
                    .desired_width(f32::INFINITY)
            );
        });
}