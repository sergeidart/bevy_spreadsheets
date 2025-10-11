use bevy_egui::egui::Color32;

use crate::sheets::systems::logic::{generate_structure_preview, generate_structure_preview_from_rows};
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::{
    EditorWindowState, ReviewChoice, StructureDetailContext, StructureReviewEntry,
};

use super::blocks::RowKind;
use super::context::{
    get_structure_preview_rows, has_undecided_structures_in_context, is_parent_key_column,
    matches_detail_path,
};

#[derive(Debug, Clone)]
pub struct StructurePreviewResult {
    pub preview: String,
    pub parse_failed: bool,
    pub is_ai_added: bool,
}

impl StructurePreviewResult {
    pub fn empty() -> Self {
        Self {
            preview: String::new(),
            parse_failed: false,
            is_ai_added: false,
        }
    }
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
                    let preview = generate_structure_preview_from_rows(&sr.original_rows);
                    if preview.is_empty() {
                        if let Some(new_idx) = parent_new_row_index {
                            if let Some(nr) = state.ai_new_row_reviews.get(new_idx) {
                                if nr.duplicate_match_row.is_none() {
                                    return StructurePreviewResult {
                                        preview: String::new(),
                                        parse_failed: false,
                                        is_ai_added: true,
                                    };
                                }
                            }
                        }
                        return StructurePreviewResult {
                            preview: "(empty)".to_string(),
                            parse_failed: false,
                            is_ai_added: false,
                        };
                    }

                    return StructurePreviewResult {
                        preview,
                        parse_failed: false,
                        is_ai_added: false,
                    };
                }
            }

            let (preview, parse_failed) = generate_structure_preview(cell);
            return StructurePreviewResult {
                preview,
                parse_failed,
                is_ai_added: false,
            };
        }
    }

    if let Some(new_idx) = parent_new_row_index {
        if let Some(nr) = state.ai_new_row_reviews.get(new_idx) {
            if nr.duplicate_match_row.is_none() {
                return StructurePreviewResult {
                    preview: "(AI added)".to_string(),
                    parse_failed: false,
                    is_ai_added: true,
                };
            }
        }
    }

    StructurePreviewResult {
        preview: "(no cache)".to_string(),
        parse_failed: false,
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
        row_index: usize,
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, OriginalPreviewCellPlan)>,
    },
    NewPlain {
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, OriginalPreviewCellPlan)>,
    },
    NewDuplicate {
        merge_decided: bool,
        merge_selected: bool,
        has_undecided_structures: bool,
        treat_as_regular: bool,
        columns: Vec<(ColumnEntry, OriginalPreviewCellPlan)>,
    },
}

fn map_structure_preview_to_cell(result: StructurePreviewResult) -> StructurePreviewCell {
    if result.is_ai_added {
        StructurePreviewCell {
            text: "(AI added)".to_string(),
            color: Some(Color32::LIGHT_BLUE),
        }
    } else if result.parse_failed {
        StructurePreviewCell {
            text: "(parse err)".to_string(),
            color: Some(Color32::from_rgb(220, 120, 120)),
        }
    } else {
        let text = if result.preview.is_empty() {
            "(empty)".to_string()
        } else {
            result.preview
        };
        StructurePreviewCell { text, color: None }
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
                        // Hide parent_key in original/preview rows: parent_key should only be
                        // shown inside AI rows (non-interactable). For original preview rows
                        // we emit Empty so the column isn't rendered here.
                        let is_parent_key = is_parent_key_column(detail_ctx, *actual_col);
                        if is_parent_key {
                            // Don't show parent_key in original preview rows
                            columns.push((*entry, OriginalPreviewCellPlan::Empty));
                        } else if let Some(pos) = rr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col)
                        {
                            columns.push((
                                *entry,
                                OriginalPreviewCellPlan::Data(OriginalDataCellPlan {
                                    actual_col: *actual_col,
                                    position: pos,
                                    show_toggle: true,
                                    strike_ai_override: false,
                                }),
                            ));
                        } else {
                            columns.push((*entry, OriginalPreviewCellPlan::Empty));
                        }
                    }
                }
            }

            Some(OriginalPreviewPlan::Existing {
                row_index: rr.row_index,
                has_undecided_structures: has_undecided,
                columns,
            })
        }
        RowKind::NewPlain => {
            let _nr = state.ai_new_row_reviews.get(idx)?;
            let has_undecided = has_undecided_structures_in_context(
                ai_structure_reviews,
                detail_ctx,
                None,
                Some(idx),
            );

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
                                // Place the "AI Added" label in the first regular column that is
                                // NOT the parent_key (col 1). This ensures parent_key remains
                                // reserved for AI-rows only and isn't used as the label carrier.
                                let is_parent_key = is_parent_key_column(detail_ctx, *actual_col);
                                if !label_drawn && !is_parent_key {
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
                                let is_parent_key = is_parent_key_column(detail_ctx, *actual_col);
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
                                    columns.push((
                                        *entry,
                                        OriginalPreviewCellPlan::Data(OriginalDataCellPlan {
                                            actual_col: *actual_col,
                                            position: pos,
                                            show_toggle: nr.merge_decided && nr.merge_selected,
                                            strike_ai_override: strike,
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
                merge_selected: nr.merge_selected,
                has_undecided_structures: has_undecided,
                treat_as_regular,
                columns,
            })
        }
    }
}
