use super::cell_render::{render_review_ai_cell, render_review_ai_cell_linked, render_review_choice_cell, render_review_original_cell};
use crate::sheets::definitions::SheetMetadata;
use crate::sheets::resources::SheetRegistry;
use crate::ui::common::{generate_structure_preview, generate_structure_preview_from_rows};
use crate::ui::elements::ai_review::ai_batch_review_ui::ColumnEntry;
use crate::ui::elements::editor::state::{EditorWindowState, ReviewChoice, StructureReviewEntry};
use bevy_egui::egui::{self, Color32, RichText};
use egui_extras::TableBody;
use std::collections::HashSet;

/// Unified 3-row block per logical review item (existing row, new row, or duplicate new row)
/// Order: OriginalPreview -> AiSuggested -> Status (optional depending on type)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowBlock {
    OriginalPreview(usize, RowKind),
    AiSuggested(usize, RowKind),
    Status(usize, RowKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind { Existing, NewPlain, NewDuplicate }

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
    pub linked_column_options: &'a std::collections::HashMap<usize, std::sync::Arc<HashSet<String>>>,
    pub structure_nav_clicked: &'a mut Option<(Option<usize>, Option<usize>, Vec<usize>)>,
}

pub fn build_blocks(state: &EditorWindowState) -> (Vec<RowBlock>, HashSet<usize>) {
    let mut blocks = Vec::new();
    let mut starts = HashSet::new();
    let mut push_group = |items: Vec<RowBlock>| { if items.is_empty() { return; } let start = blocks.len(); starts.insert(start); blocks.extend(items); };

    for i in 0..state.ai_row_reviews.len() { push_group(vec![ RowBlock::OriginalPreview(i, RowKind::Existing), RowBlock::AiSuggested(i, RowKind::Existing), RowBlock::Status(i, RowKind::Existing) ]); }

    for i in 0..state.ai_new_row_reviews.len() {
        let nr = &state.ai_new_row_reviews[i];
        if nr.duplicate_match_row.is_some() {
            let mut group = vec![
                RowBlock::OriginalPreview(i, RowKind::NewDuplicate),
                RowBlock::AiSuggested(i, RowKind::NewDuplicate),
            ];
            group.push(RowBlock::Status(i, RowKind::NewDuplicate));
            push_group(group);
        } else {
            push_group(vec![
                RowBlock::OriginalPreview(i, RowKind::NewPlain),
                RowBlock::AiSuggested(i, RowKind::NewPlain),
            ]);
        }
    }

    (blocks, starts)
}

pub fn render_rows(body: &mut TableBody, mut ctx: RowContext) {
    for (bi, blk) in ctx.blocks.iter().enumerate() {
        let group_start = ctx.group_start_indices.contains(&bi);
        let row_height = if group_start { 25.0 } else { 22.0 };
        body.row(row_height, |mut row| {
            match *blk {
                RowBlock::OriginalPreview(i, k) => render_original_preview_row(&mut row, i, k, &mut ctx),
                RowBlock::AiSuggested(i, k) => render_ai_suggested_row(&mut row, i, k, &mut ctx),
                RowBlock::Status(i, k) => render_status_row(&mut row, i, k, &mut ctx),
            }
        });
    }
}

fn render_original_preview_row(row: &mut egui_extras::TableRow, idx: usize, kind: RowKind, ctx: &mut RowContext) {
    match kind {
        RowKind::Existing => { if let Some(rr) = ctx.state.ai_row_reviews.get(idx) { 
            // Check if this row has any undecided structures
            let has_undecided_structures = ctx.ai_structure_reviews.iter().any(|sr| 
                sr.parent_row_index == rr.row_index && sr.parent_new_row_index.is_none() && sr.is_undecided()
            );
            row.col(|ui| { 
                let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Accept"));
                if btn.clicked() && !has_undecided_structures { ctx.existing_accept.push(idx); } 
                if has_undecided_structures {
                    btn.on_hover_text("Review structures first (click structure buttons)");
                }
            }); 
            for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); } 
            for col_entry in ctx.merged_columns { row.col(|ui| match col_entry { ColumnEntry::Structure(col_idx) => { let structure_content = ctx.active_sheet_grid.and_then(|g| g.get(rr.row_index)).and_then(|r| r.get(*col_idx)).map(|s| s.as_str()).unwrap_or(""); let (preview, parse_failed) = generate_structure_preview(structure_content); let txt = if parse_failed {"(parse err)".to_string()} else if preview.is_empty() {"(empty)".to_string()} else { preview }; ui.label(txt); } ColumnEntry::Regular(actual_col) => { let pos = rr.non_structure_columns.iter().position(|c| c==actual_col); let orig_val = pos.and_then(|p| rr.original.get(p)).map(|s| s.as_str()).unwrap_or(""); let ai_val = pos.and_then(|p| rr.ai.get(p)).map(|s| s.as_str()); let choice = pos.and_then(|p| rr.choices.get(p)).copied(); render_review_original_cell(ui, orig_val, ai_val, choice); } }); } 
        } }
        RowKind::NewPlain => { if ctx.state.ai_new_row_reviews.get(idx).is_some() { row.col(|ui| { if ui.button("Accept").clicked() { ctx.new_accept.push(idx); } }); for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); } let mut first = true; for _c in ctx.merged_columns { row.col(|ui| { if first { ui.colored_label(Color32::LIGHT_BLUE, "AI Added"); first=false; } else { ui.label(""); } }); } } }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(idx) {
                // Check if this new duplicate row has any undecided structures (both on matched existing row and new row itself)
                let has_undecided_structures = ctx.ai_structure_reviews.iter().any(|sr| {
                    // Check structures on the matched existing row
                    (sr.parent_new_row_index.is_none() && sr.parent_row_index == nr.duplicate_match_row.unwrap_or(usize::MAX) && sr.is_undecided())
                    // Check structures on this new row itself
                    || (sr.parent_new_row_index == Some(idx) && sr.is_undecided())
                });
                row.col(|ui| {
                    if !nr.merge_decided {
                        if ui.radio(nr.merge_selected, "Merge").clicked() {
                            nr.merge_selected = true;
                        }
                    } else {
                        let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Accept"));
                        if btn.clicked() && !has_undecided_structures {
                            ctx.new_accept.push(idx);
                        }
                        if has_undecided_structures {
                            btn.on_hover_text("Review structures first (click structure buttons)");
                        }
                    }
                });
                for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); }
                if let Some(orig_vec) = nr.original_for_merge.as_ref() {
                    for col_entry in ctx.merged_columns {
                        row.col(|ui| {
                            match col_entry {
                                ColumnEntry::Structure(col_idx) => {
                                    // Show original structure preview for duplicate row original preview layer
                                    if let Some(sr) = ctx.ai_structure_reviews.iter().find(|sr| sr.parent_new_row_index.is_none() && sr.parent_row_index == nr.duplicate_match_row.unwrap_or(usize::MAX) && sr.structure_path.first()==Some(col_idx)) {
                                        let txt = generate_structure_preview_from_rows(&sr.original_rows);
                                        let display = if txt.is_empty() {"(empty)".to_string()} else {txt};
                                        ui.label(display);
                                    } else {
                                        ui.label("(no struct)");
                                    }
                                }
                                ColumnEntry::Regular(actual_col) => {
                                    if let Some(pos) = nr.non_structure_columns.iter().position(|c| c==actual_col) {
                                        let orig_val = orig_vec.get(pos).cloned().unwrap_or_default();
                                        let mut struck = false;
                                        if nr.merge_decided && nr.merge_selected { if let Some(choices)=nr.choices.as_ref() { if let Some(choice)=choices.get(pos) { if matches!(choice, ReviewChoice::AI) { ui.label(RichText::new(orig_val.clone()).strikethrough()); struck = true; } } } }
                                        if !struck { ui.label(orig_val); }
                                    } else { ui.label(""); }
                                }
                            }
                        });
                    }
                } else {
                    for _ in ctx.merged_columns { row.col(|ui| { ui.label("?"); }); }
                }
            }
        }
    }
}

fn render_ai_suggested_row(row: &mut egui_extras::TableRow, idx: usize, kind: RowKind, ctx: &mut RowContext) {
    match kind {
        RowKind::Existing => {
            if let Some(rr) = ctx.state.ai_row_reviews.get_mut(idx) {
                // Check if this row has any undecided structures
                let has_undecided_structures = ctx.ai_structure_reviews.iter().any(|sr| 
                    sr.parent_row_index == rr.row_index && sr.parent_new_row_index.is_none() && sr.is_undecided()
                );
                row.col(|ui| { 
                    let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Cancel"));
                    if btn.clicked() && !has_undecided_structures { ctx.existing_cancel.push(idx); } 
                    if has_undecided_structures {
                        btn.on_hover_text("Review structures first (click structure buttons)");
                    }
                });
                for (_h,val) in ctx.ancestor_key_columns { row.col(|ui| { ui.label(RichText::new(val).color(Color32::from_rgb(0, 200, 0))); }); }
                
                for col_entry in ctx.merged_columns {
                    row.col(|ui| {
                        match col_entry {
                            ColumnEntry::Structure(col_idx) => {
                                let sr = ctx.ai_structure_reviews.iter().find(|sr| sr.parent_row_index==rr.row_index && sr.structure_path.first()==Some(col_idx));
                                if let Some(sr) = sr {
                                    let p = generate_structure_preview_from_rows(&sr.ai_rows);
                                    let txt = if p.is_empty() { "(no changes)".to_string() } else { p };
                                    let (btn_txt, btn_color) = if sr.decided {
                                        if sr.accepted {
                                            (RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0)), Some(Color32::from_rgba_premultiplied(0, 100, 0, 40)))
                                        } else if sr.rejected {
                                            (RichText::new(format!("✗ {}", txt)).color(Color32::from_rgb(150, 150, 150)), Some(Color32::from_rgba_premultiplied(100, 100, 100, 40)))
                                        } else {
                                            (RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0)), Some(Color32::from_rgba_premultiplied(0, 100, 0, 40)))
                                        }
                                    } else {
                                        (RichText::new(txt), None)
                                    };
                                    let mut btn = egui::Button::new(btn_txt);
                                    if let Some(color) = btn_color { btn = btn.fill(color); }
                                    // Only allow clicking if structure is NOT decided
                                    let btn_response = ui.add_enabled(!sr.decided, btn);
                                    if btn_response.clicked() && !sr.decided {
                                        *ctx.structure_nav_clicked = Some((Some(rr.row_index), None, sr.structure_path.clone()));
                                    }
                                    if sr.decided {
                                        btn_response.on_hover_text("Structure already decided");
                                    }
                                } else {
                                    ui.label("(no changes)");
                                }
                            }
                            ColumnEntry::Regular(actual_col) => {
                                if let Some(pos)= rr.non_structure_columns.iter().position(|c| c==actual_col) {
                                    let orig_val = rr.original.get(pos).map(|s| s.as_str()).unwrap_or("");
                                    
                                    let changed = if let Some(allowed_values) = ctx.linked_column_options.get(actual_col) {
                                        if let Some(ai_val) = rr.ai.get_mut(pos) {
                                            let cell_id = ui.id().with(("ai_linked_cell", rr.row_index, *actual_col, pos));
                                            render_review_ai_cell_linked(ui, orig_val, ai_val, allowed_values, cell_id)
                                        } else {
                                            false
                                        }
                                    } else {
                                        render_review_ai_cell(ui, orig_val, rr.ai.get_mut(pos))
                                    };
                                    
                                    if changed { if let Some(choice)= rr.choices.get_mut(pos) { *choice = ReviewChoice::AI; } }
                                } else { ui.label(""); }
                            }
                        }
                    });
                }
            }
        }
        RowKind::NewPlain => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(idx) {
                row.col(|ui| { if ui.button("Cancel").clicked() { ctx.new_cancel.push(idx); } });
                for (_h,val) in ctx.ancestor_key_columns { row.col(|ui| { ui.label(RichText::new(val).color(Color32::LIGHT_GREEN)); }); }
                
                for col_entry in ctx.merged_columns {
                    row.col(|ui| {
                        match col_entry {
                            ColumnEntry::Structure(col_idx) => {
                                let sr = ctx.ai_structure_reviews.iter().find(|sr| sr.parent_new_row_index==Some(idx) && sr.structure_path.first()==Some(col_idx));
                                if let Some(sr) = sr {
                                    let p = generate_structure_preview_from_rows(&sr.ai_rows);
                                    let txt = if p.is_empty() { "(empty)".to_string() } else { p };
                                    let (btn_txt, btn_color) = if sr.decided {
                                        if sr.accepted {
                                            (RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0)), Some(Color32::from_rgba_premultiplied(0, 100, 0, 40)))
                                        } else if sr.rejected {
                                            (RichText::new(format!("✗ {}", txt)).color(Color32::from_rgb(150, 150, 150)), Some(Color32::from_rgba_premultiplied(100, 100, 100, 40)))
                                        } else {
                                            (RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0)), Some(Color32::from_rgba_premultiplied(0, 100, 0, 40)))
                                        }
                                    } else {
                                        (RichText::new(txt), None)
                                    };
                                    let mut btn = egui::Button::new(btn_txt);
                                    if let Some(color) = btn_color { btn = btn.fill(color); }
                                    let btn_response = ui.add_enabled(!sr.decided, btn);
                                    if btn_response.clicked() && !sr.decided {
                                        *ctx.structure_nav_clicked = Some((None, Some(idx), sr.structure_path.clone()));
                                    }
                                    if sr.decided {
                                        btn_response.on_hover_text("Structure already decided");
                                    }
                                } else {
                                    ui.label("(empty)");
                                }
                            }
                            ColumnEntry::Regular(actual_col) => {
                                if let Some(pos)= nr.non_structure_columns.iter().position(|c| c==actual_col) {
                                    if let Some(cell)= nr.ai.get_mut(pos) {
                                        if let Some(allowed_values) = ctx.linked_column_options.get(actual_col) {
                                            let cell_id = ui.id().with(("ai_linked_new_plain", idx, *actual_col, pos));
                                            render_review_ai_cell_linked(ui, "", cell, allowed_values, cell_id);
                                        } else {
                                            ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY));
                                        }
                                    } else { ui.label(""); }
                                } else { ui.label(""); }
                            }
                        }
                    });
                }
            }
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(idx) {
                // Check if this new duplicate row has any undecided structures
                let has_undecided_structures = ctx.ai_structure_reviews.iter().any(|sr| {
                    // Check structures on the matched existing row
                    (sr.parent_new_row_index.is_none() && sr.parent_row_index == nr.duplicate_match_row.unwrap_or(usize::MAX) && sr.is_undecided())
                    // Check structures on this new row itself
                    || (sr.parent_new_row_index == Some(idx) && sr.is_undecided())
                });
                row.col(|ui| {
                    if nr.merge_decided {
                        let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Cancel"));
                        if btn.clicked() && !has_undecided_structures {
                            ctx.new_cancel.push(idx);
                        }
                        if has_undecided_structures {
                            btn.on_hover_text("Review structures first (click structure buttons)");
                        }
                    } else {
                        if ui.radio(!nr.merge_selected, "Separate").clicked() {
                            nr.merge_selected = false;
                        }
                    }
                });
                for (_h,val) in ctx.ancestor_key_columns { row.col(|ui| { ui.label(RichText::new(val).color(Color32::LIGHT_GREEN)); }); }
                
                for col_entry in ctx.merged_columns {
                    row.col(|ui| {
                        match col_entry {
                            ColumnEntry::Structure(col_idx) => {
                                let sr = ctx.ai_structure_reviews.iter().find(|sr| sr.parent_new_row_index==Some(idx) && sr.structure_path.first()==Some(col_idx));
                                if let Some(sr) = sr {
                                    let p = generate_structure_preview_from_rows(&sr.ai_rows);
                                    let txt = if p.is_empty() { "(empty)".to_string() } else { p };
                                    let (btn_txt, btn_color) = if sr.decided {
                                        if sr.accepted {
                                            (RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0)), Some(Color32::from_rgba_premultiplied(0, 100, 0, 40)))
                                        } else if sr.rejected {
                                            (RichText::new(format!("✗ {}", txt)).color(Color32::from_rgb(150, 150, 150)), Some(Color32::from_rgba_premultiplied(100, 100, 100, 40)))
                                        } else {
                                            (RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0)), Some(Color32::from_rgba_premultiplied(0, 100, 0, 40)))
                                        }
                                    } else {
                                        (RichText::new(txt), None)
                                    };
                                    let mut btn = egui::Button::new(btn_txt);
                                    if let Some(color) = btn_color { btn = btn.fill(color); }
                                    let btn_response = ui.add_enabled(!sr.decided, btn);
                                    if btn_response.clicked() && !sr.decided {
                                        *ctx.structure_nav_clicked = Some((None, Some(idx), sr.structure_path.clone()));
                                    }
                                    if sr.decided {
                                        btn_response.on_hover_text("Structure already decided");
                                    }
                                } else {
                                    ui.label("(empty)");
                                }
                            }
                            ColumnEntry::Regular(actual_col) => {
                                if let Some(pos)= nr.non_structure_columns.iter().position(|c| c==actual_col) {
                                    if let Some(cell)= nr.ai.get_mut(pos) {
                                        if let Some(allowed_values) = ctx.linked_column_options.get(actual_col) {
                                            let cell_id = ui.id().with(("ai_linked_new_dup", idx, *actual_col, pos));
                                            render_review_ai_cell_linked(ui, "", cell, allowed_values, cell_id);
                                        } else {
                                            ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY));
                                        }
                                    } else { ui.label(""); }
                                } else { ui.label(""); }
                            }
                        }
                    });
                }
            }
        }
    }
}

fn render_status_row(
    row: &mut egui_extras::TableRow,
    idx: usize,
    kind: RowKind,
    ctx: &mut RowContext,
) {
    match kind {
        RowKind::Existing => {
            if let Some(rr) = ctx.state.ai_row_reviews.get_mut(idx) {
                row.col(|ui| { ui.add_space(2.0); });
                for _ in ctx.ancestor_key_columns {
                    row.col(|ui| { ui.label(""); });
                }
                for col_entry in ctx.merged_columns {
                    row.col(|ui| {
                        match col_entry {
                            ColumnEntry::Structure(_) => { ui.label(""); },
                            ColumnEntry::Regular(actual_col) => {
                                if let Some(pos) = rr.non_structure_columns.iter().position(|c| c == actual_col) {
                                    let orig_val = rr.original.get(pos).map(|s| s.as_str()).unwrap_or("");
                                    let ai_val = rr.ai.get(pos).map(|s| s.as_str());
                                    if let Some(choice) = rr.choices.get_mut(pos) {
                                        let _changed = render_review_choice_cell(ui, Some(choice), orig_val, ai_val);
                                    } else {
                                        ui.label("");
                                    }
                                } else {
                                    ui.label("");
                                }
                            }
                        }
                    });
                }
            }
        }
        RowKind::NewPlain => {
            // Should not be present, but keep empty fallback
            row.col(|ui| { ui.add_space(0.0); });
            for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); }
            for _ in ctx.merged_columns { row.col(|ui| { ui.add_space(0.0); }); }
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get_mut(idx) {
                // Show when undecided OR decided+merge
                if (!nr.merge_decided) || (nr.merge_decided && nr.merge_selected) {
                    row.col(|ui| {
                        if nr.merge_decided && nr.merge_selected {
                            ui.small(RichText::new("Merge Choices").color(Color32::from_rgb(180, 160, 40)));
                        } else if !nr.merge_decided {
                            if ui.add(egui::Button::new(RichText::new("Decide").color(Color32::WHITE)).fill(Color32::from_rgb(150, 90, 20))).on_hover_text("Confirm selection").clicked() {
                                nr.merge_decided = true;
                            }
                        } else {
                            ui.add_space(2.0);
                        }
                    });
                    for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(2.0); }); }
                    for col_entry in ctx.merged_columns {
                        row.col(|ui| {
                            match col_entry {
                                ColumnEntry::Structure(_) => { ui.label(""); },
                                ColumnEntry::Regular(actual_col) => {
                                    if let Some(pos) = nr.non_structure_columns.iter().position(|c| c == actual_col) {
                                        if nr.merge_selected && nr.merge_decided {
                                            if let (Some(orig_vec), Some(choices)) = (nr.original_for_merge.as_ref(), nr.choices.as_mut()) {
                                                let orig_val = orig_vec.get(pos).cloned().unwrap_or_default();
                                                let ai_val = nr.ai.get(pos).map(|s| s.as_str());
                                                if let Some(choice_ref) = choices.get_mut(pos) {
                                                    let _changed = render_review_choice_cell(ui, Some(choice_ref), &orig_val, ai_val);
                                                } else { ui.label(""); }
                                            } else { ui.label(""); }
                                        } else {
                                            ui.label("");
                                        }
                                    } else { ui.label(""); }
                                }
                            }
                        });
                    }
                } else {
                    // Separate decided: empty placeholders
                    row.col(|ui| { ui.add_space(0.0); });
                    for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); }
                    for _ in ctx.merged_columns { row.col(|ui| { ui.add_space(0.0); }); }
                }
            }
        }
    }
}

