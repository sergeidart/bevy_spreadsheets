// src/sheets/systems/ai/structure_results/builder.rs
// Functions for building parent rows from context

use crate::sheets::definitions::SheetGridData;
use crate::ui::elements::editor::state::StructureNewRowContext;

/// Build parent row from context (synthetic for new rows or from sheet)
pub fn build_parent_row(
    new_row_context: &Option<StructureNewRowContext>,
    sheet: &SheetGridData,
    parent_row_index: usize,
    num_columns: usize,
) -> Vec<String> {
    if let Some(ctx) = new_row_context {
        // Start with full_ai_row if available (includes structure columns as JSON)
        // Otherwise create empty row and populate with non-structure values
        let mut synthetic_row = if let Some(ref full_row) = ctx.full_ai_row {
            let mut row = full_row.clone();
            if row.len() < num_columns {
                row.resize(num_columns, String::new());
            }
            row
        } else {
            vec![String::new(); num_columns]
        };

        // Override with non_structure_values (these are the user-facing values)
        for (col_idx, value) in &ctx.non_structure_values {
            if let Some(slot) = synthetic_row.get_mut(*col_idx) {
                *slot = value.clone();
            }
        }
        synthetic_row
    } else {
        let mut row = sheet
            .grid
            .get(parent_row_index)
            .cloned()
            .unwrap_or_default();
        if row.len() < num_columns {
            row.resize(num_columns, String::new());
        }
        row
    }
}
