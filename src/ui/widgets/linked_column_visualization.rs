// src/ui/widgets/linked_column_visualization.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Id, Response, Sense, TextStyle, Ui, Vec2};

/// Input parameters for the visualization function.
pub(super) struct LinkedEditorVisParams<'a> {
    pub ui: &'a mut egui::Ui,
    pub id: egui::Id,
    pub input_text: &'a mut String, // Mutable reference to the text being edited
    pub original_value: &'a str,    // The original, persistent value of the cell
    pub filtered_suggestions: &'a [String],
    pub show_popup: bool,
    pub validation_error: Option<String>, // Contains error message if invalid
    pub link_error: Option<String>,       // Contains error message if link target is invalid
}

/// Output parameters from the visualization function.
#[derive(Default)]
pub(super) struct LinkedEditorVisOutput {
    /// The response from the main TextEdit widget.
    pub text_edit_response: Option<Response>,
    /// The value of the suggestion that was clicked, if any.
    pub clicked_suggestion: Option<String>,
}

/// Renders the UI for the linked column editor (TextEdit + Suggestion Popup).
///
/// # Arguments
///
/// * `params` - A struct containing all necessary input parameters.
///
/// # Returns
///
/// * `LinkedEditorVisOutput` - A struct containing the TextEdit response and any clicked suggestion.
pub(super) fn show_linked_editor_ui(params: LinkedEditorVisParams<'_>) -> LinkedEditorVisOutput {
    let mut output = LinkedEditorVisOutput::default();
    let text_edit_id = params.id.with("ac_text_edit");
    let popup_id = params.id.with("ac_popup");

    // --- Handle Link Error Display ---
    if let Some(err_msg) = &params.link_error {
        output.text_edit_response = Some(
            params
                .ui
                .add_sized(
                    params.ui.available_size(),
                    egui::Label::new(egui::RichText::new("Link Error").color(egui::Color32::RED)),
                )
                .on_hover_text(err_msg.clone()), // Clone error message for hover text
        );
        // No further UI needed if link is broken
        return output;
    }

    // --- Main Widget Logic (TextEdit + Popup) ---
    let mut text_edit_response: Option<Response> = None;
    params.ui.vertical(|ui| {
        let te_response = ui.add(
            egui::TextEdit::singleline(params.input_text) // Use input_text directly here
                .id(text_edit_id)
                .desired_width(f32::INFINITY)
                .hint_text("--Type or select--"), // Updated hint text
        );
        text_edit_response = Some(te_response);
    });
    let text_edit_response = text_edit_response.unwrap(); // Should always be Some
    output.text_edit_response = Some(text_edit_response.clone()); // Clone for output

    // --- Suggestion Popup ---
    if params.show_popup && !params.filtered_suggestions.is_empty() {
        let area = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(text_edit_response.rect.left_bottom() + Vec2::new(0.0, 4.0)); // Position below TextEdit

        area.show(params.ui.ctx(), |ui_area| {
            // Set width to match TextEdit
            ui_area.set_min_width(text_edit_response.rect.width());
            ui_area.set_max_width(text_edit_response.rect.width());

            let frame = egui::Frame::popup(ui_area.style()); // Use popup frame style
            frame.show(ui_area, |ui_frame| {
                egui::ScrollArea::vertical()
                    .max_height(200.0) // Limit popup height
                    .show(ui_frame, |ui_scroll| {
                        ui_scroll.vertical(|ui_vert| {
                            for suggestion in params.filtered_suggestions {
                                // Check if the suggestion is clicked
                                if ui_vert
                                    .selectable_label(*params.input_text == *suggestion, suggestion)
                                    .clicked()
                                {
                                    // Capture the clicked value
                                    output.clicked_suggestion = Some(suggestion.clone());
                                    // Update input_text immediately to match the clicked suggestion
                                    *params.input_text = suggestion.clone();
                                    // Store the updated input_text (the suggestion) back into memory
                                    ui_vert.memory_mut(|mem| {
                                        mem.data.insert_temp(text_edit_id, params.input_text.clone())
                                    });
                                    // Request focus removal to close the popup and commit
                                    ui_vert.memory_mut(|mem| mem.request_focus(Id::NULL));
                                    break; // Exit the inner loop since a selection was made
                                }
                            }
                        });
                    });
            });
        }); // End area.show
    } // End if show_popup

    // --- Draw Error Background ---
    let desired_height = params.ui.style().spacing.interact_size.y; // Standard interaction height
    let widget_rect = text_edit_response.rect;
    // Calculate background rect ensuring it covers the desired height
    let bg_rect = egui::Rect::from_min_size(
        widget_rect.min,
        egui::vec2(widget_rect.width(), desired_height.max(widget_rect.height())),
    );

    if let Some(validation_err_msg) = &params.validation_error {
        let error_bg_color = egui::Color32::from_rgb(60, 10, 10); // Dark red background
        params.ui.painter().add(egui::Shape::rect_filled(
            bg_rect,
            params.ui.style().visuals.widgets.inactive.rounding(),
            error_bg_color,
        ));

        // Add informative hover text for the validation error
        text_edit_response.on_hover_text(validation_err_msg.clone());
    } else if output.clicked_suggestion.is_some()
        || (text_edit_response.lost_focus() && *params.input_text != params.original_value)
    {
        // Add hover text showing original value if a change was made (suggestion clicked or focus lost)
        // but there's no validation error.
        text_edit_response.on_hover_text(format!("Original: '{}'", params.original_value));
    }

    output
}
