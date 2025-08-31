// src/ui/elements/editor/editor_ai_log.rs
use bevy_egui::egui;
use super::state::EditorWindowState;

// Draws persistent bottom panel if enabled
pub(super) fn show_ai_output_log_bottom(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
) {
    if !state.ai_output_panel_visible { return; }
    let panel_id = egui::Id::new("ai_output_bottom_panel");
    egui::TopBottomPanel::bottom(panel_id)
        .resizable(true)
        .default_height(140.0)
        .show_separator_line(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("AI Output / Log");
                ui.add_space(8.0);
                if ui.button("Copy Raw").clicked() {
                    ctx.copy_text(state.ai_raw_output_display.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖ Close").on_hover_text("Hide AI Output Panel (will reappear on next AI run)").clicked() {
                        state.ai_output_panel_visible = false;
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("ai_raw_output_log_scroll_area")
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
        });
}