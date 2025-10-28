use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_egui::egui::{self, Color32, RichText};
use egui_extras::{TableBody, TableRow};

use super::ai_row::render_ai_suggested_row;
use super::cell_render::render_ancestor_dropdown;
use super::original_row::render_original_preview_row;
use super::status_row::render_status_row;

use crate::sheets::definitions::SheetMetadata;
use crate::sheets::resources::SheetRegistry;
pub use crate::sheets::systems::ai_review::build_blocks;
use crate::sheets::systems::ai_review::{
    prepare_ai_suggested_plan, prepare_original_preview_plan, prepare_status_row_plan, RowBlock,
    RowKind,
};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::{EditorWindowState, StructureReviewEntry};

const ROW_HEIGHT: f32 = 26.0;
pub(super) const PARENT_KEY_COLOR: Color32 = Color32::from_rgb(0, 150, 0);

pub struct RowContext<'a> {
    pub state: &'a mut EditorWindowState,
    pub ancestor_key_columns: &'a [(String, String)],
    pub merged_columns: &'a [ColumnEntry],
    pub blocks: &'a [RowBlock],
    pub group_start_indices: &'a HashSet<usize>,
    pub existing_accept: &'a mut Vec<usize>,
    pub existing_cancel: &'a mut Vec<usize>,
    pub new_accept: &'a mut Vec<usize>,
    pub new_cancel: &'a mut Vec<usize>,
    pub active_sheet_grid: Option<&'a Vec<Vec<String>>>,
    pub ai_structure_reviews: &'a [StructureReviewEntry],
    pub sheet_metadata: Option<&'a SheetMetadata>,
    pub registry: &'a SheetRegistry,
    pub linked_column_options: &'a HashMap<usize, Arc<HashSet<String>>>,
    pub structure_nav_clicked: &'a mut Option<(Option<usize>, Option<usize>, Vec<usize>)>,
    pub structure_quick_accept: &'a mut Vec<(Option<usize>, Option<usize>, Vec<usize>)>,
}

impl<'a> RowContext<'a> {
    pub fn render_ancestor_keys(&self, row: &mut TableRow) {
        for (_header, value) in self.ancestor_key_columns.iter() {
            row.col(|ui| {
                ui.label(RichText::new(value.clone()).color(PARENT_KEY_COLOR));
            });
        }
    }

    /// Build hierarchically filtered options for a specific ancestor level
    ///
    /// key_idx: The index of the ancestor key (0 = root/top-level parent, N-1 = immediate parent)
    /// current_ancestors: The current DISPLAY TEXT values of all ancestors for this row
    ///
    /// Returns: Vec of display text options that the user can select
    ///
    /// Note: The parent_key column stores row_index values, not display text.
    /// This method returns display text for the UI, and the calling code must convert
    /// the selected display text back to row_index when storing.
    fn build_ancestor_options(&self, key_idx: usize, current_ancestors: &[String]) -> Vec<String> {
        // Try to get parent sheet info from virtual_structure_stack first
        // If not available, derive it from sheet name
        let (parent_category, parent_sheet_name) = if let Some(vs) = self.state.virtual_structure_stack.get(key_idx) {
            (vs.parent.parent_category.clone(), vs.parent.parent_sheet.clone())
        } else {
            // Fallback: derive parent sheet from current sheet name
            // Structure tables are named: ParentTable_ColumnName
            let current_sheet = match &self.state.selected_sheet_name {
                Some(s) => s,
                None => return Vec::new(),
            };

            // Navigate up by removing suffixes (key_idx + 1) levels
            let mut target_sheet = current_sheet.as_str();
            for _ in 0..=(key_idx) {
                target_sheet = match target_sheet.rsplit_once('_') {
                    Some((parent, _)) => parent,
                    None => return Vec::new(),
                };
            }

            (self.state.selected_category.clone(), target_sheet.to_string())
        };

        // Get the parent sheet that contains the options for this level
        let parent_sheet = match self.registry.get_sheet(&parent_category, &parent_sheet_name) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let meta = match &parent_sheet.metadata {
            Some(m) => m,
            None => return Vec::new(),
        };

        // Find the first data column (for display text)
        let display_col_idx = match meta.columns.iter().position(|c| {
            let h = c.header.to_lowercase();
            h != "row_index"
                && h != "parent_key"
                && h != "id"
                && h != "created_at"
                && h != "updated_at"
        }) {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        // If this is the first level (key_idx == 0), no filtering needed
        // Just return all unique display values from the parent sheet
        if key_idx == 0 {
            let mut options = HashSet::new();
            for row in &parent_sheet.grid {
                if let Some(display_value) = row.get(display_col_idx) {
                    if !display_value.is_empty() {
                        options.insert(display_value.clone());
                    }
                }
            }
            let mut result: Vec<String> = options.into_iter().collect();
            result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            return result;
        }

        // For deeper levels, we need to filter based on the immediate parent's context
        // Since grand_* columns have been removed, we simplify to only check immediate parent
        // matching. For full hierarchy filtering, would need to walk parent chain for each row.
        //
        // Simplified approach: If we have a parent at key_idx-1, filter by that parent's row_index
        // This gives reasonable filtering without the complexity of full chain walking.
        
        if key_idx > 0 && key_idx <= current_ancestors.len() {
            let immediate_parent_display = &current_ancestors[key_idx - 1];
            
            if !immediate_parent_display.is_empty() {
                // Find the immediate parent's sheet (one level up)
                let parent_of_parent = if let Some(vs) = self.state.virtual_structure_stack.get(key_idx - 1) {
                    (vs.parent.parent_category.clone(), vs.parent.parent_sheet.clone())
                } else if let Some(current_sheet) = &self.state.selected_sheet_name {
                    // Derive parent sheet by removing last underscore segment
                    let mut target = current_sheet.as_str();
                    for _ in 0..(key_idx - 1) {
                        target = match target.rsplit_once('_') {
                            Some((parent, _)) => parent,
                            None => {
                                // Can't derive, return all options
                                let mut options = HashSet::new();
                                for row in &parent_sheet.grid {
                                    if let Some(display_value) = row.get(display_col_idx) {
                                        if !display_value.is_empty() {
                                            options.insert(display_value.clone());
                                        }
                                    }
                                }
                                let mut result: Vec<String> = options.into_iter().collect();
                                result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                                return result;
                            }
                        };
                    }
                    (self.state.selected_category.clone(), target.to_string())
                } else {
                    // No context, return all options
                    let mut options = HashSet::new();
                    for row in &parent_sheet.grid {
                        if let Some(display_value) = row.get(display_col_idx) {
                            if !display_value.is_empty() {
                                options.insert(display_value.clone());
                            }
                        }
                    }
                    let mut result: Vec<String> = options.into_iter().collect();
                    result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                    return result;
                };

                // Get the immediate parent's sheet to find its row_index
                if let Some(parent_of_parent_sheet) = self.registry.get_sheet(&parent_of_parent.0, &parent_of_parent.1) {
                    if let Some(parent_of_parent_meta) = &parent_of_parent_sheet.metadata {
                        // Find display column in parent's parent sheet
                        if let Some(parent_display_col) = parent_of_parent_meta.columns.iter().position(|c| {
                            let h = c.header.to_lowercase();
                            h != "row_index" && h != "parent_key" && h != "id" && h != "created_at" && h != "updated_at"
                        }) {
                            // Find row_index where display matches immediate_parent_display
                            if let Some(parent_row_index) = parent_of_parent_sheet.grid.iter()
                                .find(|row| row.get(parent_display_col).map(|v| v == immediate_parent_display).unwrap_or(false))
                                .and_then(|row| row.get(0).and_then(|s| s.parse::<i64>().ok()))
                            {
                                // Now filter current parent_sheet by parent_key == parent_row_index
                                if let Some(parent_key_col) = meta.columns.iter().position(|c| c.header.eq_ignore_ascii_case("parent_key")) {
                                    let mut options = HashSet::new();
                                    for row in &parent_sheet.grid {
                                        if let Some(pk_val) = row.get(parent_key_col).and_then(|v| v.parse::<i64>().ok()) {
                                            if pk_val == parent_row_index {
                                                if let Some(display_value) = row.get(display_col_idx) {
                                                    if !display_value.is_empty() {
                                                        options.insert(display_value.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let mut result: Vec<String> = options.into_iter().collect();
                                    result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                                    return result;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: return all options (no filtering)
        let mut options = HashSet::new();
        for row in &parent_sheet.grid {
            if let Some(display_value) = row.get(display_col_idx) {
                if !display_value.is_empty() {
                    options.insert(display_value.clone());
                }
            }
        }

        // Convert to sorted vector for consistent UI
        let mut result: Vec<String> = options.into_iter().collect();
        result.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        result
    }

    pub fn render_ancestor_keys_for_original_row(&mut self, row: &mut TableRow, kind: RowKind, data_idx: usize) {
        for (key_idx, (_header, _context_value)) in self.ancestor_key_columns.iter().enumerate() {
            row.col(|ui| {
                match kind {
                    RowKind::Existing => {
                        if let Some(rr) = self.state.ai_row_reviews.get_mut(data_idx) {
                            let override_key = 1000 + key_idx;
                            let override_val = rr.key_overrides.entry(override_key).or_insert(false);
                            ui.checkbox(override_val, "Override");
                        }
                    }
                    RowKind::NewDuplicate => {
                        if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                            let override_key = 1000 + key_idx;
                            let override_val = nr.key_overrides.entry(override_key).or_insert(false);
                            ui.checkbox(override_val, "Override");
                        }
                    }
                    RowKind::NewPlain => {
                        if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                            let override_key = 1000 + key_idx;
                            let override_val = nr.key_overrides.entry(override_key).or_insert(false);
                            ui.checkbox(override_val, "Override");
                        }
                    }
                }
            });
        }
    }
    
    pub fn render_ancestor_keys_for_ai_row(&mut self, row: &mut TableRow, kind: RowKind, data_idx: usize) {
        for (key_idx, (_header, context_value)) in self.ancestor_key_columns.iter().enumerate() {
            row.col(|ui| {
                match kind {
                    RowKind::Existing => {
                        // Phase 1: Prepare data (mutable access)
                        let (override_enabled, need_rebuild, ancestor_snapshot_opt) = if let Some(rr) = self.state.ai_row_reviews.get_mut(data_idx) {
                            // Ensure ancestor_key_values is populated
                            while rr.ancestor_key_values.len() <= key_idx {
                                rr.ancestor_key_values.push(String::new());
                            }
                            // Initialize from context if empty
                            if rr.ancestor_key_values[key_idx].is_empty() {
                                rr.ancestor_key_values[key_idx] = context_value.clone();
                            }

                            let override_key = 1000 + key_idx;
                            let override_enabled = *rr.key_overrides.get(&override_key).unwrap_or(&false);

                            let (need_rebuild, snapshot_opt) = if override_enabled {
                                let need_rebuild = if let Some((cached_ancestors, _)) = rr.ancestor_dropdown_cache.get(&key_idx) {
                                    cached_ancestors.len() != key_idx ||
                                    cached_ancestors.iter().enumerate().any(|(i, v)| {
                                        rr.ancestor_key_values.get(i).map(|s| s.as_str()) != Some(v.as_str())
                                    })
                                } else {
                                    true
                                };

                                let snapshot: Option<Vec<String>> = if need_rebuild {
                                    Some(rr.ancestor_key_values.iter().take(key_idx).cloned().collect())
                                } else {
                                    None
                                };

                                (need_rebuild, snapshot)
                            } else {
                                (false, None)
                            };

                            (override_enabled, need_rebuild, snapshot_opt)
                        } else {
                            (false, false, None)
                        };

                        // Phase 2: Build options (immutable access to self)
                        if need_rebuild {
                            if let Some(ancestor_snapshot) = ancestor_snapshot_opt {
                                let options = self.build_ancestor_options(key_idx, &ancestor_snapshot);
                                if let Some(rr) = self.state.ai_row_reviews.get_mut(data_idx) {
                                    rr.ancestor_dropdown_cache.insert(key_idx, (ancestor_snapshot, options));
                                }
                            }
                        }

                        // Phase 3: Render UI
                        if let Some(rr) = self.state.ai_row_reviews.get_mut(data_idx) {
                            let options = if override_enabled {
                                rr.ancestor_dropdown_cache
                                    .get(&key_idx)
                                    .map(|(_, opts)| opts.clone())
                                    .unwrap_or_default()
                            } else {
                                Vec::new()
                            };

                            ui.horizontal(|ui| {
                                if override_enabled {
                                    let cell_id = egui::Id::new(("ancestor_dropdown", "existing", data_idx, key_idx));
                                    render_ancestor_dropdown(ui, &mut rr.ancestor_key_values[key_idx], &options, cell_id);
                                } else {
                                    ui.label(RichText::new(&rr.ancestor_key_values[key_idx]).color(PARENT_KEY_COLOR));
                                }
                            });
                        } else {
                            ui.label(RichText::new(context_value.clone()).color(PARENT_KEY_COLOR));
                        }
                    }
                    RowKind::NewDuplicate => {
                        // Phase 1: Prepare data (mutable access)
                        let (override_enabled, need_rebuild, ancestor_snapshot_opt) = if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                            // Ensure ancestor_key_values is populated
                            while nr.ancestor_key_values.len() <= key_idx {
                                nr.ancestor_key_values.push(String::new());
                            }
                            // Initialize from context if empty
                            if nr.ancestor_key_values[key_idx].is_empty() {
                                nr.ancestor_key_values[key_idx] = context_value.clone();
                            }

                            let override_key = 1000 + key_idx;
                            let override_enabled = *nr.key_overrides.get(&override_key).unwrap_or(&false);

                            let (need_rebuild, snapshot_opt) = if override_enabled {
                                let need_rebuild = if let Some((cached_ancestors, _)) = nr.ancestor_dropdown_cache.get(&key_idx) {
                                    cached_ancestors.len() != key_idx ||
                                    cached_ancestors.iter().enumerate().any(|(i, v)| {
                                        nr.ancestor_key_values.get(i).map(|s| s.as_str()) != Some(v.as_str())
                                    })
                                } else {
                                    true
                                };

                                let snapshot: Option<Vec<String>> = if need_rebuild {
                                    Some(nr.ancestor_key_values.iter().take(key_idx).cloned().collect())
                                } else {
                                    None
                                };

                                (need_rebuild, snapshot)
                            } else {
                                (false, None)
                            };

                            (override_enabled, need_rebuild, snapshot_opt)
                        } else {
                            (false, false, None)
                        };

                        // Phase 2: Build options (immutable access to self)
                        if need_rebuild {
                            if let Some(ancestor_snapshot) = ancestor_snapshot_opt {
                                let options = self.build_ancestor_options(key_idx, &ancestor_snapshot);
                                if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                                    nr.ancestor_dropdown_cache.insert(key_idx, (ancestor_snapshot, options));
                                }
                            }
                        }

                        // Phase 3: Render UI
                        if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                            let options = if override_enabled {
                                nr.ancestor_dropdown_cache
                                    .get(&key_idx)
                                    .map(|(_, opts)| opts.clone())
                                    .unwrap_or_default()
                            } else {
                                Vec::new()
                            };

                            ui.horizontal(|ui| {
                                if override_enabled {
                                    let cell_id = egui::Id::new(("ancestor_dropdown", "new_dup", data_idx, key_idx));
                                    render_ancestor_dropdown(ui, &mut nr.ancestor_key_values[key_idx], &options, cell_id);
                                } else {
                                    ui.label(RichText::new(&nr.ancestor_key_values[key_idx]).color(PARENT_KEY_COLOR));
                                }
                            });
                        } else {
                            ui.label(RichText::new(context_value.clone()).color(PARENT_KEY_COLOR));
                        }
                    }
                    RowKind::NewPlain => {
                        // New plain rows: allow editing ancestor keys with override toggle
                        if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                            // Ensure ancestor_key_values is populated
                            while nr.ancestor_key_values.len() <= key_idx {
                                nr.ancestor_key_values.push(String::new());
                            }
                            // Initialize from context if empty
                            if nr.ancestor_key_values[key_idx].is_empty() {
                                nr.ancestor_key_values[key_idx] = context_value.clone();
                            }
                            
                            // Get or insert override state for this ancestor key
                            let override_key = 1000 + key_idx; // Use 1000+ to avoid conflicts with regular columns
                            let override_enabled = *nr.key_overrides.get(&override_key).unwrap_or(&false);
                            
                            ui.horizontal(|ui| {
                                // Show value or editable text
                                if override_enabled {
                                    // Editable text box without green color
                                    ui.add(egui::TextEdit::singleline(&mut nr.ancestor_key_values[key_idx]).desired_width(120.0));
                                } else {
                                    ui.label(RichText::new(&nr.ancestor_key_values[key_idx]).color(PARENT_KEY_COLOR));
                                }
                            });
                        } else {
                            ui.label(RichText::new(context_value.clone()).color(PARENT_KEY_COLOR));
                        }
                    }
                }
            });
        }
    }
}

pub fn render_rows(body: &mut TableBody<'_>, mut ctx: RowContext<'_>) {
    if ctx.blocks.is_empty() {
        return;
    }

    let _ = (&ctx.active_sheet_grid, ctx.sheet_metadata, ctx.registry);

    for (block_index, block) in ctx.blocks.iter().copied().enumerate() {
        let _is_group_start = ctx.group_start_indices.contains(&block_index);
        body.row(ROW_HEIGHT, |mut row| match block {
            RowBlock::OriginalPreview(data_idx, kind) => {
                let plan = {
                    let detail_ctx = ctx.state.ai_structure_detail_context.as_ref();
                    prepare_original_preview_plan(
                        &*ctx.state,
                        ctx.ai_structure_reviews,
                        detail_ctx,
                        ctx.merged_columns,
                        kind,
                        data_idx,
                    )
                };

                if let Some(plan) = plan {
                    render_original_preview_row(&mut row, data_idx, kind, &plan, &mut ctx);
                } else {
                    render_empty_row(
                        &mut row,
                        ctx.ancestor_key_columns.len(),
                        ctx.merged_columns.len(),
                    );
                }
            }
            RowBlock::AiSuggested(data_idx, kind) => {
                let plan = {
                    let detail_ctx = ctx.state.ai_structure_detail_context.as_ref();
                    prepare_ai_suggested_plan(
                        &*ctx.state,
                        ctx.ai_structure_reviews,
                        detail_ctx,
                        ctx.merged_columns,
                        ctx.linked_column_options,
                        kind,
                        data_idx,
                    )
                };

                if let Some(plan) = plan {
                    render_ai_suggested_row(&mut row, data_idx, kind, &plan, &mut ctx);
                } else {
                    render_empty_row(
                        &mut row,
                        ctx.ancestor_key_columns.len(),
                        ctx.merged_columns.len(),
                    );
                }
            }
            RowBlock::Status(data_idx, kind) => {
                let plan = prepare_status_row_plan(
                    &*ctx.state,
                    ctx.ai_structure_reviews,
                    ctx.merged_columns,
                    kind,
                    data_idx,
                );

                if let Some(plan) = plan {
                    render_status_row(&mut row, data_idx, kind, &plan, &mut ctx);
                } else {
                    render_empty_row(
                        &mut row,
                        ctx.ancestor_key_columns.len(),
                        ctx.merged_columns.len(),
                    );
                }
            }
            RowBlock::Separator => {
                // Render a full-row thin divider that spans all table columns.
                // We iterate each column to collect the left/right extents, then draw
                // a single line across the whole span using the painter from the
                // last column's ui (closures execute left-to-right).
                let total_cols = 1 + ctx.ancestor_key_columns.len() + ctx.merged_columns.len();
                // Draw a short segment across each column's rect; because each segment
                // is drawn inside its column UI they won't be clipped to only the
                // first column and will visually form a continuous line across the table.
                for _col_idx in 0..total_cols {
                    row.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        let y = rect.center().y;
                        ui.painter().line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
                        );
                    });
                }
            }
        });
    }
}

fn render_empty_row(row: &mut TableRow, ancestor_count: usize, merged_count: usize) {
    row.col(|ui| {
        ui.add_space(0.0);
    });
    for _ in 0..ancestor_count {
        row.col(|ui| {
            ui.add_space(0.0);
        });
    }
    for _ in 0..merged_count {
        row.col(|ui| {
            ui.add_space(0.0);
        });
    }
}