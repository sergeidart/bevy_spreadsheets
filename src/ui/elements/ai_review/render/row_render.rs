use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_egui::egui::{self, Color32, RichText};
use egui_extras::{TableBody, TableRow};

use super::ai_row::render_ai_suggested_row;
use super::cell_render::render_ancestor_dropdown;
use super::original_row::render_original_preview_row;

use crate::sheets::definitions::SheetMetadata;
use crate::sheets::resources::SheetRegistry;
pub use crate::sheets::systems::ai_review::build_blocks;
use crate::sheets::systems::ai_review::{
    prepare_ai_suggested_plan, prepare_original_preview_plan, RowBlock, RowKind,
};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::sheets::systems::logic::lineage_helpers::{
    get_parent_sheet_options, get_parent_sheet_options_filtered, display_value_to_row_index,
};
use crate::ui::elements::editor::state::{EditorWindowState, StructureReviewEntry};

const ROW_HEIGHT: f32 = 26.0;
pub(super) const PARENT_KEY_COLOR: Color32 = Color32::from_rgb(0, 150, 0);

/// Trait for review entries that support ancestor dropdown caching
trait AncestorDropdownSupport {
    fn get_key_overrides(&self) -> &HashMap<usize, bool>;
    fn get_key_overrides_mut(&mut self) -> &mut HashMap<usize, bool>;
    fn get_ancestor_key_values(&self) -> &Vec<String>;
    fn get_ancestor_key_values_mut(&mut self) -> &mut Vec<String>;
    fn get_ancestor_dropdown_cache(&self) -> &HashMap<usize, (Vec<String>, Vec<String>)>;
}

impl AncestorDropdownSupport for crate::ui::elements::editor::state::RowReview {
    fn get_key_overrides(&self) -> &HashMap<usize, bool> {
        &self.key_overrides
    }
    fn get_key_overrides_mut(&mut self) -> &mut HashMap<usize, bool> {
        &mut self.key_overrides
    }
    fn get_ancestor_key_values(&self) -> &Vec<String> {
        &self.ancestor_key_values
    }
    fn get_ancestor_key_values_mut(&mut self) -> &mut Vec<String> {
        &mut self.ancestor_key_values
    }
    fn get_ancestor_dropdown_cache(&self) -> &HashMap<usize, (Vec<String>, Vec<String>)> {
        &self.ancestor_dropdown_cache
    }
}

impl AncestorDropdownSupport for crate::ui::elements::editor::state::NewRowReview {
    fn get_key_overrides(&self) -> &HashMap<usize, bool> {
        &self.key_overrides
    }
    fn get_key_overrides_mut(&mut self) -> &mut HashMap<usize, bool> {
        &mut self.key_overrides
    }
    fn get_ancestor_key_values(&self) -> &Vec<String> {
        &self.ancestor_key_values
    }
    fn get_ancestor_key_values_mut(&mut self) -> &mut Vec<String> {
        &mut self.ancestor_key_values
    }
    fn get_ancestor_dropdown_cache(&self) -> &HashMap<usize, (Vec<String>, Vec<String>)> {
        &self.ancestor_dropdown_cache
    }
}

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
    /// Get parent sheet info for a specific ancestor level
    ///
    /// Virtual structures deprecated; derive from sheet name
    fn get_parent_sheet_info(&self, key_idx: usize) -> Option<(Option<String>, String)> {
        // Derive from current sheet name
        let current_sheet = self.state.selected_sheet_name.as_ref()?;

        // Navigate up by removing suffixes (key_idx + 1) levels
        let mut target_sheet = current_sheet.as_str();
        for _ in 0..=(key_idx) {
            target_sheet = target_sheet.rsplit_once('_')?.0;
        }

        Some((self.state.selected_category.clone(), target_sheet.to_string()))
    }

    /// Prepare ancestor dropdown for a review entry
    ///
    /// Ensures ancestor_key_values is populated and initialized from context if needed.
    /// Returns (override_enabled, need_rebuild, ancestor_snapshot) tuple.
    fn prepare_ancestor_dropdown<T: AncestorDropdownSupport>(
        review: &mut T,
        key_idx: usize,
        context_value: &str,
    ) -> (bool, bool, Option<Vec<String>>) {
        let values = review.get_ancestor_key_values_mut();

        // Ensure ancestor_key_values is populated
        while values.len() <= key_idx {
            values.push(String::new());
        }

        // Initialize from context if empty
        if values[key_idx].is_empty() {
            values[key_idx] = context_value.to_string();
        }

        let override_key = 1000 + key_idx;
        let override_enabled = *review.get_key_overrides().get(&override_key).unwrap_or(&false);

        let (need_rebuild, snapshot_opt) = if override_enabled {
            let cache = review.get_ancestor_dropdown_cache();
            let values = review.get_ancestor_key_values();

            let need_rebuild = if let Some((cached_ancestors, _)) = cache.get(&key_idx) {
                cached_ancestors.len() != key_idx ||
                cached_ancestors.iter().enumerate().any(|(i, v)| {
                    values.get(i).map(|s| s.as_str()) != Some(v.as_str())
                })
            } else {
                true
            };

            let snapshot: Option<Vec<String>> = if need_rebuild {
                Some(values.iter().take(key_idx).cloned().collect())
            } else {
                None
            };

            (need_rebuild, snapshot)
        } else {
            (false, None)
        };

        (override_enabled, need_rebuild, snapshot_opt)
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
        // Get parent sheet info for this level
        let Some((parent_category, parent_sheet_name)) = self.get_parent_sheet_info(key_idx) else {
            return Vec::new();
        };

        // If this is the first level (key_idx == 0), no filtering needed
        if key_idx == 0 {
            return get_parent_sheet_options(self.registry, &parent_category, &parent_sheet_name);
        }

        // For deeper levels, filter based on immediate parent's row_index
        if key_idx > 0 && key_idx <= current_ancestors.len() {
            let immediate_parent_display = &current_ancestors[key_idx - 1];

            if !immediate_parent_display.is_empty() {
                // Get the immediate parent's sheet info (one level up)
                let Some((grandparent_category, grandparent_sheet_name)) = self.get_parent_sheet_info(key_idx - 1) else {
                    // Can't determine parent, return all options
                    return get_parent_sheet_options(self.registry, &parent_category, &parent_sheet_name);
                };

                // Convert immediate parent's display text to row_index
                if let Some(parent_row_index) = display_value_to_row_index(
                    self.registry,
                    &grandparent_category,
                    &grandparent_sheet_name,
                    immediate_parent_display,
                    None, // No parent filter for the grandparent lookup
                ) {
                    // Return options filtered by this parent_key
                    return get_parent_sheet_options_filtered(
                        self.registry,
                        &parent_category,
                        &parent_sheet_name,
                        parent_row_index,
                    );
                }
            }
        }

        // Fallback: return all options (no filtering)
        get_parent_sheet_options(self.registry, &parent_category, &parent_sheet_name)
    }

    pub fn render_ancestor_keys_for_original_row(&mut self, row: &mut TableRow, kind: RowKind, data_idx: usize) {
        for (key_idx, (_header, _context_value)) in self.ancestor_key_columns.iter().enumerate() {
            row.col(|ui| {
                // Get the review entry and render the override checkbox
                let review_opt: Option<&mut dyn AncestorDropdownSupport> = match kind {
                    RowKind::Existing => self.state.ai_row_reviews.get_mut(data_idx).map(|r| r as &mut dyn AncestorDropdownSupport),
                    RowKind::NewDuplicate | RowKind::NewPlain => self.state.ai_new_row_reviews.get_mut(data_idx).map(|r| r as &mut dyn AncestorDropdownSupport),
                };

                if let Some(review) = review_opt {
                    let override_key = 1000 + key_idx;
                    let override_val = review.get_key_overrides_mut().entry(override_key).or_insert(false);
                    ui.checkbox(override_val, "Override");
                }
            });
        }
    }
    
    pub fn render_ancestor_keys_for_ai_row(&mut self, row: &mut TableRow, kind: RowKind, data_idx: usize) {
        for (key_idx, (_header, context_value)) in self.ancestor_key_columns.iter().enumerate() {
            row.col(|ui| {
                match kind {
                    RowKind::Existing => {
                        // Phase 1: Prepare data
                        let (override_enabled, need_rebuild, ancestor_snapshot_opt) =
                            if let Some(rr) = self.state.ai_row_reviews.get_mut(data_idx) {
                                Self::prepare_ancestor_dropdown(rr, key_idx, context_value)
                            } else {
                                (false, false, None)
                            };

                        // Phase 2: Build options
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
                        // Phase 1: Prepare data
                        let (override_enabled, need_rebuild, ancestor_snapshot_opt) =
                            if let Some(nr) = self.state.ai_new_row_reviews.get_mut(data_idx) {
                                Self::prepare_ancestor_dropdown(nr, key_idx, context_value)
                            } else {
                                (false, false, None)
                            };

                        // Phase 2: Build options
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