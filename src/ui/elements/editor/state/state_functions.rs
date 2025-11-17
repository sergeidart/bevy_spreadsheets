// src/ui/elements/editor/state_functions.rs
// Core functions and methods for EditorWindowState

use super::state_definitions::*;
use crate::sheets::definitions::ColumnValidator;

impl EditorWindowState {
    /// Returns the currently active sheet context.
    /// Returns the user's selected (category, sheet) pair.
    pub fn current_sheet_context(&self) -> (Option<String>, Option<String>) {
        (
            self.selected_category.clone(),
            self.selected_sheet_name.clone(),
        )
    }

    /// Add a new AI call log entry at the head of the log (newest first)
    pub fn add_ai_call_log(
        &mut self,
        status: String,
        response: Option<String>,
        request: Option<String>,
        is_error: bool,
    ) {
        let entry = AiCallLogEntry {
            status,
            response,
            request,
            is_error,
        };
        // Insert at front (newest first)
        self.ai_call_log.insert(0, entry);
        // Optionally limit log size to prevent memory issues
        if self.ai_call_log.len() > 100 {
            self.ai_call_log.truncate(100);
        }
    }

    pub fn mark_ai_included_columns_dirty(&mut self) {
        self.ai_cached_included_columns_dirty = true;
        self.ai_cached_included_columns_valid = false;
        self.ai_cached_included_structure_columns.clear();
    }

    pub fn ensure_ai_included_columns_cache(
        &mut self,
        registry: &crate::sheets::resources::SheetRegistry,
        category: &Option<String>,
        sheet_name: &str,
    ) {
        let needs_rebuild = self.ai_cached_included_columns_dirty
            || self.ai_cached_included_columns_category.as_ref() != category.as_ref()
            || self.ai_cached_included_columns_sheet.as_deref() != Some(sheet_name)
            || !self.ai_cached_included_columns_valid;

        if !needs_rebuild {
            self.ai_cached_included_columns_valid = true;
            return;
        }

        self.ai_cached_included_columns_category = category.clone();
        self.ai_cached_included_columns_sheet = Some(sheet_name.to_string());
        self.ai_cached_included_columns_path.clear();
        self.ai_cached_included_columns_dirty = false;
        self.ai_cached_included_columns_valid = false;

        if let Some(meta) = registry
            .get_sheet(category, sheet_name)
            .and_then(|s| s.metadata.as_ref())
        {
            self.ai_cached_included_columns.clear();
            self.ai_cached_included_columns
                .resize(meta.columns.len(), false);
            self.ai_cached_included_structure_columns.clear();
            self.ai_cached_included_structure_columns
                .resize(meta.columns.len(), false);

            use std::collections::HashSet;
            let mut included_structures: HashSet<Vec<usize>> = HashSet::new();
            let (root_category, root_sheet_opt) = self.resolve_root_sheet(registry);
            if let Some(root_sheet) = root_sheet_opt {
                if let Some(root_meta) = registry
                    .get_sheet(&root_category, &root_sheet)
                    .and_then(|s| s.metadata.as_ref())
                {
                    included_structures = root_meta
                        .ai_included_structure_paths()
                        .into_iter()
                        .collect();
                }
            }

            let mut prefix_path = self.ai_cached_included_columns_path.clone();
            for (idx, column) in meta.columns.iter().enumerate() {
                if matches!(column.validator, Some(ColumnValidator::Structure)) {
                    if !included_structures.is_empty() {
                        prefix_path.push(idx);
                        if included_structures.contains(&prefix_path) {
                            if let Some(flag) =
                                self.ai_cached_included_structure_columns.get_mut(idx)
                            {
                                *flag = true;
                            }
                        }
                        prefix_path.pop();
                    }
                    continue;
                }
                if !matches!(column.ai_include_in_send, Some(false)) {
                    if let Some(flag) = self.ai_cached_included_columns.get_mut(idx) {
                        *flag = true;
                    }
                }
            }
            self.ai_cached_included_columns_valid = true;
        } else {
            self.ai_cached_included_columns.clear();
            self.ai_cached_included_structure_columns.clear();
        }
    }

    /// Resolve ultimate root sheet (category, sheet) for current view (following structure parents).
    pub fn resolve_root_sheet(
        &self,
        _registry: &crate::sheets::resources::SheetRegistry,
    ) -> (Option<String>, Option<String>) {
        (
            self.selected_category.clone(),
            self.selected_sheet_name.clone(),
        )
    }

    pub fn mark_structure_result_received(&mut self) {
        self.ai_structure_results_received = self.ai_structure_results_received.saturating_add(1);
        if self.ai_structure_results_expected < self.ai_structure_results_received {
            self.ai_structure_results_expected = self.ai_structure_results_received;
        }
        // Increment completed tasks counter for progress tracking
        self.ai_completed_tasks += 1;
        self.refresh_structure_waiting_state();
    }

    pub fn refresh_structure_waiting_state(&mut self) {
        let waiting = self.ai_structure_results_received < self.ai_structure_results_expected
            || !self.ai_pending_structure_jobs.is_empty()
            || self.ai_active_structure_job.is_some();

        if waiting {
            self.ai_waiting_for_structure_results = true;
            self.ai_batch_review_active = false;
            if matches!(
                self.ai_mode,
                AiModeState::Idle
                    | AiModeState::Preparing
                    | AiModeState::ResultsReady
                    | AiModeState::Submitting
            ) {
                self.ai_mode = AiModeState::Submitting;
            }
        } else {
            self.ai_waiting_for_structure_results = false;
            self.ai_batch_review_active = true;
            // Mark that child tables need to be loaded on next system run
            self.ai_needs_structure_child_tables_loaded = true;
            // Initialize navigation context for drill_into_structure support
            self.ai_current_category = self.ai_last_send_root_category.clone();
            self.ai_current_sheet = self.ai_last_send_root_sheet.clone().unwrap_or_default();
            if matches!(
                self.ai_mode,
                AiModeState::Idle | AiModeState::Preparing | AiModeState::Submitting
            ) {
                self.ai_mode = AiModeState::ResultsReady;
            }
        }
    }

    pub fn allocate_structure_new_row_token(&mut self) -> usize {
        const NEW_ROW_TOKEN_BASE: usize = usize::MAX / 2;
        let token = NEW_ROW_TOKEN_BASE.saturating_add(self.ai_structure_new_row_token_counter);
        self.ai_structure_new_row_token_counter =
            self.ai_structure_new_row_token_counter.saturating_add(1);
        token
    }

    /// Returns true if we're currently viewing a structure sheet via navigation context
    /// and should hide the technical id/parent_key columns (columns 0 and 1)
    pub fn should_hide_structure_technical_columns(
        &self,
        category: &Option<String>,
        sheet_name: &str,
    ) -> bool {
        self.structure_navigation_stack
            .last()
            .filter(|nav_ctx| {
                &nav_ctx.structure_sheet_name == sheet_name && category == &nav_ctx.parent_category
            })
            .is_some()
    }

    /// Returns the list of visible column indices for the current sheet view
    /// Respects the 'hidden' flag on columns to hide technical columns
    /// For structure tables, technical columns (row_index at 0, parent_key at 1) are hidden by default
    /// When show_hidden_sheets is true, shows ALL columns including row_index
    pub fn get_visible_column_indices(
        &self,
        _category: &Option<String>,
        _sheet_name: &str,
        metadata: &crate::sheets::definitions::SheetMetadata,
    ) -> Vec<usize> {
        metadata
            .columns
            .iter()
            .enumerate()
            .filter(|(_, col)| {
                // Always filter out deleted columns
                if col.deleted {
                    return false;
                }
                // If show_hidden_sheets is enabled, show all non-deleted columns
                if self.show_hidden_sheets {
                    return true;
                }
                // Otherwise respect the hidden flag
                !col.hidden
            })
            .map(|(idx, _)| idx)
            .collect()
    }
}
