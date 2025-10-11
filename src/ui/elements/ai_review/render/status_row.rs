use bevy_egui::egui::{self, Color32, RichText};
use egui_extras::TableRow;

use super::row_render::RowContext;
use crate::sheets::systems::ai_review::{RowKind, StatusActionPlan, StatusCellPlan, StatusRowPlan};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::ReviewChoice;

pub fn render_status_row(
    row: &mut TableRow,
    data_idx: usize,
    kind: RowKind,
    plan: &StatusRowPlan,
    ctx: &mut RowContext<'_>,
) {
    match plan {
        StatusRowPlan::Existing { action, columns } => {
            render_status_action_cell(row, action, data_idx, kind, ctx);
            ctx.render_ancestor_keys(row);
            render_status_columns(row, columns);
        }
        StatusRowPlan::NewPlain { action } => {
            render_status_action_cell(row, action, data_idx, kind, ctx);
            ctx.render_ancestor_keys(row);
            render_empty_columns(row, ctx.merged_columns.len());
        }
        StatusRowPlan::NewDuplicate {
            action, columns, ..
        } => {
            render_status_action_cell(row, action, data_idx, kind, ctx);
            ctx.render_ancestor_keys(row);
            render_status_columns(row, columns);
        }
    }
}

fn render_status_action_cell(
    row: &mut TableRow,
    action: &StatusActionPlan,
    data_idx: usize,
    kind: RowKind,
    ctx: &mut RowContext<'_>,
) {
    row.col(|ui| match (kind, action) {
        (_, StatusActionPlan::None) => {
            ui.add_space(0.0);
        }
        (RowKind::NewDuplicate, StatusActionPlan::DecideButton) => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Decide").color(Color32::WHITE))
                            .fill(Color32::from_rgb(150, 90, 20)),
                    )
                    .clicked()
                {
                    nr.merge_decided = true;
                    if nr.choices.is_none() {
                        nr.choices =
                            Some(vec![ReviewChoice::Original; nr.non_structure_columns.len()]);
                    }
                }
            } else {
                ui.label("—");
            }
        }
        (RowKind::NewDuplicate, StatusActionPlan::MergeLabel { text, color }) => {
            ui.small(RichText::new(text.clone()).color(*color));
        }
        _ => {
            ui.add_space(0.0);
        }
    });
}

fn render_status_columns(row: &mut TableRow, columns: &[(ColumnEntry, StatusCellPlan)]) {
    for (_entry, cell_plan) in columns {
        row.col(|ui| {
            if let Some(text) = &cell_plan.text {
                let mut label = RichText::new(text.clone());
                if let Some(color) = cell_plan.color {
                    label = label.color(color);
                }
                ui.label(label);
            } else {
                ui.label("");
            }
        });
    }
}

fn render_empty_columns(row: &mut TableRow, count: usize) {
    for _ in 0..count {
        row.col(|ui| {
            ui.add_space(0.0);
        });
    }
}
