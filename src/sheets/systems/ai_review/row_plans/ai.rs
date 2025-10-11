use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_egui::egui::Color32;

use crate::sheets::systems::logic::generate_structure_preview_from_rows;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::{
    EditorWindowState, StructureDetailContext, StructureReviewEntry,
};

use super::blocks::RowKind;
use super::context::{
    get_structure_preview_rows, has_undecided_structures_in_context, is_parent_key_column,
    matches_detail_path,
};

#[derive(Debug, Clone)]
pub struct RegularAiCellPlan {
    pub actual_col: usize,
    pub position: Option<usize>,
    pub is_parent_key: bool,
    pub has_linked_options: bool,
}

#[derive(Debug, Clone)]
pub struct StructureButtonPlan {
    pub text: String,
    pub text_color: Option<Color32>,
    pub fill_color: Option<Color32>,
    pub decided: bool,
    pub parent_row_index: Option<usize>,
    pub parent_new_row_index: Option<usize>,
    pub path: Vec<usize>,
    pub allow_quick_accept: bool,
    pub tooltip: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub enum AiSuggestedCellPlan {
    Structure(StructureButtonPlan),
    Regular(RegularAiCellPlan),
}

#[derive(Debug, Clone)]
pub enum AiSuggestedPlan {
    Existing {
        row_index: usize,
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, AiSuggestedCellPlan)>,
    },
    NewPlain {
        columns: Vec<(ColumnEntry, AiSuggestedCellPlan)>,
    },
    NewDuplicate {
        merge_decided: bool,
        merge_selected: bool,
        has_undecided_structures: bool,
        columns: Vec<(ColumnEntry, AiSuggestedCellPlan)>,
    },
}

fn structure_button_placeholder(
    parent_row_index: Option<usize>,
    parent_new_row_index: Option<usize>,
) -> StructureButtonPlan {
    StructureButtonPlan {
        text: "(no changes)".to_string(),
        text_color: None,
        fill_color: None,
        decided: true,
        parent_row_index,
        parent_new_row_index,
        path: Vec::new(),
        allow_quick_accept: false,
        tooltip: None,
    }
}

fn build_structure_button_plan(
    sr: &StructureReviewEntry,
    detail_ctx: Option<&StructureDetailContext>,
    parent_row_index: Option<usize>,
    parent_new_row_index: Option<usize>,
) -> StructureButtonPlan {
    let preview = get_structure_preview_rows(sr);
    let preview = generate_structure_preview_from_rows(preview);
    let mut text = if preview.is_empty() {
        "(empty)".to_string()
    } else {
        preview
    };
    let mut text_color = None;
    let mut fill_color = None;

    if sr.decided {
        if sr.accepted {
            text = format!("✓ {}", text);
            text_color = Some(Color32::from_rgb(0, 200, 0));
            fill_color = Some(Color32::from_rgba_premultiplied(0, 100, 0, 40));
        } else if sr.rejected {
            text = format!("✗ {}", text);
            text_color = Some(Color32::from_rgb(150, 150, 150));
            fill_color = Some(Color32::from_rgba_premultiplied(100, 100, 100, 40));
        } else {
            text = format!("✓ {}", text);
            text_color = Some(Color32::from_rgb(0, 200, 0));
            fill_color = Some(Color32::from_rgba_premultiplied(0, 100, 0, 40));
        }
    }

    StructureButtonPlan {
        text,
        text_color,
        fill_color,
        decided: sr.decided,
        parent_row_index,
        parent_new_row_index,
        path: sr.structure_path.clone(),
        allow_quick_accept: !sr.decided && detail_ctx.is_none(),
        tooltip: if sr.decided {
            Some("Structure already decided")
        } else {
            None
        },
    }
}

pub fn prepare_ai_suggested_plan(
    state: &EditorWindowState,
    ai_structure_reviews: &[StructureReviewEntry],
    detail_ctx: Option<&StructureDetailContext>,
    merged_columns: &[ColumnEntry],
    linked_column_options: &HashMap<usize, Arc<HashSet<String>>>,
    kind: RowKind,
    idx: usize,
) -> Option<AiSuggestedPlan> {
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
                        let plan = ai_structure_reviews
                            .iter()
                            .find(|sr| {
                                sr.parent_row_index == rr.row_index
                                    && sr.parent_new_row_index.is_none()
                                    && matches_detail_path(sr, detail_ctx, *col_idx)
                            })
                            .map(|sr| {
                                build_structure_button_plan(
                                    sr,
                                    detail_ctx,
                                    Some(rr.row_index),
                                    None,
                                )
                            })
                            .unwrap_or_else(|| {
                                structure_button_placeholder(Some(rr.row_index), None)
                            });
                        columns.push((*entry, AiSuggestedCellPlan::Structure(plan)));
                    }
                    ColumnEntry::Regular(actual_col) => {
                        let position = rr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col);
                        let is_parent_key = is_parent_key_column(detail_ctx, *actual_col);
                        let has_linked = linked_column_options.contains_key(actual_col);
                        columns.push((
                            *entry,
                            AiSuggestedCellPlan::Regular(RegularAiCellPlan {
                                actual_col: *actual_col,
                                position,
                                is_parent_key,
                                has_linked_options: has_linked,
                            }),
                        ));
                    }
                }
            }

            Some(AiSuggestedPlan::Existing {
                row_index: rr.row_index,
                has_undecided_structures: has_undecided,
                columns,
            })
        }
        RowKind::NewPlain => {
            let nr = state.ai_new_row_reviews.get(idx)?;
            let mut columns = Vec::with_capacity(merged_columns.len());
            for entry in merged_columns {
                match entry {
                    ColumnEntry::Structure(col_idx) => {
                        let plan = ai_structure_reviews
                            .iter()
                            .find(|sr| {
                                sr.parent_new_row_index == Some(idx)
                                    && matches_detail_path(sr, detail_ctx, *col_idx)
                            })
                            .map(|sr| build_structure_button_plan(sr, detail_ctx, None, Some(idx)))
                            .unwrap_or_else(|| structure_button_placeholder(None, Some(idx)));
                        columns.push((*entry, AiSuggestedCellPlan::Structure(plan)));
                    }
                    ColumnEntry::Regular(actual_col) => {
                        let position = nr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col);
                        let is_parent_key = is_parent_key_column(detail_ctx, *actual_col);
                        let has_linked = linked_column_options.contains_key(actual_col);
                        columns.push((
                            *entry,
                            AiSuggestedCellPlan::Regular(RegularAiCellPlan {
                                actual_col: *actual_col,
                                position,
                                is_parent_key,
                                has_linked_options: has_linked,
                            }),
                        ));
                    }
                }
            }

            Some(AiSuggestedPlan::NewPlain { columns })
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

            let mut columns = Vec::with_capacity(merged_columns.len());
            for entry in merged_columns {
                match entry {
                    ColumnEntry::Structure(col_idx) => {
                        let plan = ai_structure_reviews
                            .iter()
                            .find(|sr| {
                                sr.parent_new_row_index == Some(idx)
                                    && matches_detail_path(sr, detail_ctx, *col_idx)
                            })
                            .map(|sr| build_structure_button_plan(sr, detail_ctx, None, Some(idx)))
                            .unwrap_or_else(|| structure_button_placeholder(None, Some(idx)));
                        columns.push((*entry, AiSuggestedCellPlan::Structure(plan)));
                    }
                    ColumnEntry::Regular(actual_col) => {
                        let position = nr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col);
                        let is_parent_key = is_parent_key_column(detail_ctx, *actual_col);
                        let has_linked = linked_column_options.contains_key(actual_col);
                        columns.push((
                            *entry,
                            AiSuggestedCellPlan::Regular(RegularAiCellPlan {
                                actual_col: *actual_col,
                                position,
                                is_parent_key,
                                has_linked_options: has_linked,
                            }),
                        ));
                    }
                }
            }

            Some(AiSuggestedPlan::NewDuplicate {
                merge_decided: nr.merge_decided,
                merge_selected: nr.merge_selected,
                has_undecided_structures: has_undecided,
                columns,
            })
        }
    }
}
