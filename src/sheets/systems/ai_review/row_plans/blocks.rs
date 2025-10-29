use std::collections::HashSet;

use crate::ui::elements::editor::state::EditorWindowState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowBlock {
    OriginalPreview(usize, RowKind),
    AiSuggested(usize, RowKind),
    Separator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Existing,
    NewPlain,
    NewDuplicate,
}

pub fn build_blocks(state: &EditorWindowState) -> (Vec<RowBlock>, HashSet<usize>) {
    let mut blocks = Vec::new();
    let mut starts = HashSet::new();
    let mut push_group = |items: Vec<RowBlock>| {
        if items.is_empty() {
            return;
        }
        if !blocks.is_empty() {
            // Insert a visual separator between groups
            blocks.push(RowBlock::Separator);
        }
        let start = blocks.len();
        starts.insert(start);
        blocks.extend(items);
    };

    for i in 0..state.ai_row_reviews.len() {
        push_group(vec![
            RowBlock::OriginalPreview(i, RowKind::Existing),
            RowBlock::AiSuggested(i, RowKind::Existing),
        ]);
    }

    for i in 0..state.ai_new_row_reviews.len() {
        let review = &state.ai_new_row_reviews[i];
        if review.duplicate_match_row.is_some() {
            push_group(vec![
                RowBlock::OriginalPreview(i, RowKind::NewDuplicate),
                RowBlock::AiSuggested(i, RowKind::NewDuplicate),
            ]);
        } else {
            push_group(vec![
                RowBlock::OriginalPreview(i, RowKind::NewPlain),
                RowBlock::AiSuggested(i, RowKind::NewPlain),
            ]);
        }
    }

    (blocks, starts)
}
