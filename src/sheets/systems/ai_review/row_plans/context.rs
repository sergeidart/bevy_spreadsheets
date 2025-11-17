use crate::ui::elements::editor::state::{StructureDetailContext, StructureReviewEntry};

pub(super) fn get_structure_preview_rows(sr: &StructureReviewEntry) -> &[Vec<String>] {
    if sr.decided && !sr.merged_rows.is_empty() {
        &sr.merged_rows
    } else {
        &sr.ai_rows
    }
}

pub(super) fn matches_detail_path(
    sr: &StructureReviewEntry,
    detail_ctx: Option<&StructureDetailContext>,
    col_idx: usize,
) -> bool {
    if let Some(ctx) = detail_ctx {
        if sr.structure_path.len() <= ctx.structure_path.len() {
            return false;
        }
        sr.structure_path[..ctx.structure_path.len()] == ctx.structure_path[..]
            && sr.structure_path.get(ctx.structure_path.len()) == Some(&col_idx)
    } else {
        sr.structure_path.first() == Some(&col_idx)
    }
}

pub(super) fn has_undecided_structures_in_context(
    ai_structure_reviews: &[StructureReviewEntry],
    detail_ctx: Option<&StructureDetailContext>,
    parent_row_index: Option<usize>,
    parent_new_row_index: Option<usize>,
) -> bool {
    ai_structure_reviews.iter().any(|sr| {
        let row_matches = match (parent_row_index, parent_new_row_index) {
            (Some(row_idx), None) => {
                sr.parent_row_index == row_idx && sr.parent_new_row_index.is_none()
            }
            (None, Some(new_idx)) => sr.parent_new_row_index == Some(new_idx),
            _ => false,
        };

        if !row_matches || !sr.is_undecided() {
            return false;
        }

        if let Some(ctx) = detail_ctx {
            sr.structure_path.len() > ctx.structure_path.len()
                && sr.structure_path[..ctx.structure_path.len()] == ctx.structure_path[..]
        } else {
            true
        }
    })
}

pub(super) fn is_parent_key_column(actual_col: usize, detail_ctx: Option<&StructureDetailContext>) -> bool {
    // parent_key is column 1 only in structure/child tables (when detail_ctx is Some)
    detail_ctx.is_some() && actual_col == 1
}
