// src/ui/widgets/linked_column_visualization.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Response};

/// Adds the TextEdit widget for a linked column and returns its response.
/// Makes the TextEdit frameless so the background from the containing frame shows through.
pub(super) fn add_linked_text_edit(
    ui: &mut egui::Ui,
    id: egui::Id,
    input_text: &mut String,      // Mutable text buffer
    _link_error: &Option<String>, // Error state is handled by the containing frame's background
    _original_value: &str,        // Hover text is handled by the containing frame
) -> Response {
    let text_edit_id = id.with("ac_text_edit");

    // 1️⃣ Build the TextEdit widget, making it frameless
    let text_edit_widget = egui::TextEdit::singleline(input_text)
        .id(text_edit_id)
        // .desired_width(f32::INFINITY) // Rely on add_sized in the caller
        .hint_text("--Type or select--")
        .frame(false); // <-- MAKE TEXTEDIT FRAMELESS

    // Use add_sized to fill available space provided by the calling frame/allocator
    let response = ui.add_sized(ui.available_size(), text_edit_widget);

    // Hover text and background color are handled by the caller (edit_cell_widget)

    response
}
