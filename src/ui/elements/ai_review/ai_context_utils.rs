// Moved from editor/ai_context_utils.rs to ai_review/ai_context_utils.rs
use crate::sheets::definitions::{ColumnDataType, ColumnValidator, SheetMetadata};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use crate::sheets::systems::logic::lineage_helpers::{walk_parent_lineage, gather_lineage_ai_contexts};
use bevy::prelude::*;
use std::collections::HashMap;

pub fn decorate_context_with_type(
    context: Option<&String>,
    data_type: ColumnDataType,
) -> Option<String> {
    let ctx_ref = context?;
    let trimmed = ctx_ref.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let type_label = match data_type {
        ColumnDataType::String => Some("String"),
        ColumnDataType::Bool => Some("Bool"),
        ColumnDataType::I64 => Some("Integer"),
        ColumnDataType::F64 => Some("Float"),
    };

    if let Some(label) = type_label {
        let mut result = trimmed.to_string();
        // Use raw string to avoid escape interpretation
        result.push_str(" \\ ");
        result.push_str(label);
        Some(result)
    } else {
        Some(ctx_ref.clone())
    }
}

/// Result of column inclusion analysis for AI requests
pub struct ColumnInclusion {
    pub included_indices: Vec<usize>,
    pub column_contexts: Vec<Option<String>>,
}

/// Collect columns that should be included in AI requests, filtering out
/// deleted, structure, and technical columns
pub fn collect_ai_included_columns(
    meta: &SheetMetadata,
    _in_structure_sheet: bool,
) -> ColumnInclusion {
    let mut included_indices = Vec::new();
    let mut column_contexts = Vec::new();
    
    for (idx, col) in meta.columns.iter().enumerate() {
        info!(
            "AI Column Filter: idx={}, header='{}', deleted={}, ai_include={:?}, validator={:?}",
            idx, col.header, col.deleted, col.ai_include_in_send, col.validator
        );
        
        if col.deleted {
            info!("  → SKIP: deleted");
            continue;
        }
        
        if matches!(
            col.validator,
            Some(ColumnValidator::Structure)
        ) {
            info!("  → SKIP: structure validator");
            continue;
        }
        
        if matches!(col.ai_include_in_send, Some(false)) {
            info!("  → SKIP: ai_include_in_send=false");
            continue;
        }
        
        // Omit technical columns from payload to AI ALWAYS
        if col.header.eq_ignore_ascii_case("row_index")
            || col.header.eq_ignore_ascii_case("id")
            || col.header.eq_ignore_ascii_case("parent_key")
        {
            info!("  → SKIP: technical column");
            continue;
        }
        
        // Store actual grid column index expected by downstream processors
        let actual_grid_idx = if meta.is_structure_table() {
            meta.metadata_index_to_grid_index(idx)
        } else {
            idx
        };
        
        info!("  → INCLUDE: grid_idx={}", actual_grid_idx);
        included_indices.push(actual_grid_idx);
        column_contexts.push(decorate_context_with_type(
            col.ai_context.as_ref(),
            col.data_type,
        ));
    }
    
    ColumnInclusion {
        included_indices,
        column_contexts,
    }
}

/// Result of lineage prefix building
pub struct LineagePrefixes {
    pub key_prefix_count: usize,
    #[allow(dead_code)]
    pub key_prefix_headers: Option<Vec<String>>,
    pub prefix_contexts: Vec<Option<String>>,
    pub prefix_values: Vec<String>,
    pub prefix_pairs_by_row: HashMap<usize, Vec<(String, String)>>,
}

impl Default for LineagePrefixes {
    fn default() -> Self {
        Self {
            key_prefix_count: 0,
            key_prefix_headers: None,
            prefix_contexts: Vec::new(),
            prefix_values: Vec::new(),
            prefix_pairs_by_row: HashMap::new(),
        }
    }
}

/// Build lineage prefixes for AI requests using programmatic lineage walking.
/// Handles both virtual structure and real structure navigation contexts.
pub fn build_lineage_prefixes(
    state: &EditorWindowState,
    registry: &SheetRegistry,
    selection: &[usize],
) -> LineagePrefixes {
    // Collect prefix columns (ancestor key context) to prepend to rows
    let mut headers: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let mut prefix_contexts: Vec<Option<String>> = Vec::new();
    
    // Real structure navigation
    if !state.structure_navigation_stack.is_empty() {
        if let Some(nav_ctx) = state.structure_navigation_stack.last() {
            // Walk parent lineage to get full ancestry
            if let Some(parent_sheet) = registry.get_sheet(&nav_ctx.parent_category, &nav_ctx.parent_sheet_name) {
                if parent_sheet.metadata.is_some() {
                    // Parse parent_row_key as row_index
                    if let Ok(parent_row_idx) = nav_ctx.parent_row_key.parse::<usize>() {
                        info!("🔍 DEBUG: Starting lineage walk for parent_sheet='{}', parent_row_idx={}", 
                              nav_ctx.parent_sheet_name, parent_row_idx);
                        
                        // Get complete lineage starting from parent (includes parent itself + ancestors)
                        let lineage = walk_parent_lineage(
                            registry,
                            &nav_ctx.parent_category,
                            &nav_ctx.parent_sheet_name,
                            parent_row_idx,
                        );
                        
                        info!("🔍 DEBUG: walk_parent_lineage returned {} entries: {:?}", 
                              lineage.len(), 
                              lineage.iter().map(|(t, d, i)| format!("{}[{}]={}", t, i, d)).collect::<Vec<_>>());
                        
                        // Gather AI contexts from lineage for better AI understanding
                        let lineage_contexts = gather_lineage_ai_contexts(registry, &lineage);
                        
                        // Add each lineage entry
                        for ((table_name, display_value, _row_idx), ai_ctx) in lineage.iter().zip(lineage_contexts.iter()) {
                            // Get the first data column header from the table
                            if let Some(tbl_sheet) = registry.get_sheet(&nav_ctx.parent_category, table_name) {
                                if let Some(tbl_meta) = &tbl_sheet.metadata {
                                    if let Some(di) = get_first_data_col_idx(tbl_meta) {
                                        let header = tbl_meta.columns.get(di).map(|c| c.header.clone()).unwrap_or_else(|| "Key".to_string());
                                        headers.push(header.clone());
                                        values.push(display_value.clone());
                                        let col_def = &tbl_meta.columns[di];
                                        
                                        // Use lineage context if available, otherwise fall back to column context
                                        let context_to_use = if !ai_ctx.is_empty() {
                                            Some(ai_ctx.clone())
                                        } else {
                                            col_def.ai_context.clone()
                                        };
                                        
                                        prefix_contexts.push(decorate_context_with_type(context_to_use.as_ref(), col_def.data_type));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Build result
    if values.is_empty() {
        return LineagePrefixes::default();
    }
    
    info!(
        "Lineage Prefixes Built: headers={:?}, values={:?}, prefix_contexts.len()={}",
        headers, values, prefix_contexts.len()
    );
    
    let key_prefix_count = values.len();
    let key_prefix_headers = Some(headers.clone());
    
    // Build pairs for review UI fallback (keyed by row_index)
    let pairs: Vec<(String, String)> = headers
        .iter()
        .cloned()
        .zip(values.iter().cloned())
        .collect();
    
    let mut prefix_pairs_by_row = HashMap::new();
    for &row_index in selection {
        prefix_pairs_by_row.insert(row_index, pairs.clone());
    }
    
    LineagePrefixes {
        key_prefix_count,
        key_prefix_headers,
        prefix_contexts,
        prefix_values: values,
        prefix_pairs_by_row,
    }
}

/// Helper to get first non-technical column index in metadata
fn get_first_data_col_idx(meta: &SheetMetadata) -> Option<usize> {
    meta.columns.iter().position(|col| {
        let lower = col.header.to_lowercase();
        lower != "row_index"
            && lower != "parent_key"
            && lower != "id"
            && lower != "created_at"
            && lower != "updated_at"
    })
}
