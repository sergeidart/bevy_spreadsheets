// src/sheets/systems/ai/throttled.rs
use crate::ui::elements::editor::state::{EditorWindowState, ThrottledAiAction};
use bevy::prelude::*;

pub fn apply_throttled_ai_changes(
    mut state: ResMut<EditorWindowState>,
    mut cell_update_writer: EventWriter<crate::sheets::events::UpdateCellEvent>,
    mut add_row_writer: EventWriter<crate::sheets::events::AddSheetRowRequest>,
) {
    // Process multiple operations per frame for better throughput
    // Limit to avoid frame drops, but allow batching for efficiency
    const MAX_OPERATIONS_PER_FRAME: usize = 5;
    
    let queue_size_before = state.ai_throttled_apply_queue.len();
    let mut operations_processed = 0;
    
    while let Some(action) = state.ai_throttled_apply_queue.pop_front() {
        let (cat, sheet_opt) = state.current_sheet_context();
        if let Some(sheet) = sheet_opt.clone() {
            match action {
                ThrottledAiAction::UpdateCell {
                    row_index,
                    col_index,
                    value,
                } => {
                    cell_update_writer.write(crate::sheets::events::UpdateCellEvent {
                        category: cat.clone(),
                        sheet_name: sheet,
                        row_index,
                        col_index,
                        new_value: value,
                    });
                }
                ThrottledAiAction::AddRow { initial_values } => {
                    add_row_writer.write(crate::sheets::events::AddSheetRowRequest {
                        category: cat.clone(),
                        sheet_name: sheet,
                        initial_values: if initial_values.is_empty() {
                            None
                        } else {
                            Some(initial_values)
                        },
                    });
                }
            }
        }
        
        operations_processed += 1;
        if operations_processed >= MAX_OPERATIONS_PER_FRAME {
            break;
        }
    }
    
    // Log progress when processing large batches
    if queue_size_before > 10 && operations_processed > 0 {
        let remaining = state.ai_throttled_apply_queue.len();
        trace!(
            "Processed {} AI operations ({} remaining in queue)",
            operations_processed,
            remaining
        );
    }
}
