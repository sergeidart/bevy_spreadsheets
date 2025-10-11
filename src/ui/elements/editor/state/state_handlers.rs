// src/ui/elements/editor/state_handlers.rs
// Handler methods for EditorWindowState (reset, interaction modes, etc.)

use super::state_definitions::*;

impl EditorWindowState {
    pub fn reset_interaction_modes_and_selections(&mut self) {
        self.current_interaction_mode = SheetInteractionState::Idle;
        self.ai_mode = AiModeState::Idle;
        self.ai_selected_rows.clear();
        self.selected_columns_for_deletion.clear();
        // Legacy single-row / multi-map AI review fields removed.

        self.column_drag_state = ColumnDragState::default();

        // Ensure structure source columns chain has at least one entry
        if self.options_structure_source_columns.is_empty() {
            self.options_structure_source_columns.push(None);
        }

        self.pending_validator_change_requires_confirmation = false;
        self.pending_validator_new_validator_summary = None;
        self.pending_validator_target_is_structure = false;

        // NOTE: virtual structure stack intentionally preserved so user can back out after mode changes
        // Clear real structure navigation stack (hidden temporal filters) and cached filters when resetting interaction modes.
        // This ensures that selecting a sheet from the table list does not continue to apply a hidden
        // parent_key filter or any lingering custom filters from a previous navigation path.
        self.structure_navigation_stack.clear();
        // Clear filtered row indices cache and force recalculation to remove stale filters
        self.filtered_row_indices_cache.clear();
        self.force_filter_recalculation = true;
        // Clear column options popup filter inputs
        self.options_column_filter_input.clear();
        self.options_column_filter_terms.clear();
        // Keep random picker visible state as-is across mode changes.

        self.ai_group_add_popup_open = false;
        self.ai_group_add_name_input.clear();
        self.ai_group_rename_popup_open = false;
        self.ai_group_rename_target = None;
        self.ai_group_rename_input.clear();
        self.ai_group_delete_popup_open = false;
        self.ai_group_delete_target = None;
        self.ai_group_delete_target_category = None;
        self.ai_group_delete_target_sheet = None;
        // Removed ai_active_structure_review (no separate detail view)
        self.ai_pending_structure_jobs.clear();
        self.ai_active_structure_job = None;
        self.ai_last_send_root_rows.clear();
        self.ai_last_send_root_category = None;
        self.ai_last_send_root_sheet = None;
        self.ai_planned_structure_paths.clear();
        self.ai_structure_reviews.clear();
        self.ai_structure_new_row_contexts.clear();
        self.ai_structure_new_row_token_counter = 0;
        self.ai_structure_results_expected = 0;
        self.ai_structure_results_received = 0;
        self.ai_waiting_for_structure_results = false;
        self.ai_structure_generation_counter = 0;
        self.ai_structure_active_generation = 0;

        self.mark_ai_included_columns_dirty();
    }
}
