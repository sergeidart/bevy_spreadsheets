use bevy_egui::egui::Color32;

use crate::ui::common::generate_structure_preview_from_rows;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::{EditorWindowState, StructureReviewEntry};

use super::blocks::RowKind;
use super::context::{get_structure_preview_rows, matches_detail_path};

#[derive(Debug, Clone)]
pub struct StatusCellPlan {
    pub text: Option<String>,
    pub color: Option<Color32>,
    pub highlight: bool,
}

#[derive(Debug, Clone)]
pub enum StatusActionPlan {
    None,
    DecideButton,
    MergeLabel { text: String, color: Color32 },
}

#[derive(Debug, Clone)]
pub enum StatusRowPlan {
    Existing {
        action: StatusActionPlan,
        columns: Vec<(ColumnEntry, StatusCellPlan)>,
    },
    NewPlain {
        action: StatusActionPlan,
    },
    NewDuplicate {
        merge_decided: bool,
        merge_selected: bool,
        action: StatusActionPlan,
        columns: Vec<(ColumnEntry, StatusCellPlan)>,
    },
}

pub fn prepare_status_row_plan(
    state: &EditorWindowState,
    ai_structure_reviews: &[StructureReviewEntry],
    merged_columns: &[ColumnEntry],
    kind: RowKind,
    idx: usize,
) -> Option<StatusRowPlan> {
    match kind {
        RowKind::Existing => {
            let _ = state.ai_row_reviews.get(idx)?;
            let columns = merged_columns
                .iter()
                .map(|entry| {
                    (
                        *entry,
                        StatusCellPlan {
                            text: None,
                            color: None,
                            highlight: false,
                        },
                    )
                })
                .collect();
            Some(StatusRowPlan::Existing {
                action: StatusActionPlan::None,
                columns,
            })
        }
        RowKind::NewPlain => {
            let _ = state.ai_new_row_reviews.get(idx)?;
            Some(StatusRowPlan::NewPlain {
                action: StatusActionPlan::None,
            })
        }
        RowKind::NewDuplicate => {
            let nr = state.ai_new_row_reviews.get(idx)?;
            let action = if nr.merge_decided && nr.merge_selected {
                StatusActionPlan::MergeLabel {
                    text: "Merge Choices".to_string(),
                    color: Color32::from_rgb(180, 160, 40),
                }
            } else if !nr.merge_decided {
                StatusActionPlan::DecideButton
            } else {
                StatusActionPlan::None
            };

            let mut columns = Vec::with_capacity(merged_columns.len());
            for entry in merged_columns {
                match entry {
                    ColumnEntry::Structure(col_idx) => {
                        if nr.merge_decided && nr.merge_selected {
                            if let Some(match_row) = nr.duplicate_match_row {
                                let sr_opt = ai_structure_reviews.iter().find(|sr| {
                                    sr.parent_new_row_index.is_none()
                                        && sr.parent_row_index == match_row
                                        && matches_detail_path(sr, None, *col_idx)
                                });

                                if let Some(sr) = sr_opt {
                                    let preview = generate_structure_preview_from_rows(
                                        get_structure_preview_rows(sr),
                                    );
                                    let text = if preview.is_empty() {
                                        "(empty)".to_string()
                                    } else {
                                        preview
                                    };
                                    columns.push((
                                        *entry,
                                        StatusCellPlan {
                                            text: Some(text),
                                            color: if sr.decided {
                                                if sr.accepted {
                                                    Some(Color32::from_rgb(0, 200, 0))
                                                } else if sr.rejected {
                                                    Some(Color32::from_rgb(150, 150, 150))
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            },
                                            highlight: false,
                                        },
                                    ));
                                    continue;
                                }
                            }
                        }

                        columns.push((
                            *entry,
                            StatusCellPlan {
                                text: None,
                                color: None,
                                highlight: false,
                            },
                        ));
                    }
                    ColumnEntry::Regular(_) => {
                        columns.push((
                            *entry,
                            StatusCellPlan {
                                text: None,
                                color: None,
                                highlight: false,
                            },
                        ));
                    }
                }
            }

            Some(StatusRowPlan::NewDuplicate {
                merge_decided: nr.merge_decided,
                merge_selected: nr.merge_selected,
                action,
                columns,
            })
        }
    }
}
