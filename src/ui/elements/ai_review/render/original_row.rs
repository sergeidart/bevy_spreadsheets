use bevy_egui::egui::{self, Layout, RichText};
use egui_extras::TableRow;

use super::cell_render::{render_original_choice_toggle, render_review_original_cell};
use super::row_render::RowContext;
use crate::sheets::systems::ai_review::{
    OriginalDataCellPlan, OriginalPreviewCellPlan, OriginalPreviewPlan, RowKind,
};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::ReviewChoice;

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
                        ctx.existing_accept.push(data_idx);
                    }
                });
            });
            ctx.render_ancestor_keys(row);
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
                        ctx.new_accept.push(data_idx);
                    }
                });
            });
            ctx.render_ancestor_keys(row);
            render_original_preview_columns(row, kind, data_idx, columns, ctx);
        }
        OriginalPreviewPlan::NewDuplicate {
            merge_decided,
            merge_selected,
            has_undecided_structures,
            columns,
            ..
        } => {
            row.col(|ui| {
                if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                    if *merge_decided {
                        ui.vertical(|ui| {
                            if *merge_selected {
                                ui.label(RichText::new("Merge selected").strong());
                            } else {
                                ui.label(RichText::new("Separate selected").strong());
                            }

                            if ui.button("Change decision").clicked() {
                                nr.merge_decided = false;
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
            ctx.render_ancestor_keys(row);
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
    // Guard: parent_key (actual_col == 1) should never render as an original preview data cell.
    if plan.actual_col == 1 {
        ui.label("");
        return;
    }
    match kind {
        RowKind::Existing => {
            if let Some(rr) = ctx.state.ai_row_reviews.get_mut(data_idx) {
                let original_value = rr
                    .original
                    .get(plan.position)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let ai_value = rr.ai.get(plan.position).map(|s| s.as_str());
                let choice_opt = rr.choices.get_mut(plan.position);

                render_original_data_cell_contents(
                    ui,
                    original_value,
                    ai_value,
                    choice_opt,
                    plan.show_toggle,
                    plan.strike_ai_override,
                );
            } else {
                ui.label("—");
            }
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                let original_value = nr
                    .original_for_merge
                    .as_ref()
                    .and_then(|row| row.get(plan.position))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let ai_value = nr.ai.get(plan.position).map(|s| s.as_str());
                let choice_opt = nr
                    .choices
                    .as_mut()
                    .and_then(|choices| choices.get_mut(plan.position));

                render_original_data_cell_contents(
                    ui,
                    original_value,
                    ai_value,
                    choice_opt,
                    plan.show_toggle,
                    plan.strike_ai_override,
                );
            } else {
                ui.label("—");
            }
        }
        RowKind::NewPlain => {
            ui.label("");
        }
    }
}

fn render_original_data_cell_contents(
    ui: &mut egui::Ui,
    original_value: &str,
    ai_value: Option<&str>,
    choice_opt: Option<&mut ReviewChoice>,
    show_toggle: bool,
    strike_ai_override: bool,
) {
    let choice_value = choice_opt.as_ref().map(|choice| **choice);
    let mut choice_opt = choice_opt;

    ui.horizontal(|ui| {
        // Orig picker on the leftmost side
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            if show_toggle {
                if render_original_choice_toggle(
                    ui,
                    choice_opt.as_deref_mut(),
                    original_value,
                    ai_value,
                ) {
                    // toggle clicked handled inside toggle function
                }
            } else {
                ui.add_space(0.0);
            }
        });

        ui.add_space(6.0);

        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            render_review_original_cell(
                ui,
                original_value,
                ai_value,
                choice_value,
                strike_ai_override,
            );
        });
    });
}
