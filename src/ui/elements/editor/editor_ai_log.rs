// src/ui/elements/editor/editor_ai_log.rs
use super::state::EditorWindowState;
use bevy_egui::egui::{self, Color32, RichText};

// Draws persistent bottom panel if enabled
pub(super) fn show_ai_output_log_bottom(ctx: &egui::Context, state: &mut EditorWindowState) {
    if !state.ai_output_panel_visible {
        return;
    }
    let panel_id = egui::Id::new("ai_output_bottom_panel");
    egui::TopBottomPanel::bottom(panel_id)
        .resizable(true)
        .default_height(200.0)
        .show_separator_line(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("AI Call Log");
                ui.add_space(8.0);
                if ui.button("Copy All").clicked() {
                    let combined = build_combined_log_text(state);
                    ctx.copy_text(combined);
                }
                if ui.button("Clear").clicked() {
                    state.ai_call_log.clear();
                    state.ai_raw_output_display.clear();
                }
                // Close is controlled by the bottom bar toggle; no close button here
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("ai_call_log_scroll_area")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    // Display entries in chronological order (newest first)
                    for (idx, entry) in state.ai_call_log.iter().enumerate() {
                        if idx > 0 {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);
                        }
                        
                        // Status line
                        let status_color = if entry.is_error {
                            Color32::from_rgb(255, 100, 100)
                        } else {
                            Color32::from_rgb(100, 200, 255)
                        };
                        ui.label(RichText::new(&entry.status).color(status_color).strong());
                        
                        // Response section
                        if let Some(response) = &entry.response {
                            ui.add_space(2.0);
                            ui.label(RichText::new("Currently Received:").color(Color32::from_rgb(150, 255, 150)));
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                egui::ScrollArea::vertical()
                                    .id_salt(format!("response_{}", idx))
                                    .max_height(150.0)
                                    .show(ui, |ui| {
                                        let mut response_clone = response.clone();
                                        ui.add(
                                            egui::TextEdit::multiline(&mut response_clone)
                                                .font(egui::TextStyle::Monospace)
                                                .interactive(false)
                                                .desired_width(ui.available_width())
                                        );
                                    });
                            });
                        }
                        
                        // Request section
                        if let Some(request) = &entry.request {
                            ui.add_space(2.0);
                            ui.label(RichText::new("What was sent:").color(Color32::from_rgb(255, 200, 100)));
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                egui::ScrollArea::vertical()
                                    .id_salt(format!("request_{}", idx))
                                    .max_height(150.0)
                                    .show(ui, |ui| {
                                        let mut request_clone = request.clone();
                                        ui.add(
                                            egui::TextEdit::multiline(&mut request_clone)
                                                .font(egui::TextStyle::Monospace)
                                                .interactive(false)
                                                .desired_width(ui.available_width())
                                        );
                                    });
                            });
                        }
                    }
                    
                    // Keep backward compatibility - show old raw display if call log is empty
                    if state.ai_call_log.is_empty() && !state.ai_raw_output_display.is_empty() {
                        let mut display_text_clone = state.ai_raw_output_display.clone();
                        ui.add_sized(
                            ui.available_size(),
                            egui::TextEdit::multiline(&mut display_text_clone)
                                .font(egui::TextStyle::Monospace)
                                .interactive(false)
                                .desired_width(f32::INFINITY),
                        );
                    }
                });
        });
}

fn build_combined_log_text(state: &EditorWindowState) -> String {
    let mut result = String::new();
    for (idx, entry) in state.ai_call_log.iter().enumerate() {
        if idx > 0 {
            result.push_str("\n\n========================================\n\n");
        }
        result.push_str(&format!("Status: {}\n", entry.status));
        if let Some(response) = &entry.response {
            result.push_str("\nCurrently Received:\n");
            result.push_str(response);
            result.push('\n');
        }
        if let Some(request) = &entry.request {
            result.push_str("\nWhat was sent:\n");
            result.push_str(request);
            result.push('\n');
        }
    }
    result
}
