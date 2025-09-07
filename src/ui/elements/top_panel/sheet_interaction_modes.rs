// src/ui/elements/top_panel/sheet_interaction_modes.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::{AddSheetRowRequest, RequestAddColumn};
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, SheetInteractionState};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::definitions::RandomPickerMode; // For initializing from metadata when opening panel

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
    let is_sheet_selected = state.current_sheet_context().1.is_some();

    let can_add_row =
        is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
    if ui
        .add_enabled(can_add_row, egui::Button::new("➕ Add Row"))
        .clicked()
    {
        let (cat_opt, sheet_opt) = state.current_sheet_context();
        if let Some(sheet_name) = sheet_opt {
            event_writers.add_row_event_writer.write(AddSheetRowRequest { category: cat_opt, sheet_name, initial_values: None });
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
            let (cat_opt, sheet_opt) = state.current_sheet_context();
            if let Some(sheet_name) = sheet_opt { event_writers.add_column_event_writer.write(RequestAddColumn { category: cat_opt, sheet_name }); }
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

    ui.separator();
    // NEW: Random Picker toggle button (shown in the same row)
    let rp_btn_text = if state.show_random_picker_panel { "🎲 Random Picker (Hide)" } else { "🎲 Random Picker" };
    if ui
        .add_enabled(is_sheet_selected, egui::Button::new(rp_btn_text))
        .on_hover_text("Pick a random value from a column (simple) or by weighted columns (complex)")
        .clicked()
    {
        state.show_random_picker_panel = !state.show_random_picker_panel;
        // When opening the panel, initialize Random Picker UI from persisted metadata
        if state.show_random_picker_panel {
            state.random_picker_needs_init = true; // Also flag for systems that rely on it
            if let Some(sheet_name) = &state.selected_sheet_name {
                if let Some(sheet) = _registry.get_sheet(&state.selected_category, sheet_name) {
                    if let Some(meta) = &sheet.metadata {
                        let num_cols = meta.columns.len();
                        if let Some(rp) = &meta.random_picker {
                            state.random_picker_mode_is_complex = matches!(rp.mode, RandomPickerMode::Complex);
                            state.random_simple_result_col = rp.simple_result_col_index.min(num_cols.saturating_sub(1));
                            state.random_complex_result_col = rp.complex_result_col_index.min(num_cols.saturating_sub(1));
                            state.random_complex_weight_col = rp.weight_col_index.filter(|i| *i < num_cols);
                            state.random_complex_second_weight_col = rp.second_weight_col_index.filter(|i| *i < num_cols);
                        } else {
                            // Defaults if no settings persisted yet
                            state.random_picker_mode_is_complex = false;
                            state.random_simple_result_col = 0.min(num_cols.saturating_sub(1));
                            state.random_complex_result_col = 0.min(num_cols.saturating_sub(1));
                            state.random_complex_weight_col = None;
                            state.random_complex_second_weight_col = None;
                        }
                        // Clear last shown value on (re)open
                        state.random_picker_last_value.clear();
                    }
                }
            }
        }
    }

    // NEW: Summarizer toggle button
    let sum_btn_text = if state.show_summarizer_panel { "∑ Summarizer (Hide)" } else { "∑ Summarizer" };
    if ui
        .add_enabled(is_sheet_selected, egui::Button::new(sum_btn_text))
        .on_hover_text("Sum numeric column or count non-empty values for text columns")
        .clicked()
    {
        state.show_summarizer_panel = !state.show_summarizer_panel;
        if state.show_summarizer_panel {
            state.summarizer_last_result.clear();
            state.summarizer_selected_col = 0; // Reset selection to first column
        }
    }
}