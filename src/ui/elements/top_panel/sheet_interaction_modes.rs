// src/ui/elements/top_panel/sheet_interaction_modes.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::{AddSheetRowRequest, RequestAddColumn};
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, SheetInteractionState};
use crate::sheets::resources::SheetRegistry;

// Assuming this struct definition is intended to hold mutable references
pub(super) struct InteractionModeEventWriters<'a, 'w> {
    pub add_row_event_writer: &'a mut EventWriter<'w, AddSheetRowRequest>,
    pub add_column_event_writer: &'a mut EventWriter<'w, RequestAddColumn>,
}

pub(super) fn show_sheet_interaction_mode_buttons<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    _registry: &SheetRegistry, // Mark as unused with underscore
    event_writers: InteractionModeEventWriters<'a, 'w>, // Takes struct of mutable refs
) {
    let is_sheet_selected = state.selected_sheet_name.is_some();

    let can_add_row =
        is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
    if ui
        .add_enabled(can_add_row, egui::Button::new("➕ Add Row"))
        .clicked()
    {
        if let Some(sheet_name) = &state.selected_sheet_name {
            event_writers // Use directly
                .add_row_event_writer
                .write(AddSheetRowRequest {
                    category: state.selected_category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            state.request_scroll_to_new_row = true;
            state.force_filter_recalculation = true;
        }
    }
    ui.separator();

    if state.current_interaction_mode == SheetInteractionState::AiModeActive {
        if ui.button("❌ Cancel AI").clicked() {
            state.reset_interaction_modes_and_selections();
        }
    } else {
        let can_enter_ai_mode =
            is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
        if ui
            .add_enabled(can_enter_ai_mode, egui::Button::new("✨ AI Mode"))
            .on_hover_text("Enable row selection and AI controls")
            .clicked()
        {
            state.current_interaction_mode = SheetInteractionState::AiModeActive;
            state.ai_mode = AiModeState::Preparing;
            state.ai_selected_rows.clear();
        }
    }
    ui.separator();

    if state.current_interaction_mode == SheetInteractionState::DeleteModeActive {
        if ui.button("❌ Cancel Delete").clicked() {
            state.reset_interaction_modes_and_selections();
        }
    } else {
        let can_enter_delete_mode =
            is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
        if ui
            .add_enabled(can_enter_delete_mode, egui::Button::new("🗑️ Delete Mode"))
            .on_hover_text("Enable row and column selection for deletion")
            .clicked()
        {
            state.current_interaction_mode = SheetInteractionState::DeleteModeActive;
            state.ai_selected_rows.clear();
            state.selected_columns_for_deletion.clear();
        }
    }
    ui.separator();

    if state.current_interaction_mode == SheetInteractionState::ColumnModeActive {
        if ui
            .button("➕ Add Column")
            .on_hover_text("Add a new column to the current sheet")
            .clicked()
        {
            if let Some(sheet_name) = &state.selected_sheet_name {
                event_writers // Use directly
                    .add_column_event_writer
                    .write(RequestAddColumn {
                        category: state.selected_category.clone(),
                        sheet_name: sheet_name.clone(),
                    });
            }
        }
        if ui.button("❌ Finish Column Edit").clicked() {
            state.reset_interaction_modes_and_selections();
        }
    } else {
        let can_enter_column_mode =
            is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
        if ui
            .add_enabled(can_enter_column_mode, egui::Button::new("🏛️ Column Mode"))
            .on_hover_text("Enable column adding, deletion, and reordering")
            .clicked()
        {
            state.current_interaction_mode = SheetInteractionState::ColumnModeActive;
        }
    }
}