// src/ui/elements/ai_review/structure_review_helpers.rs
// Helper functions for structure review display context

use crate::sheets::definitions::{ColumnValidator, StructureFieldDefinition};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
/// Build column list from structure schema
/// If in_virtual_structure_review is true, filters out structure columns (nested structures)
pub fn build_structure_columns(
    union_cols: &[usize],
    detail_ctx: &Option<crate::ui::elements::editor::state::StructureDetailContext>,
    in_virtual_structure_review: bool,
    virtual_sheet_name: &str,
    _selected_category: &Option<String>,
    registry: &SheetRegistry,
) -> (Vec<ColumnEntry>, Vec<StructureFieldDefinition>) {
    // If in virtual structure review mode, filter out structure columns
    if in_virtual_structure_review {
        let mut result = Vec::new();
        // Find the virtual sheet to get its metadata
        if let Some(sheet) = registry
            .iter_sheets()
            .find(|(_, name, _)| *name == virtual_sheet_name)
            .and_then(|(_, _, sheet)| Some(sheet))
        {
            if let Some(meta) = &sheet.metadata {
                for &col_idx in union_cols {
                    if let Some(col_def) = meta.columns.get(col_idx) {
                        let is_structure =
                            matches!(col_def.validator, Some(ColumnValidator::Structure));
                        let is_included = !matches!(col_def.ai_include_in_send, Some(false));

                        // EXCLUDE structure columns in virtual structure review
                        // (they are nested structures and shouldn't be navigable)
                        // Also exclude columns not included by schema groups
                        if !is_structure && is_included {
                            result.push(ColumnEntry::Regular(col_idx));
                        }
                    }
                }
            }
        }
        return (result, Vec::new());
    }

    let detail_ctx = match detail_ctx {
        Some(ctx) => ctx,
        None => return (Vec::new(), Vec::new()),
    };

    // Build child table name and get from registry (already loaded when AI Review started)
    let parent_sheet = &detail_ctx.root_sheet;
    let parent_meta = registry
        .get_sheet(&detail_ctx.root_category, parent_sheet)
        .and_then(|s| s.metadata.as_ref());
    
    let column_header = parent_meta
        .and_then(|meta| detail_ctx.structure_path.first().and_then(|&idx| meta.columns.get(idx)))
        .map(|col| col.header.as_str())
        .unwrap_or("");
    
    let child_table_name = format!("{}_{}", parent_sheet, column_header);
    
    // Get the child sheet from registry (should already be loaded)
    let child_meta = registry
        .get_sheet(&detail_ctx.root_category, &child_table_name)
        .and_then(|s| s.metadata.as_ref());
    
    // Build column entries from child sheet metadata - SAME as regular sheet view
    let mut result = Vec::new();
    let mut schema_fields = Vec::new();
    
    let Some(child_meta) = child_meta else {
        bevy::log::error!(
            "Child sheet '{}' not loaded in registry - tables should be loaded when AI Review starts",
            child_table_name
        );
        return (Vec::new(), Vec::new());
    };
    
    for (col_idx, col_def) in child_meta.columns.iter().enumerate() {
        let is_structure = matches!(col_def.validator, Some(ColumnValidator::Structure));
        
        // Skip technical columns (row_index=0, parent_key=1)
        if col_idx == 0 || col_idx == 1 {
            continue;
        }
        
        // Show data columns from the child sheet
        if is_structure {
            result.push(ColumnEntry::Structure(col_idx));
        } else {
            result.push(ColumnEntry::Regular(col_idx));
        }
        
        // Convert to StructureFieldDefinition for compatibility
        schema_fields.push(StructureFieldDefinition {
            header: col_def.header.clone(),
            data_type: col_def.data_type,
            validator: col_def.validator.clone(),
            filter: col_def.filter.clone(),
            ai_context: col_def.ai_context.clone(),
            ai_include_in_send: col_def.ai_include_in_send,
            ai_enable_row_generation: col_def.ai_enable_row_generation,
            width: col_def.width,
            structure_schema: col_def.structure_schema.clone(),
            structure_column_order: col_def.structure_column_order.clone(),
            structure_key_parent_column_index: col_def.structure_key_parent_column_index,
            structure_ancestor_key_parent_column_indices: col_def.structure_ancestor_key_parent_column_indices.clone(),
        });
    }

    (result, schema_fields)
}


