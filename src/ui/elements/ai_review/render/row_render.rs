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

/// Helper function to get the appropriate rows for structure preview.
/// Returns merged_rows if the structure is decided (contains user choices),
/// otherwise returns ai_rows.
fn get_structure_preview_rows(sr: &StructureReviewEntry) -> &[Vec<String>] {
    if sr.decided && !sr.merged_rows.is_empty() {
        &sr.merged_rows
    } else {
        &sr.ai_rows
    }
}

/// Get original structure preview from snapshot cache.
/// 
/// **Single source of truth** for all original structure previews (base level + nested).
/// 
/// **Process**:
/// 1. Lookup cached full row by (parent_row_index, parent_new_row_index).
/// 2. Extract structure cell JSON at `col_idx`.
/// 3. If in nested detail context, use pre-parsed `StructureReviewEntry.original_rows`.
/// 4. Otherwise parse JSON once and return preview.
/// 
/// **For duplicate rows**: Resolves `duplicate_match_row` to find the actual existing row's
/// structure review entry (structure reviews key by existing row, not new row).
/// 
/// Returns `(preview_text, parse_failed_flag, is_ai_added_flag)`.
fn get_original_structure_preview_from_cache(
    ctx: &RowContext,
    parent_row_index: Option<usize>,
    parent_new_row_index: Option<usize>,
    col_idx: usize,
) -> (String, bool, bool) {
    // Determine the actual parent_row_index to use for structure lookup
    // For new duplicate rows, the structure review entries reference the matched existing row
    let actual_parent_row_idx = if let Some(new_idx) = parent_new_row_index {
        if let Some(nr) = ctx.state.ai_new_row_reviews.get(new_idx) {
            nr.duplicate_match_row.or(parent_row_index)
        } else {
            parent_row_index
        }
    } else {
        parent_row_index
    };
    
    // Look up the cached snapshot
    let cache_key = (parent_row_index, parent_new_row_index);
    if let Some(cached_row) = ctx.state.ai_original_row_snapshot_cache.get(&cache_key) {
        if let Some(structure_content) = cached_row.get(col_idx) {
            // For nested structures, we need to extract from the JSON
            // Check if we're in a nested detail context
            if let Some(detail_ctx) = ctx.state.ai_structure_detail_context.as_ref() {
                // In nested context, look for a structure review entry that has parsed original_rows
                // For original rows: match by parent_row_index with parent_new_row_index = None
                // For merge/new rows: match by parent_new_row_index
                let sr_opt = ctx.ai_structure_reviews.iter().find(|sr| {
                    let parent_matches = if let Some(new_idx) = parent_new_row_index {
                        // Match merge/new row by parent_new_row_index
                        sr.parent_new_row_index == Some(new_idx)
                    } else {
                        // Match original row by parent_row_index
                        sr.parent_row_index == actual_parent_row_idx.unwrap_or(usize::MAX) 
                            && sr.parent_new_row_index.is_none()
                    };
                    
                    parent_matches && if detail_ctx.structure_path.is_empty() {
                        sr.structure_path.first() == Some(&col_idx)
                    } else {
                        sr.structure_path.starts_with(&detail_ctx.structure_path) 
                            && sr.structure_path.get(detail_ctx.structure_path.len()) == Some(&col_idx)
                    }
                });
                
                if let Some(sr) = sr_opt {
                    // Use the pre-parsed original_rows from the structure review entry
                    let preview = generate_structure_preview_from_rows(&sr.original_rows);
                    
                    // Check if this is an AI-added row (beyond original_rows_count)
                    // If so, and it has no duplicate match, it should be marked as AI-added
                    if preview.is_empty() {
                        if let Some(new_idx) = parent_new_row_index {
                            // Check if this is a NewRowReview beyond original count
                            if let Some(nr) = ctx.state.ai_new_row_reviews.get(new_idx) {
                                // No match = AI added, not originally empty
                                if nr.duplicate_match_row.is_none() {
                                    return (String::new(), false, true);
                                }
                            }
                        }
                        return ("(empty)".to_string(), false, false);
                    }
                    return (preview, false, false);
                }
            }
            
            // Top-level or fallback: parse the JSON from cache
            let (preview, parse_failed) = generate_structure_preview(structure_content);
            return (preview, parse_failed, false);
        }
    }
    
    // Fallback: no cache entry
    ("(no cache)".to_string(), false, false)
}

/// Helper function to check if a row has undecided structures in the current context.
/// When in structure detail view, only checks structures that are children of the current path.
fn has_undecided_structures_in_context(
    ctx: &RowContext,
    parent_row_index: Option<usize>,
    parent_new_row_index: Option<usize>,
) -> bool {
    ctx.ai_structure_reviews.iter().any(|sr| {
        // Match structures for this row
        let row_matches = match (parent_row_index, parent_new_row_index) {
            (Some(row_idx), None) => sr.parent_row_index == row_idx && sr.parent_new_row_index.is_none(),
            (None, Some(new_idx)) => sr.parent_new_row_index == Some(new_idx),
            _ => false,
        };
        
        if !row_matches || !sr.is_undecided() {
            return false;
        }
        
        // If we're in a structure detail view, only count structures that are children of current path
        if let Some(detail_ctx) = ctx.state.ai_structure_detail_context.as_ref() {
            // Structure must start with the current detail path to be considered
            sr.structure_path.len() > detail_ctx.structure_path.len()
                && sr.structure_path[..detail_ctx.structure_path.len()] == detail_ctx.structure_path[..]
        } else {
            // Top level: count all structures
            true
        }
    })
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
            // Check if this row has any undecided structures in the current context
            let has_undecided_structures = has_undecided_structures_in_context(
                ctx, Some(rr.row_index), None
            );
            row.col(|ui| { 
                let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Accept"));
                if btn.clicked() && !has_undecided_structures { ctx.existing_accept.push(idx); } 
                if has_undecided_structures {
                    btn.on_hover_text("Review structures first (click structure buttons)");
                }
            }); 
            for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); } 
            // Use unified cache-based preview for all structure columns
            for col_entry in ctx.merged_columns { row.col(|ui| match col_entry { 
                ColumnEntry::Structure(col_idx) => {
                    let (preview, parse_failed, is_ai_added) = get_original_structure_preview_from_cache(
                        ctx, Some(rr.row_index), None, *col_idx
                    );
                    if is_ai_added {
                        ui.colored_label(Color32::LIGHT_BLUE, "(AI added)");
                    } else {
                        let txt = if parse_failed {"(parse err)".to_string()} else if preview.is_empty() {"(empty)".to_string()} else { preview };
                        ui.label(txt);
                    }
                } 
                ColumnEntry::Regular(actual_col) => { 
                    let pos = rr.non_structure_columns.iter().position(|c| c==actual_col); 
                    let orig_val = pos.and_then(|p| rr.original.get(p)).map(|s| s.as_str()).unwrap_or(""); 
                    let ai_val = pos.and_then(|p| rr.ai.get(p)).map(|s| s.as_str()); 
                    let choice = pos.and_then(|p| rr.choices.get(p)).copied(); 
                    render_review_original_cell(ui, orig_val, ai_val, choice); 
                } 
            }); } 
        } }
        RowKind::NewPlain => { 
            if let Some(_nr) = ctx.state.ai_new_row_reviews.get(idx) {
                // Check if this new row has any undecided structures in the current context
                let has_undecided_structures = has_undecided_structures_in_context(
                    ctx, None, Some(idx)
                );
                row.col(|ui| { 
                    let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Accept"));
                    if btn.clicked() && !has_undecided_structures { 
                        ctx.new_accept.push(idx); 
                    }
                    if has_undecided_structures {
                        btn.on_hover_text("Review structures first (click structure buttons)");
                    }
                }); 
                for _ in ctx.ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); } 
                // Show "AI Added" label, and for structure columns show "(AI added)"
                let mut shown_label = false;
                for col_entry in ctx.merged_columns { 
                    row.col(|ui| { 
                        match col_entry {
                            ColumnEntry::Structure(_col_idx) => {
                                // For structure columns in AI-added rows, show "(AI added)"
                                ui.colored_label(Color32::LIGHT_BLUE, "(AI added)");
                            }
                            ColumnEntry::Regular(_actual_col) => {
                                // For non-structure columns, show "AI Added" once in first column
                                if !shown_label {
                                    ui.colored_label(Color32::LIGHT_BLUE, "AI Added");
                                    shown_label = true;
                                } else {
                                    ui.label("");
                                }
                            }
                        }
                    }); 
                } 
            } 
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get(idx) {
                // Extract values we need before borrowing ctx immutably
                let match_row = nr.duplicate_match_row;
                let merge_decided = nr.merge_decided;
                let merge_selected = nr.merge_selected;
                
                // Check if this new duplicate row has any undecided structures in the current context
                // We need to check both the new row and its matched existing row
                let has_undecided_structures = has_undecided_structures_in_context(ctx, None, Some(idx))
                    || match_row.map_or(false, |row_idx| 
                        has_undecided_structures_in_context(ctx, Some(row_idx), None)
                    );
                
                // Extract data from nr before entering closures
                let original_for_merge = nr.original_for_merge.clone();
                let non_structure_columns = nr.non_structure_columns.clone();
                let choices = nr.choices.clone();
                
                let nr = ctx.state.ai_new_row_reviews.get_mut(idx).unwrap();
                row.col(|ui| {
                    if !merge_decided {
                        if ui.radio(merge_selected, "Merge").clicked() {
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
                // Treat all states except separate-decided (merge_decided && !merge_selected) as a legitimate row needing original structure preview.
                let treat_as_regular = !merge_decided || merge_selected;

                if treat_as_regular {
                    for col_entry in ctx.merged_columns {
                        row.col(|ui| match col_entry {
                            ColumnEntry::Structure(col_idx) => {
                                // Use unified cache for duplicate row structure preview
                                let (preview, parse_failed, is_ai_added) = get_original_structure_preview_from_cache(
                                    ctx, None, Some(idx), *col_idx
                                );
                                
                                // For new rows without a match, show "AI Added" instead of "(empty)"
                                if is_ai_added {
                                    ui.colored_label(Color32::LIGHT_BLUE, "(AI added)");
                                } else {
                                    let txt = if parse_failed {
                                        "(parse err)".to_string()
                                    } else if preview.is_empty() {
                                        "(empty)".to_string()
                                    } else {
                                        preview
                                    };
                                    ui.label(txt);
                                }
                            }
                            ColumnEntry::Regular(actual_col) => {
                                if let Some(orig_vec) = original_for_merge.as_ref() {
                                    if let Some(pos) = non_structure_columns.iter().position(|c| c==actual_col) {
                                        let orig_val = orig_vec.get(pos).cloned().unwrap_or_default();
                                        let mut struck = false;
                                        if merge_decided && merge_selected { if let Some(choices_vec)=choices.as_ref() { if let Some(choice)=choices_vec.get(pos) { if matches!(choice, ReviewChoice::AI) { ui.label(RichText::new(orig_val.clone()).strikethrough()); struck = true; } } } }
                                        if !struck { ui.label(orig_val); }
                                    } else { ui.label(""); }
                                } else { ui.label("?"); }
                            }
                        });
                    }
                } else {
                    // Separate-decided state: keep placeholders (acts like new separate row original preview is not relevant)
                    for _ in ctx.merged_columns { row.col(|ui| { ui.label(""); }); }
                }
            }
        }
    }
}

fn render_ai_suggested_row(row: &mut egui_extras::TableRow, idx: usize, kind: RowKind, ctx: &mut RowContext) {
    match kind {
        RowKind::Existing => {
            if let Some(rr) = ctx.state.ai_row_reviews.get(idx) {
                // Extract row_index before borrowing ctx immutably
                let row_index = rr.row_index;
                
                // Check if this row has any undecided structures in the current context
                let has_undecided_structures = has_undecided_structures_in_context(
                    ctx, Some(row_index), None
                );
                
                let rr = ctx.state.ai_row_reviews.get_mut(idx).unwrap();
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
                                // When in structure detail mode, match nested structures by checking if their path extends the current path
                                let sr = ctx.ai_structure_reviews.iter().find(|sr| {
                                    sr.parent_row_index == rr.row_index 
                                    && sr.parent_new_row_index.is_none() 
                                    && if let Some(detail_ctx) = ctx.state.ai_structure_detail_context.as_ref() {
                                        // In structure detail mode: match structures whose path starts with current path + this column
                                        sr.structure_path.len() > detail_ctx.structure_path.len()
                                        && sr.structure_path[..detail_ctx.structure_path.len()] == detail_ctx.structure_path[..]
                                        && sr.structure_path.get(detail_ctx.structure_path.len()) == Some(col_idx)
                                    } else {
                                        // Top level: match structures whose first element is this column
                                        sr.structure_path.first() == Some(col_idx)
                                    }
                                });
                                if let Some(sr) = sr {
                                    let p = generate_structure_preview_from_rows(get_structure_preview_rows(sr));
                                    let txt = if p.is_empty() { "(no changes)".to_string()} else { p };
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
                                // When in structure detail mode, match nested structures by checking if their path extends the current path
                                let sr = ctx.ai_structure_reviews.iter().find(|sr| {
                                    sr.parent_new_row_index == Some(idx)
                                    && if let Some(detail_ctx) = ctx.state.ai_structure_detail_context.as_ref() {
                                        // In structure detail mode: match structures whose path starts with current path + this column
                                        sr.structure_path.len() > detail_ctx.structure_path.len()
                                        && sr.structure_path[..detail_ctx.structure_path.len()] == detail_ctx.structure_path[..]
                                        && sr.structure_path.get(detail_ctx.structure_path.len()) == Some(col_idx)
                                    } else {
                                        // Top level: match structures whose first element is this column
                                        sr.structure_path.first() == Some(col_idx)
                                    }
                                });
                                if let Some(sr) = sr {
                                    let p = generate_structure_preview_from_rows(get_structure_preview_rows(sr));
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
                                } else {
                                    // Column is in schema but not in data
                                    // This happens when AI didn't generate a value for this column
                                    ui.label("—");
                                }
                            }
                        }
                    });
                }
            }
        }
        RowKind::NewDuplicate => {
            if let Some(nr) = ctx.state.ai_new_row_reviews.get(idx) {
                // Extract values we need before borrowing ctx immutably
                let match_row = nr.duplicate_match_row;
                let merge_decided = nr.merge_decided;
                let merge_selected = nr.merge_selected;
                
                // Check if this new duplicate row has any undecided structures in the current context
                // We need to check both the new row and its matched existing row
                let has_undecided_structures = has_undecided_structures_in_context(ctx, None, Some(idx))
                    || match_row.map_or(false, |row_idx| 
                        has_undecided_structures_in_context(ctx, Some(row_idx), None)
                    );
                
                let nr = ctx.state.ai_new_row_reviews.get_mut(idx).unwrap();
                row.col(|ui| {
                    if merge_decided {
                        let btn = ui.add_enabled(!has_undecided_structures, egui::Button::new("Cancel"));
                        if btn.clicked() && !has_undecided_structures {
                            ctx.new_cancel.push(idx);
                        }
                        if has_undecided_structures {
                            btn.on_hover_text("Review structures first (click structure buttons)");
                        }
                    } else {
                        if ui.radio(!merge_selected, "Separate").clicked() {
                            nr.merge_selected = false;
                        }
                    }
                });
                for (_h,val) in ctx.ancestor_key_columns { row.col(|ui| { ui.label(RichText::new(val).color(Color32::LIGHT_GREEN)); }); }
                
                for col_entry in ctx.merged_columns {
                    row.col(|ui| {
                        match col_entry {
                            ColumnEntry::Structure(col_idx) => {
                                // When in structure detail mode, match nested structures by checking if their path extends the current path
                                let sr = ctx.ai_structure_reviews.iter().find(|sr| {
                                    sr.parent_new_row_index == Some(idx)
                                    && if let Some(detail_ctx) = ctx.state.ai_structure_detail_context.as_ref() {
                                        // In structure detail mode: match structures whose path starts with current path + this column
                                        sr.structure_path.len() > detail_ctx.structure_path.len()
                                        && sr.structure_path[..detail_ctx.structure_path.len()] == detail_ctx.structure_path[..]
                                        && sr.structure_path.get(detail_ctx.structure_path.len()) == Some(col_idx)
                                    } else {
                                        // Top level: match structures whose first element is this column
                                        sr.structure_path.first() == Some(col_idx)
                                    }
                                });
                                if let Some(sr) = sr {
                                    let p = generate_structure_preview_from_rows(get_structure_preview_rows(sr));
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
                                } else {
                                    // Column is in schema but not in data
                                    ui.label("—");
                                }
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
                                ColumnEntry::Structure(col_idx) => {
                                    // Show structure preview for merge-decided rows
                                    if nr.merge_selected && nr.merge_decided {
                                        // Find the structure entry for this duplicate row
                                        if let Some(sr) = ctx.ai_structure_reviews.iter().find(|sr| {
                                            sr.parent_new_row_index.is_none() 
                                            && sr.parent_row_index == nr.duplicate_match_row.unwrap_or(usize::MAX) 
                                            && sr.structure_path.first() == Some(col_idx)
                                        }) {
                                            // Show merged structure if decided, otherwise show empty
                                            if sr.decided {
                                                let p = generate_structure_preview_from_rows(get_structure_preview_rows(sr));
                                                let txt = if p.is_empty() { "(empty)".to_string() } else { p };
                                                let display_txt = if sr.accepted {
                                                    RichText::new(format!("✓ {}", txt)).color(Color32::from_rgb(0, 200, 0))
                                                } else if sr.rejected {
                                                    RichText::new(format!("✗ {}", txt)).color(Color32::from_rgb(150, 150, 150))
                                                } else {
                                                    RichText::new(txt)
                                                };
                                                ui.label(display_txt);
                                            } else {
                                                ui.label("");
                                            }
                                        } else {
                                            ui.label("");
                                        }
                                    } else {
                                        ui.label("");
                                    }
                                },
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

