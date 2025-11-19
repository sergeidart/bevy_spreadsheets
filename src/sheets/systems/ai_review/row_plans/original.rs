use bevy_egui::egui::Color32;

use crate::sheets::systems::logic::{generate_structure_preview, generate_structure_preview_from_rows_with_headers};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::{
    EditorWindowState, ReviewChoice, StructureDetailContext, StructureReviewEntry,
};

use super::blocks::RowKind;
use super::context::{
    has_undecided_structures_in_context, is_parent_key_column,
    matches_detail_path,
};

#[derive(Debug, Clone)]
pub struct StructurePreviewResult {
    pub preview: String,
    pub is_ai_added: bool,
}

fn get_original_structure_preview_from_cache(
    state: &EditorWindowState,
    ai_structure_reviews: &[StructureReviewEntry],
    detail_ctx: Option<&StructureDetailContext>,
    parent_row_index: Option<usize>,
    parent_new_row_index: Option<usize>,
    col_idx: usize,
) -> StructurePreviewResult {
    let actual_parent_row = if let Some(new_idx) = parent_new_row_index {
        state
            .ai_new_row_reviews
            .get(new_idx)
            .and_then(|nr| nr.duplicate_match_row)
            .or(parent_row_index)
    } else {
        parent_row_index
    };

    let cache_key = (parent_row_index, parent_new_row_index);
    if let Some(cached_row) = state.ai_original_row_snapshot_cache.get(&cache_key) {
        if let Some(cell) = cached_row.get(col_idx) {
            if let Some(ctx) = detail_ctx {
                let sr_opt = ai_structure_reviews.iter().find(|sr| {
                    let parent_matches = if let Some(new_idx) = parent_new_row_index {
                        sr.parent_new_row_index == Some(new_idx)
                    } else if let Some(row_idx) = actual_parent_row {
                        sr.parent_row_index == row_idx && sr.parent_new_row_index.is_none()
                    } else {
                        false
                    };
                    parent_matches && matches_detail_path(sr, Some(ctx), col_idx)
                });

                if let Some(sr) = sr_opt {
                    // Use schema_headers to properly skip technical columns
                    // StructureReviewEntry rows don't include technical columns, so headers won't start with row_index/parent_key
                    let preview = generate_structure_preview_from_rows_with_headers(
                        &sr.original_rows,
                        Some(&sr.schema_headers),
                    );
                    
                    if preview.is_empty() {
                        if let Some(new_idx) = parent_new_row_index {
                            if let Some(nr) = state.ai_new_row_reviews.get(new_idx) {
                                if nr.duplicate_match_row.is_none() {
                                    return StructurePreviewResult {
                                        preview: String::new(),
                                        is_ai_added: true,
                                    };
                                }
                            }
                        }
                        // Return empty string instead of "(empty)" for truly empty content
                        return StructurePreviewResult {
                            preview: String::new(),
                            is_ai_added: false,
                        };
                    }

                    return StructurePreviewResult {
                        preview,
                        is_ai_added: false,
                    };
                }
            }

            let (preview, _parse_failed) = generate_structure_preview(cell);
            return StructurePreviewResult {
                preview,
                is_ai_added: false,
            };
        }
    }

    if let Some(new_idx) = parent_new_row_index {
        if let Some(nr) = state.ai_new_row_reviews.get(new_idx) {
            if nr.duplicate_match_row.is_none() {
                return StructurePreviewResult {
                    preview: "(AI added)".to_string(),
                    is_ai_added: true,
                };
            }
        }
    }

    StructurePreviewResult {
        preview: "(no cache)".to_string(),
        is_ai_added: false,
    }
}

#[derive(Debug, Clone)]
pub struct StructurePreviewCell {
    pub text: String,
    pub color: Option<Color32>,
}

#[derive(Debug, Clone)]
pub struct OriginalDataCellPlan {
    pub actual_col: usize,
    pub position: usize,
    pub show_toggle: bool,
    pub strike_ai_override: bool,
    pub is_key_column: bool,
    pub is_parent_key: bool,
}

#[derive(Debug, Clone)]
pub enum OriginalPreviewCellPlan {
    Structure(StructurePreviewCell),
    Data(OriginalDataCellPlan),
    Label { text: String, color: Color32 },
    Empty,
}

#[derive(Debug, Clone)]
pub enum OriginalPreviewPlan {
    Existing {
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, OriginalPreviewCellPlan)>,
    },
    NewPlain {
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, OriginalPreviewCellPlan)>,
    },
    NewDuplicate {
        merge_decided: bool,
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, OriginalPreviewCellPlan)>,
    },
}

fn map_structure_preview_to_cell(result: StructurePreviewResult) -> StructurePreviewCell {
    if result.is_ai_added {
        StructurePreviewCell {
            text: "(AI added)".to_string(),
            color: Some(Color32::LIGHT_BLUE),
        }
    } else {
        // Show empty string instead of "(empty)" for truly empty original content
        StructurePreviewCell { 
            text: result.preview,
            color: None 
        }
    }
}

pub fn prepare_original_preview_plan(
    state: &EditorWindowState,
    ai_structure_reviews: &[StructureReviewEntry],
    detail_ctx: Option<&StructureDetailContext>,
    merged_columns: &[ColumnEntry],
    kind: RowKind,
    idx: usize,
) -> Option<OriginalPreviewPlan> {
    match kind {
        RowKind::Existing => {
            let rr = state.ai_row_reviews.get(idx)?;
            let has_undecided = has_undecided_structures_in_context(
                ai_structure_reviews,
                detail_ctx,
                Some(rr.row_index),
                None,
            );

            // Calculate offset: In navigation drill-down (detail_ctx.is_some()), arrays have row_index and parent_key prepended
            // So array structure is: [row_index, parent_key, data...]
            // non_structure_columns refers to metadata columns (e.g., [1, 2] for parent_key and Tags)
            // But array indices are [0, 1, 2] with row_index at index 0
            // At parent level (detail_ctx.is_none()), arrays have NO prepending, so no offset needed
            // In navigation drill-down: arrays have [row_idx, parent_key, data...]
            let in_navigation_drilldown = detail_ctx.is_some();

            let mut columns = Vec::with_capacity(merged_columns.len());
            for entry in merged_columns {
                match entry {
                    ColumnEntry::Structure(col_idx) => {
                        let preview = get_original_structure_preview_from_cache(
                            state,
                            ai_structure_reviews,
                            detail_ctx,
                            Some(rr.row_index),
                            None,
                            *col_idx,
                        );
                        columns.push((
                            *entry,
                            OriginalPreviewCellPlan::Structure(map_structure_preview_to_cell(
                                preview,
                            )),
                        ));
                    }
                    ColumnEntry::Regular(actual_col) => {
                        if let Some(pos) = rr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col)
                        {
                            // Check if this is a key column (first column in the row)
                            let is_key = *actual_col == 0;
                            // Check if this is a parent_key column (column 1 in structure tables)
                            let is_parent_key = detail_ctx.is_some() && *actual_col == 1;
                            // In navigation drill-down, skip only row_idx (add 1)
                            let adjusted_pos = if in_navigation_drilldown {
                                pos + 1
                            } else {
                                pos
                            };
                            columns.push((
                                *entry,
                                OriginalPreviewCellPlan::Data(OriginalDataCellPlan {
                                    actual_col: *actual_col,
                                    position: adjusted_pos,
                                    show_toggle: true,
                                    strike_ai_override: false,
                                    is_key_column: is_key,
                                    is_parent_key,
                                }),
                            ));
                        } else {
                            columns.push((*entry, OriginalPreviewCellPlan::Empty));
                        }
                    }
                }
            }

            Some(OriginalPreviewPlan::Existing {
                has_undecided_structures: has_undecided,
                columns,
            })
        }
        RowKind::NewPlain => {
            let nr = state.ai_new_row_reviews.get(idx)?;
            let has_undecided = has_undecided_structures_in_context(
                ai_structure_reviews,
                detail_ctx,
                None,
                Some(idx),
            );

            // Check if in navigation drill-down (detail_ctx.is_some() means we're viewing child table)
            let in_navigation_drilldown = detail_ctx.is_some();

            let mut columns = Vec::with_capacity(merged_columns.len());
            let mut label_drawn = false;
            for entry in merged_columns {
                match entry {
                    ColumnEntry::Structure(_) => {
                        columns.push((
                            *entry,
                            OriginalPreviewCellPlan::Structure(StructurePreviewCell {
                                text: "(AI added)".to_string(),
                                color: Some(Color32::LIGHT_BLUE),
                            }),
                        ));
                    }
                    ColumnEntry::Regular(actual_col) => {
                        let is_parent_key = is_parent_key_column(*actual_col, detail_ctx);
                        
                        if is_parent_key {
                            // For parent_key column, create a Data plan so checkbox can render
                            if let Some(pos) = nr.non_structure_columns.iter().position(|c| c == actual_col) {
                                // In navigation drill-down, skip only row_idx (add 1)
                                let adjusted_pos = if in_navigation_drilldown {
                                    pos + 1
                                } else {
                                    pos
                                };
                                columns.push((
                                    *entry,
                                    OriginalPreviewCellPlan::Data(OriginalDataCellPlan {
                                        actual_col: *actual_col,
                                        position: adjusted_pos,
                                        show_toggle: false,
                                        strike_ai_override: false,
                                        is_key_column: false,
                                        is_parent_key: true,
                                    }),
                                ));
                            } else {
                                columns.push((*entry, OriginalPreviewCellPlan::Empty));
                            }
                        } else if !label_drawn {
                            // Place the "AI Added" label in the first regular column that is not parent_key
                            columns.push((
                                *entry,
                                OriginalPreviewCellPlan::Label {
                                    text: "AI Added".to_string(),
                                    color: Color32::LIGHT_BLUE,
                                },
                            ));
                            label_drawn = true;
                        } else {
                            columns.push((*entry, OriginalPreviewCellPlan::Empty));
                        }
                    }
                }
            }

            Some(OriginalPreviewPlan::NewPlain {
                has_undecided_structures: has_undecided,
                columns,
            })
        }
        RowKind::NewDuplicate => {
            let nr = state.ai_new_row_reviews.get(idx)?;
            let has_undecided = has_undecided_structures_in_context(
                ai_structure_reviews,
                detail_ctx,
                None,
                Some(idx),
            ) || nr.duplicate_match_row.map_or(false, |row_idx| {
                has_undecided_structures_in_context(
                    ai_structure_reviews,
                    detail_ctx,
                    Some(row_idx),
                    None,
                )
            });

            // Check if in navigation drill-down (detail_ctx.is_some() means we're viewing child table)
            let in_navigation_drilldown = detail_ctx.is_some();

            let treat_as_regular = !nr.merge_decided || nr.merge_selected;
            let mut columns = Vec::with_capacity(merged_columns.len());
            for entry in merged_columns {
                match entry {
                    ColumnEntry::Structure(col_idx) => {
                        let preview = get_original_structure_preview_from_cache(
                            state,
                            ai_structure_reviews,
                            detail_ctx,
                            None,
                            Some(idx),
                            *col_idx,
                        );
                        columns.push((
                            *entry,
                            OriginalPreviewCellPlan::Structure(map_structure_preview_to_cell(
                                preview,
                            )),
                        ));
                    }
                    ColumnEntry::Regular(actual_col) => {
                        if treat_as_regular {
                            if let Some(pos) = nr
                                .non_structure_columns
                                .iter()
                                .position(|c| c == actual_col)
                            {
                                let is_parent_key = is_parent_key_column(*actual_col, detail_ctx);
                                if is_parent_key {
                                    // Don't show parent_key for original column preview during merge
                                    // (it will be shown inside AI row only)
                                    columns.push((*entry, OriginalPreviewCellPlan::Empty));
                                } else {
                                    let strike = nr.merge_decided
                                        && nr.merge_selected
                                        && matches!(
                                            nr.choices
                                                .as_ref()
                                                .and_then(|choices| choices.get(pos))
                                                .copied(),
                                            Some(ReviewChoice::AI)
                                        );
                                    // Check if this is a key column (first column in the row)
                                    let is_key = *actual_col == 0;
                                    // In navigation drill-down, skip only row_idx (add 1)
                                    let adjusted_pos = if in_navigation_drilldown {
                                        pos + 1
                                    } else {
                                        pos
                                    };
                                    columns.push((
                                        *entry,
                                        OriginalPreviewCellPlan::Data(OriginalDataCellPlan {
                                            actual_col: *actual_col,
                                            position: adjusted_pos,
                                            show_toggle: nr.merge_decided && nr.merge_selected,
                                            strike_ai_override: strike,
                                            is_key_column: is_key,
                                            is_parent_key: false,
                                        }),
                                    ));
                                }
                            } else {
                                columns.push((*entry, OriginalPreviewCellPlan::Empty));
                            }
                        } else {
                            columns.push((*entry, OriginalPreviewCellPlan::Empty));
                        }
                    }
                }
            }

            Some(OriginalPreviewPlan::NewDuplicate {
                merge_decided: nr.merge_decided,
                has_undecided_structures: has_undecided,
                columns,
            })
        }
    }
}
