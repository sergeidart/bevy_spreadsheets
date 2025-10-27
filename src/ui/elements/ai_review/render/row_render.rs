use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_egui::egui::{self, Color32, RichText};
use egui_extras::{TableBody, TableRow};

use super::ai_row::render_ai_suggested_row;
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
    
    pub fn render_ancestor_keys_with_override(&mut self, row: &mut TableRow, kind: RowKind, data_idx: usize) {
        for (key_idx, (_header, context_value)) in self.ancestor_key_columns.iter().enumerate() {
            row.col(|ui| {
                match kind {
                    RowKind::Existing => {
                        if let Some(rr) = self.state.ai_row_reviews.get_mut(data_idx) {
                            // Ensure ancestor_key_values is populated
                            while rr.ancestor_key_values.len() <= key_idx {
                                rr.ancestor_key_values.push(String::new());
                            }
                            // Initialize from context if empty
                            if rr.ancestor_key_values[key_idx].is_empty() {
                                rr.ancestor_key_values[key_idx] = context_value.clone();
                            }
                            
                            // Get or insert override state for this ancestor key
                            let override_key = 1000 + key_idx; // Use 1000+ to avoid conflicts with regular columns
                            let override_enabled = *rr.key_overrides.get(&override_key).unwrap_or(&false);
                            
                            ui.horizontal(|ui| {
                                // Checkbox toggle
                                let override_val = rr.key_overrides.entry(override_key).or_insert(false);
                                ui.checkbox(override_val, "");
                                ui.add_space(4.0);
                                
                                // Show value or editable text
                                if override_enabled {
                                    // Dropdown with options from ancestor sheet first data column
                                    let mut current = rr.ancestor_key_values[key_idx].clone();
                                    egui::ComboBox::from_id_source(format!("ancestor_override_{}_existing_{}", data_idx, key_idx))
                                        .width(140.0)
                                        .selected_text(current.clone())
                                        .show_ui(ui, |ui| {
                                            // Build options on the fly from registry using virtual structure stack
                                            if let Some(vs) = self.state.virtual_structure_stack.get(key_idx) {
                                                if let Some(parent_sheet) = self.registry.get_sheet(&vs.parent.parent_category, &vs.parent.parent_sheet) {
                                                    if let Some(meta) = &parent_sheet.metadata {
                                                        let di = meta
                                                            .columns
                                                            .iter()
                                                            .position(|c| {
                                                                let h = c.header.to_lowercase();
                                                                h != "row_index"
                                                                    && h != "parent_key"
                                                                    && !h.starts_with("grand_")
                                                                    && h != "id"
                                                                    && h != "created_at"
                                                                    && h != "updated_at"
                                                            });
                                                        if let Some(di) = di {
                                                            for row in &parent_sheet.grid {
                                                                if let Some(val) = row.get(di) {
                                                                    ui.selectable_value(&mut current, val.clone(), val);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    rr.ancestor_key_values[key_idx] = current;
                                } else {
                                    ui.label(RichText::new(&rr.ancestor_key_values[key_idx]).color(PARENT_KEY_COLOR));
                                }
                            });
                        } else {
                            ui.label(RichText::new(context_value.clone()).color(PARENT_KEY_COLOR));
                        }
                    }
                    RowKind::NewDuplicate => {
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
                                // Checkbox toggle
                                let override_val = nr.key_overrides.entry(override_key).or_insert(false);
                                ui.checkbox(override_val, "");
                                ui.add_space(4.0);
                                
                                // Show value or editable text
                                if override_enabled {
                                    let mut current = nr.ancestor_key_values[key_idx].clone();
                                    egui::ComboBox::from_id_source(format!("ancestor_override_{}_new_{}", data_idx, key_idx))
                                        .width(140.0)
                                        .selected_text(current.clone())
                                        .show_ui(ui, |ui| {
                                            if let Some(vs) = self.state.virtual_structure_stack.get(key_idx) {
                                                if let Some(parent_sheet) = self.registry.get_sheet(&vs.parent.parent_category, &vs.parent.parent_sheet) {
                                                    if let Some(meta) = &parent_sheet.metadata {
                                                        let di = meta
                                                            .columns
                                                            .iter()
                                                            .position(|c| {
                                                                let h = c.header.to_lowercase();
                                                                h != "row_index"
                                                                    && h != "parent_key"
                                                                    && !h.starts_with("grand_")
                                                                    && h != "id"
                                                                    && h != "created_at"
                                                                    && h != "updated_at"
                                                            });
                                                        if let Some(di) = di {
                                                            for row in &parent_sheet.grid {
                                                                if let Some(val) = row.get(di) {
                                                                    ui.selectable_value(&mut current, val.clone(), val);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    nr.ancestor_key_values[key_idx] = current;
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
                                // Checkbox toggle
                                let override_val = nr.key_overrides.entry(override_key).or_insert(false);
                                ui.checkbox(override_val, "");
                                ui.add_space(4.0);
                                
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
