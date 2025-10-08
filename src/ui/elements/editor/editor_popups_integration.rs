// src/ui/elements/editor/editor_popups_integration.rs
use super::main_editor::SheetEventWriters; // Assuming SheetEventWriters is made public or moved
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::elements::popups::{
    show_add_table_popup, show_ai_rule_popup, show_column_options_popup,
    show_delete_category_confirm_popups, show_delete_confirm_popup, show_migration_popup,
    show_new_category_popup, show_new_sheet_popup, show_random_picker_popup, show_rename_popup,
    show_settings_popup, show_validator_confirm_popup, MigrationPopupState,
};
use crate::ui::UiFeedbackState;
use crate::visual_copier::events::{
    PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
    VisualCopierStateChanged,
};
use crate::visual_copier::resources::VisualCopierManager;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;
use bevy::prelude::EventWriter;
use bevy_egui::egui;

#[allow(clippy::too_many_arguments)]
pub(super) fn display_active_popups(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    migration_state: &mut MigrationPopupState,
    sheet_writers: &mut SheetEventWriters,
    registry: &mut SheetRegistry, // Needs to be mutable for some popups like column options
    ui_feedback: &UiFeedbackState,
    api_key_status_res: &mut ApiKeyDisplayStatus,
    session_api_key_res: &mut SessionApiKey,
    copier_manager: &mut VisualCopierManager,
    pick_folder_writer: &mut EventWriter<PickFolderRequest>,
    queue_top_panel_copy_writer: &mut EventWriter<QueueTopPanelCopyEvent>,
    reverse_folders_writer: &mut EventWriter<ReverseTopPanelFoldersEvent>,
    state_changed_writer: &mut EventWriter<VisualCopierStateChanged>,
) {
    show_column_options_popup(
        ctx,
        state,
        &mut sheet_writers.column_rename,
        &mut sheet_writers.column_validator,
        registry,
    );
    // Separate confirmation popup (if needed) - pass validator writer and registry
    show_validator_confirm_popup(
        ctx,
        state,
        registry,
        Some(&mut sheet_writers.column_validator),
        None,
    );
    show_rename_popup(
        ctx,
        state,
        &mut sheet_writers.rename_sheet,
        &mut sheet_writers.rename_category,
        ui_feedback,
    );
    show_delete_confirm_popup(ctx, state, &mut sheet_writers.delete_sheet);
    // Category popups
    show_new_category_popup(ctx, state, &mut sheet_writers.create_category);
    show_delete_category_confirm_popups(ctx, state, &mut sheet_writers.delete_category);
    show_settings_popup(
        ctx,
        state,
        api_key_status_res,
        session_api_key_res,
        registry,
        copier_manager,
        pick_folder_writer,
        queue_top_panel_copy_writer,
        reverse_folders_writer,
        state_changed_writer,
    );
    // AI Rule (per-sheet AI Context) popup is now accessed from AI Mode via 'AI Context' button
    show_ai_rule_popup(ctx, state, registry);
    show_new_sheet_popup(
        ctx,
        state,
        &mut sheet_writers.create_sheet,
        Some(&mut sheet_writers.upload_json_to_db),
    );
    // Random Picker popup (opened by gear button in the top panel)
    show_random_picker_popup(ctx, state, registry);
    // Add Table popup (database mode - opened by "Add Table" button)
    show_add_table_popup(ctx, state, migration_state);
    // Migration popup (database mode - opened from Add Table popup)
    // We create a minimal Area to provide egui::Ui context for the migration popup
    // The popup creates its own Window internally
    egui::Area::new(egui::Id::new("migration_popup_area"))
        .fixed_pos([0.0, 0.0])
        .interactable(false)
        .show(ctx, |ui| {
            show_migration_popup(
                ui,
                migration_state,
                &mut sheet_writers.migrate_json_to_db,
                &mut sheet_writers.feedback,
            );
        });
}
