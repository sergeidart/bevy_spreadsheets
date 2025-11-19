use bevy_egui::egui::{self, Layout, RichText};
use egui_extras::TableRow;
use bevy::prelude::info;

use super::cell_render::{render_original_choice_toggle, render_review_original_cell};
use super::row_render::RowContext;
use crate::sheets::systems::ai_review::{
    OriginalDataCellPlan, OriginalPreviewCellPlan, OriginalPreviewPlan, RowKind,
};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;

pub fn render_original_preview_row(
    row: &mut TableRow,
    data_idx: usize,
    kind: RowKind,
    plan: &OriginalPreviewPlan,
    ctx: &mut RowContext<'_>,
) {
    match plan {
        OriginalPreviewPlan::Existing {
            has_undecided_structures,
            columns,
            ..
        } => {
            row.col(|ui| {
                ui.vertical(|ui| {
                    let mut accept_response =
                        ui.add_enabled(!*has_undecided_structures, egui::Button::new("Accept"));
                    if *has_undecided_structures {
                        accept_response = accept_response
                            .on_disabled_hover_text("Review structure decisions first");
                    }
                    if accept_response.clicked() {
                        info!("Child table: Accept clicked for existing row {}", data_idx);
                        ctx.existing_accept.push(data_idx);
                    }
                });
            });
            ctx.render_ancestor_keys_for_original_row(row, kind, data_idx);
            render_original_preview_columns(row, kind, data_idx, columns, ctx);
        }
        OriginalPreviewPlan::NewPlain {
            has_undecided_structures,
            columns,
        } => {
            row.col(|ui| {
                ui.vertical(|ui| {
                    let mut add_response =
                        ui.add_enabled(!*has_undecided_structures, egui::Button::new("Accept"));
                    if *has_undecided_structures {
                        add_response = add_response.on_disabled_hover_text("Review structure decisions first");
                    }
                    if add_response.clicked() {
                        info!("Child table: Accept clicked for new row {}", data_idx);
                        ctx.new_accept.push(data_idx);
                    }
                });
            });
            ctx.render_ancestor_keys_for_original_row(row, kind, data_idx);
            render_original_preview_columns(row, kind, data_idx, columns, ctx);
        }
        OriginalPreviewPlan::NewDuplicate {
            merge_decided,
            has_undecided_structures,
            columns,
            ..
        } => {
            row.col(|ui| {
                if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                    if *merge_decided {
                        // After decision: show Accept/Cancel like regular rows
                        ui.vertical(|ui| {
                            let mut accept_response = ui.add_enabled(
                                !*has_undecided_structures,
                                egui::Button::new("Accept"),
                            );
                            if *has_undecided_structures {
                                accept_response = accept_response
                                    .on_disabled_hover_text("Review structure decisions first");
                            }
                            if accept_response.clicked() {
                                ctx.new_accept.push(data_idx);
                            }
                        });
                    } else {
                        ui.vertical(|ui| {
                            let mut merge_btn = ui.add_enabled(
                                !*has_undecided_structures,
                                egui::Button::new(RichText::new("Merge").color(egui::Color32::WHITE))
                                    .fill(egui::Color32::from_rgb(70, 130, 220)),
                            );
                            if *has_undecided_structures {
                                merge_btn = merge_btn.on_disabled_hover_text(
                                    "Resolve structure decisions first",
                                );
                            }
                            if merge_btn.clicked() {
                                nr.merge_selected = true;
                                nr.merge_decided = true;
                            }
                        });
                    }
                } else {
                    ui.label("—");
                }
            });
            ctx.render_ancestor_keys_for_original_row(row, kind, data_idx);
            render_original_preview_columns(row, kind, data_idx, columns, ctx);
        }
    }
}

fn render_original_preview_columns(
    row: &mut TableRow,
    kind: RowKind,
    data_idx: usize,
    columns: &[(ColumnEntry, OriginalPreviewCellPlan)],
    ctx: &mut RowContext<'_>,
) {
    for (_entry, cell_plan) in columns {
        row.col(|ui| match cell_plan {
            OriginalPreviewCellPlan::Structure(cell) => {
                let mut text = RichText::new(cell.text.clone());
                if let Some(color) = cell.color {
                    text = text.color(color);
                }
                ui.label(text);
            }
            OriginalPreviewCellPlan::Data(plan) => {
                render_original_data_cell(ui, kind, data_idx, plan, ctx);
            }
            OriginalPreviewCellPlan::Label { text, color } => {
                ui.label(RichText::new(text.clone()).color(*color));
            }
            OriginalPreviewCellPlan::Empty => {
                ui.label("");
            }
        });
    }
}

fn render_original_data_cell(
    ui: &mut egui::Ui,
    kind: RowKind,
    data_idx: usize,
    plan: &OriginalDataCellPlan,
    ctx: &mut RowContext<'_>,
) {
    // Use the is_parent_key field from the plan
    let is_parent_key = plan.is_parent_key;
    
    match kind {
        RowKind::Existing => {
            if let Some(rr) = ctx.state.ai_row_reviews.get_mut(data_idx) {
                // Get override state for key columns or parent_key
                let override_enabled = (plan.is_key_column || is_parent_key) && 
                    *rr.key_overrides.get(&plan.position).unwrap_or(&false);

                render_original_data_cell_contents_existing(
                    ui,
                    plan.position,
                    plan.actual_col,
                    plan.show_toggle,
                    plan.strike_ai_override,
                    plan.is_key_column,
                    is_parent_key,
                    override_enabled,
                    rr,
                );
            } else {
                ui.label("—");
            }
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                // Get override state for key columns or parent_key
                let override_enabled = (plan.is_key_column || is_parent_key) && 
                    *nr.key_overrides.get(&plan.position).unwrap_or(&false);

                render_original_data_cell_contents_new_duplicate(
                    ui,
                    plan.position,
                    plan.actual_col,
                    plan.show_toggle,
                    plan.strike_ai_override,
                    plan.is_key_column,
                    is_parent_key,
                    override_enabled,
                    nr,
                );
            } else {
                ui.label("—");
            }
        }
        RowKind::NewPlain => {
            // For NewPlain rows, only show override toggle for parent_key
            if is_parent_key {
                if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                    // Only show checkbox in Original sub-row
                    // The actual value is shown in the AI sub-row
                    let override_val = nr.key_overrides.entry(plan.position).or_insert(false);
                    ui.checkbox(override_val, "Override");
                } else {
                    ui.label("");
                }
            } else {
                ui.label("");
            }
        }
    }
}

fn render_original_data_cell_contents_existing(
    ui: &mut egui::Ui,
    position: usize,
    _actual_col: usize,
    show_toggle: bool,
    strike_ai_override: bool,
    is_key_column: bool,
    is_parent_key: bool,
    override_enabled: bool,
    rr: &mut crate::ui::elements::editor::state::RowReview,
) {
    if is_parent_key {
        // For parent_key: only show checkbox in Original sub-row
        // The actual value is shown in the AI sub-row
        let override_val = rr.key_overrides.entry(position).or_insert(false);
        ui.checkbox(override_val, "Override");
    } else {
        // For regular columns: use horizontal layout with optional checkbox
        ui.horizontal(|ui| {
            // Override toggle for key columns (leftmost)
            if is_key_column {
                let override_val = rr.key_overrides.entry(position).or_insert(false);
                if ui.checkbox(override_val, "").changed() {
                    // Override state changed
                }
                ui.add_space(4.0);
            }
            
            // Orig picker on the left side
            ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                if show_toggle && !override_enabled {
                    let original_value = rr.original.get(position).map(|s| s.as_str()).unwrap_or("");
                    let ai_value = rr.ai.get(position).map(|s| s.as_str());
                    let mut choice_opt = rr.choices.get_mut(position);
                    if render_original_choice_toggle(
                        ui,
                        choice_opt.as_deref_mut(),
                        original_value,
                        ai_value,
                    ) {
                        // toggle clicked handled inside toggle function
                    }
                } else if !is_key_column {
                    ui.add_space(0.0);
                }
            });

            ui.add_space(6.0);

            ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                if override_enabled && is_key_column {
                    // Render editable text box for overridden key columns
                    if let Some(cell) = rr.original.get_mut(position) {
                        ui.add(egui::TextEdit::singleline(cell).desired_width(220.0));
                    }
                } else {
                    let original_value = rr.original.get(position).map(|s| s.as_str()).unwrap_or("");
                    let ai_value = rr.ai.get(position).map(|s| s.as_str());
                    let choice_value = rr.choices.get(position).copied();
                    render_review_original_cell(
                        ui,
                        original_value,
                        ai_value,
                        choice_value,
                        strike_ai_override,
                    );
                }
            });
        });
    }
}

fn render_original_data_cell_contents_new_duplicate(
    ui: &mut egui::Ui,
    position: usize,
    _actual_col: usize,
    show_toggle: bool,
    strike_ai_override: bool,
    is_key_column: bool,
    is_parent_key: bool,
    override_enabled: bool,
    nr: &mut crate::ui::elements::editor::state::NewRowReview,
) {
    if is_parent_key {
        // For parent_key: only show checkbox in Original sub-row
        // The actual value is shown in the AI sub-row
        let override_val = nr.key_overrides.entry(position).or_insert(false);
        ui.checkbox(override_val, "Override");
    } else {
        // For regular columns: use horizontal layout with optional checkbox
        ui.horizontal(|ui| {
            // Override toggle for key columns (leftmost)
            if is_key_column {
                let override_val = nr.key_overrides.entry(position).or_insert(false);
                if ui.checkbox(override_val, "").changed() {
                    // Override state changed
                }
                ui.add_space(4.0);
            }
            
            // Orig picker on the left side
            ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                if show_toggle && !override_enabled {
                    let original_value = nr
                        .original_for_merge
                        .as_ref()
                        .and_then(|row| row.get(position))
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let ai_value = nr.ai.get(position).map(|s| s.as_str());
                    let mut choice_opt = nr.choices.as_mut().and_then(|c| c.get_mut(position));
                    if render_original_choice_toggle(
                        ui,
                        choice_opt.as_deref_mut(),
                        original_value,
                        ai_value,
                    ) {
                        // toggle clicked handled inside toggle function
                    }
                } else if !is_key_column {
                    ui.add_space(0.0);
                }
            });

            ui.add_space(6.0);

            ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                if override_enabled && is_key_column {
                    // Render editable text box for overridden key columns
                    if let Some(original_for_merge) = &mut nr.original_for_merge {
                        if let Some(cell) = original_for_merge.get_mut(position) {
                            ui.add(egui::TextEdit::singleline(cell).desired_width(220.0));
                        }
                    }
                } else {
                    let original_value = nr
                        .original_for_merge
                        .as_ref()
                        .and_then(|row| row.get(position))
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let ai_value = nr.ai.get(position).map(|s| s.as_str());
                    let choice_value = nr.choices.as_ref().and_then(|c| c.get(position)).copied();
                    render_review_original_cell(
                        ui,
                        original_value,
                        ai_value,
                        choice_value,
                        strike_ai_override,
                    );
                }
            });
        });
    }
}