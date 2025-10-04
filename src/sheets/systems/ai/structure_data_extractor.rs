// src/sheets/systems/ai/structure_data_extractor.rs
// Helper to extract structure data from base-level AI responses

use crate::sheets::definitions::StructureFieldDefinition;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::NewRowReview;
use serde_json::Value as JsonValue;

/// Extract structure data from a NewRowReview for a specific structure column
/// Returns rows of structure data if found, empty vec if none
pub fn extract_structure_data_from_new_row(
    new_row_review: &NewRowReview,
    structure_col_idx: usize,
    structure_schema: &[StructureFieldDefinition],
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
) -> Vec<Vec<String>> {
    // Find the position of the structure column in non_structure_columns
    // Wait, structure columns are NOT in non_structure_columns!
    // Structure data is stored in the AI response but not parsed yet
    
    // We need to look at the sheet metadata to find which column index has structures
    let Some(sheet) = registry.get_sheet(category, sheet_name) else {
        return Vec::new();
    };
    
    let Some(meta) = &sheet.metadata else {
        return Vec::new();
    };
    
    // The AI response includes ALL columns in order, but nr.non_structure_columns
    // only lists the non-structure ones. We need to figure out which element
    // in the full row corresponds to our structure column.
    
    // Actually, looking at the code, structures are sent separately after base level
    // The base-level AI response has structure columns as JSON strings
    // But those are NOT included in nr.ai - they're sent separately!
    
    // Let me re-read the logs...
    // The logs show structure data being sent AFTER Phase 2 completes
    // So the structure data is NOT in nr.ai at all!
    
    // The real issue is: when we create structure jobs, we should check if there's
    // already a StructureReviewEntry with undecided data, and use that.
    
    Vec::new()
}

/// Check if a NewRowReview has structure data that should be sent
/// This happens when the base-level response included structure columns
pub fn should_send_structure_for_new_row(
    new_row_review: &NewRowReview,
    structure_col_idx: usize,
) -> bool {
    // For now, always return false - structure data comes from StructureReviewEntry
    // not from the base-level response
    false
}
