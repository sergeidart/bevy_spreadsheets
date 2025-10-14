use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_egui::egui::{self, Color32, Id, RichText};
use egui_extras::TableRow;

use super::cell_render::{
    render_ai_choice_toggle, render_review_ai_cell, render_review_ai_cell_linked,
};
use super::row_render::{RowContext, PARENT_KEY_COLOR};
use crate::sheets::systems::ai_review::{
    AiSuggestedCellPlan, AiSuggestedPlan, RegularAiCellPlan, RowKind, StructureButtonPlan,
};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;

const STRUCTURE_BUTTON_CONTEXT_LABEL: &str = "✓ Accept Structure";

pub fn render_ai_suggested_row(
    row: &mut TableRow,
    data_idx: usize,
    kind: RowKind,
    plan: &AiSuggestedPlan,
    ctx: &mut RowContext<'_>,
) {
    match plan {
        AiSuggestedPlan::Existing {
            columns,
            has_undecided_structures,
            ..
        } => {
            row.col(|ui| {
                ui.vertical(|ui| {
                    let mut decline_btn =
                        ui.add_enabled(!*has_undecided_structures, egui::Button::new("Decline"));
                    if *has_undecided_structures {
                        decline_btn = decline_btn
                            .on_disabled_hover_text("Review structure decisions first");
                    }
                    if decline_btn.clicked() {
                        ctx.existing_cancel.push(data_idx);
                    }
                });
            });
            ctx.render_ancestor_keys_with_override(row, kind, data_idx);
            render_ai_suggested_columns(row, kind, data_idx, columns, ctx, None);
        }
        AiSuggestedPlan::NewPlain { columns } => {
            row.col(|ui| {
                ui.vertical(|ui| {
                    if ui.button("Decline").clicked() {
                        ctx.new_cancel.push(data_idx);
                    }
                });
            });
            ctx.render_ancestor_keys_with_override(row, kind, data_idx);
            render_ai_suggested_columns(row, kind, data_idx, columns, ctx, None);
        }
        AiSuggestedPlan::NewDuplicate {
            merge_decided,
            merge_selected,
            columns,
            has_undecided_structures,
        } => {
            row.col(|ui| {
                if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                    if *merge_decided {
                        let label = if *merge_selected {
                            RichText::new("Merge Selected").color(Color32::from_rgb(180, 160, 40))
                        } else {
                            RichText::new("Separate").color(Color32::from_rgb(150, 150, 150))
                        };
                        ui.small(label);
                    } else {
                        ui.vertical(|ui| {
                            if *has_undecided_structures {
                                ui.label(RichText::new("Resolve structures first").italics());
                            }

                            let mut separate_btn = ui.add_enabled(
                                !*has_undecided_structures,
                                egui::Button::new(RichText::new("Separate").color(egui::Color32::WHITE))
                                    .fill(egui::Color32::from_rgb(230, 130, 40)),
                            );
                            if *has_undecided_structures {
                                separate_btn = separate_btn.on_disabled_hover_text(
                                    "Resolve structure decisions first",
                                );
                            }
                            if separate_btn.clicked() {
                                nr.merge_selected = false;
                                nr.merge_decided = true;
                            }
                        });
                    }
                } else {
                    ui.label("—");
                }
            });
            ctx.render_ancestor_keys_with_override(row, kind, data_idx);
            render_ai_suggested_columns(
                row,
                kind,
                data_idx,
                columns,
                ctx,
                Some((*merge_decided, *merge_selected)),
            );
        }
    }
}

fn render_ai_suggested_columns(
    row: &mut TableRow,
    kind: RowKind,
    data_idx: usize,
    columns: &[(ColumnEntry, AiSuggestedCellPlan)],
    ctx: &mut RowContext<'_>,
    merge_state: Option<(bool, bool)>,
) {
    for (_entry, cell_plan) in columns {
        row.col(|ui| match cell_plan {
            AiSuggestedCellPlan::Structure(button_plan) => {
                render_structure_button(ui, button_plan, ctx);
            }
            AiSuggestedCellPlan::Regular(cell_plan) => {
                render_ai_regular_cell(ui, kind, data_idx, cell_plan, ctx, merge_state);
            }
        });
    }
}

fn render_structure_button(
    ui: &mut egui::Ui,
    plan: &StructureButtonPlan,
    ctx: &mut RowContext<'_>,
) {
    let mut text = RichText::new(plan.text.clone());
    if let Some(color) = plan.text_color {
        text = text.color(color);
    }

    let mut button = egui::Button::new(text);
    if let Some(fill) = plan.fill_color {
        button = button.fill(fill);
    }

    let response = ui.add_enabled(!plan.decided, button);
    if response.clicked() && !plan.decided {
        *ctx.structure_nav_clicked = Some((
            plan.parent_row_index,
            plan.parent_new_row_index,
            plan.path.clone(),
        ));
    }

    if plan.allow_quick_accept && !plan.decided {
        response.context_menu(|menu_ui| {
            if menu_ui.button(STRUCTURE_BUTTON_CONTEXT_LABEL).clicked() {
                ctx.structure_quick_accept.push((
                    plan.parent_row_index,
                    plan.parent_new_row_index,
                    plan.path.clone(),
                ));
                menu_ui.close_menu();
            }
        });
    }

    if let Some(tooltip) = plan.tooltip {
        response.on_hover_text(tooltip);
    }
}

fn render_ai_regular_cell(
    ui: &mut egui::Ui,
    kind: RowKind,
    data_idx: usize,
    plan: &RegularAiCellPlan,
    ctx: &mut RowContext<'_>,
    merge_state: Option<(bool, bool)>,
) {
    let position = match plan.position {
        Some(pos) => pos,
        None => {
            ui.label("—");
            return;
        }
    };

    match kind {
        RowKind::Existing => {
            if let Some(rr) = ctx.state.ai_row_reviews.get_mut(data_idx) {
                // Check if this is a key column with override enabled
                let is_key_column = plan.actual_col == 0;
                let override_enabled = is_key_column && 
                    *rr.key_overrides.get(&position).unwrap_or(&false);
                
                if plan.is_parent_key {
                    // Check if override is enabled for parent_key
                    let parent_key_override = *rr.key_overrides.get(&position).unwrap_or(&false);
                    
                    if parent_key_override {
                        // Show editable textbox when override is checked (full width)
                        if let Some(cell) = rr.ai.get_mut(position) {
                            ui.add(egui::TextEdit::singleline(cell).desired_width(ui.available_width()));
                        }
                    } else {
                        // Show the parent_key as plain, non-interactable green text
                        let value = rr.ai.get(position).map(|s| s.as_str()).unwrap_or("");
                        let display = if value.trim().is_empty() { "(empty)" } else { value };
                        ui.label(RichText::new(display.to_string()).color(PARENT_KEY_COLOR));
                    }
                    return;
                }

                let original_value = rr.original.get(position).cloned().unwrap_or_default();
                let choices_vec = &mut rr.choices;
                let ai_vec = &mut rr.ai;

                ui.horizontal(|ui| {
                    let choice_opt_mut = choices_vec.get_mut(position);
                    let ai_cell_opt = ai_vec.get_mut(position);
                    let ai_value = ai_cell_opt.as_ref().map(|cell| cell.as_str());
                    render_ai_choice_toggle(ui, choice_opt_mut, original_value.as_str(), ai_value);
                    ui.add_space(6.0);

                    // Determine whether Original was explicitly chosen for this cell; if so,
                    // we want to visually strike through the AI value to indicate it's overridden.
                    let force_strikethrough = choices_vec
                        .get(position)
                        .map_or(false, |c| c == &crate::ui::elements::editor::state::ReviewChoice::Original);

                    render_ai_value_editor(
                        ui,
                        plan,
                        original_value.as_str(),
                        ai_cell_opt,
                        ctx.linked_column_options,
                        kind,
                        data_idx,
                        position,
                        force_strikethrough,
                    );
                });
            } else {
                ui.label("—");
            }
        }
        RowKind::NewPlain => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                // Check if this is a key column with override enabled
                let is_key_column = plan.actual_col == 0;
                let override_enabled = is_key_column && 
                    *nr.key_overrides.get(&position).unwrap_or(&false);
                    
                if plan.is_parent_key {
                    // Check if override is enabled for parent_key
                    let parent_key_override = *nr.key_overrides.get(&position).unwrap_or(&false);
                    
                    if parent_key_override {
                        // Show editable textbox when override is checked (full width)
                        if let Some(cell) = nr.ai.get_mut(position) {
                            ui.add(egui::TextEdit::singleline(cell).desired_width(ui.available_width()));
                        }
                    } else {
                        // Show the parent_key as plain, non-interactable green text
                        let value = nr.ai.get(position).map(|s| s.as_str()).unwrap_or("");
                        let display = if value.trim().is_empty() { "(empty)" } else { value };
                        ui.label(RichText::new(display.to_string()).color(PARENT_KEY_COLOR));
                    }
                    return;
                }

                let ai_vec = &mut nr.ai;

                ui.horizontal(|ui| {
                    let ai_cell_opt = ai_vec.get_mut(position);
                    render_ai_value_editor(
                        ui,
                        plan,
                        "",
                        ai_cell_opt,
                        ctx.linked_column_options,
                        kind,
                        data_idx,
                        position,
                        false,
                    );
                });
            } else {
                ui.label("—");
            }
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(data_idx) {
                let (merge_decided, merge_selected) = merge_state.unwrap_or((false, false));

                // Check if this is a key column with override enabled
                let is_key_column = plan.actual_col == 0;
                let override_enabled = is_key_column && 
                    *nr.key_overrides.get(&position).unwrap_or(&false);

                if plan.is_parent_key {
                    // Check if override is enabled for parent_key
                    let parent_key_override = *nr.key_overrides.get(&position).unwrap_or(&false);
                    
                    if parent_key_override {
                        // Show editable textbox when override is checked (full width)
                        if let Some(cell) = nr.ai.get_mut(position) {
                            ui.add(egui::TextEdit::singleline(cell).desired_width(ui.available_width()));
                        }
                    } else {
                        // Show the parent_key as plain, non-interactable green text
                        let value = nr.ai.get(position).map(|s| s.as_str()).unwrap_or("");
                        let display = if value.trim().is_empty() { "(empty)" } else { value };
                        ui.label(RichText::new(display.to_string()).color(PARENT_KEY_COLOR));
                    }
                    return;
                }

                let original_value = nr
                    .original_for_merge
                    .as_ref()
                    .and_then(|row| row.get(position))
                    .cloned()
                    .unwrap_or_default();
                let ai_vec = &mut nr.ai;
                let choices_vec_opt = &mut nr.choices;

                    ui.horizontal(|ui| {
                    let choice_opt = if merge_decided && merge_selected {
                        choices_vec_opt
                            .as_mut()
                            .and_then(|choices| choices.get_mut(position))
                    } else {
                        None
                    };
                    let ai_cell_opt = ai_vec.get_mut(position);
                    let ai_value = ai_cell_opt.as_ref().map(|cell| cell.as_str());

                    if merge_decided && merge_selected {
                        render_ai_choice_toggle(ui, choice_opt, original_value.as_str(), ai_value);
                        ui.add_space(6.0);
                    }

                    let force_strikethrough = (*choices_vec_opt)
                        .as_ref()
                        .and_then(|choices| choices.get(position))
                        .map_or(false, |c| c == &crate::ui::elements::editor::state::ReviewChoice::Original);

                    render_ai_value_editor(
                        ui,
                        plan,
                        original_value.as_str(),
                        ai_cell_opt,
                        ctx.linked_column_options,
                        kind,
                        data_idx,
                        position,
                        force_strikethrough,
                    );
                });
            } else {
                ui.label("—");
            }
        }
    }
}

fn render_ai_value_editor(
    ui: &mut egui::Ui,
    plan: &RegularAiCellPlan,
    original_value: &str,
    ai_cell_opt: Option<&mut String>,
    linked_column_options: &HashMap<usize, Arc<HashSet<String>>>,
    kind: RowKind,
    row_idx: usize,
    position: usize,
    force_strikethrough: bool,
) {
    if plan.has_linked_options {
        if let Some(options) = linked_column_options.get(&plan.actual_col) {
            if let Some(cell) = ai_cell_opt {
                let cell_id = linked_cell_id(kind, row_idx, plan.actual_col, position);
                render_review_ai_cell_linked(
                    ui,
                    original_value,
                    cell,
                    options.as_ref(),
                    cell_id,
                    force_strikethrough,
                );
            } else {
                ui.label("—");
            }
        } else {
            ui.label("—");
        }
    } else {
        render_review_ai_cell(ui, original_value, ai_cell_opt, force_strikethrough);
    }
}

fn linked_cell_id(kind: RowKind, row_idx: usize, actual_col: usize, position: usize) -> Id {
    let tag = match kind {
        RowKind::Existing => "existing",
        RowKind::NewPlain => "new_plain",
        RowKind::NewDuplicate => "new_dup",
    };
    Id::new(("ai_linked_cell", tag, row_idx, actual_col, position))
}
