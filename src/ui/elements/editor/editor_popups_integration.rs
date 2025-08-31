// src/ui/elements/editor/editor_popups_integration.rs
use bevy_egui::egui;
use crate::ui::elements::popups::{
    show_ai_rule_popup, show_column_options_popup,
    show_delete_confirm_popup, show_rename_popup, show_settings_popup,
    show_new_sheet_popup, show_validator_confirm_popup,
};
use crate::ui::elements::editor::state::EditorWindowState;
use super::main_editor::SheetEventWriters; // Assuming SheetEventWriters is made public or moved
use crate::sheets::resources::SheetRegistry;
use crate::ui::UiFeedbackState;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;

#[allow(clippy::too_many_arguments)]
pub(super) fn display_active_popups(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    sheet_writers: &mut SheetEventWriters,
    registry: &mut SheetRegistry, // Needs to be mutable for some popups like column options
    ui_feedback: &UiFeedbackState,
    api_key_status_res: &mut ApiKeyDisplayStatus,
    session_api_key_res: &mut SessionApiKey,
) {
    show_column_options_popup(ctx, state, &mut sheet_writers.column_rename, &mut sheet_writers.column_validator, registry);
    // Separate confirmation popup (if needed) - pass validator writer and registry
    show_validator_confirm_popup(ctx, state, registry, Some(&mut sheet_writers.column_validator), None);
    show_rename_popup(ctx, state, &mut sheet_writers.rename_sheet, ui_feedback);
    show_delete_confirm_popup(ctx, state, &mut sheet_writers.delete_sheet);
    show_ai_rule_popup(ctx, state, registry);
    show_settings_popup(ctx, state, api_key_status_res, session_api_key_res);
    show_new_sheet_popup(ctx, state, &mut sheet_writers.create_sheet);
}