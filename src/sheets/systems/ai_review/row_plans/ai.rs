use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_egui::egui::Color32;

use crate::sheets::systems::logic::structure_preview_logic::generate_structure_preview_from_rows_with_headers;
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
    let preview_rows = get_structure_preview_rows(sr);
    // Use schema_headers to properly skip technical columns
    // StructureReviewEntry rows don't include technical columns, so headers won't start with row_index/parent_key
    let preview = generate_structure_preview_from_rows_with_headers(
        preview_rows,
        Some(&sr.schema_headers),
    );
    let mut text = preview;
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

            // Calculate offset: In navigation drill-down (detail_ctx.is_some()), arrays have row_index and parent_key prepended
            // So array structure is: [row_index, parent_key, data...]
            // non_structure_columns refers to metadata columns (e.g., [1, 2] for parent_key and Tags)
            // But array indices are [0, 1, 2] with row_index at index 0
            // When we find a column's position in non_structure_columns, we need to add 1 to account for row_index
            // At parent level (detail_ctx.is_none()), arrays have NO prepending, so no offset needed
            let in_navigation_drilldown = detail_ctx.is_some();
            let needs_row_index_offset = in_navigation_drilldown;

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
                        let raw_pos = rr.non_structure_columns.iter().position(|c| c == actual_col);
                        // If in navigation drill-down, arrays have row_index at [0], so add 1 to position
                        let position = if needs_row_index_offset {
                            raw_pos.map(|pos| pos + 1)
                        } else {
                            raw_pos
                        };
                        let is_parent_key = is_parent_key_column(*actual_col, detail_ctx);
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
                has_undecided_structures: has_undecided,
                columns,
            })
        }
        RowKind::NewPlain => {
            let nr = state.ai_new_row_reviews.get(idx)?;
            
            // Check if in navigation drill-down (detail_ctx.is_some() means we're viewing child table)
            let in_navigation_drilldown = detail_ctx.is_some();
            let needs_row_index_offset = in_navigation_drilldown;
            
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
                        let raw_pos = nr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col);
                        // If in navigation drill-down, arrays have row_index at [0], so add 1 to position
                        let position = if needs_row_index_offset {
                            raw_pos.map(|pos| pos + 1)
                        } else {
                            raw_pos
                        };
                        let is_parent_key = is_parent_key_column(*actual_col, detail_ctx);
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

            // Check if in navigation drill-down (detail_ctx.is_some() means we're viewing child table)
            let in_navigation_drilldown = detail_ctx.is_some();
            let needs_row_index_offset = in_navigation_drilldown;

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
                        let raw_pos = nr
                            .non_structure_columns
                            .iter()
                            .position(|c| c == actual_col);
                        // If in navigation drill-down, arrays have row_index at [0], so add 1 to position
                        let position = if needs_row_index_offset {
                            raw_pos.map(|pos| pos + 1)
                        } else {
                            raw_pos
                        };
                        let is_parent_key = is_parent_key_column(*actual_col, detail_ctx);
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
