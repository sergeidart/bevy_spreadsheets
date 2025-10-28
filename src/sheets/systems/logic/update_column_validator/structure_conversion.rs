// src/sheets/systems/logic/update_column_validator/structure_conversion.rs
// Structure column conversion logic (TO and FROM)

use bevy::prelude::*;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnDefinition, ColumnValidator, StructureFieldDefinition},
    events::RequestUpdateColumnValidator,
};

use super::hierarchy::create_structure_technical_columns;

fn sanitize_column_header(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    while out.starts_with('_') { out.remove(0); }
    while out.ends_with('_') { out.pop(); }
    if out.is_empty() { out.push_str("new_column"); }
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert_str(0, "c_");
    }
    out
}

/// Handle conversion TO Structure validator
/// Returns (collected_defs, value_sources, structure_columns) for sheet creation
/// 
/// hierarchy_depth: The nesting level of the parent sheet (0 for root, 1 for first level child, etc.)
///                  The child structure will be at depth + 1
pub fn handle_structure_conversion_to(
    event: &RequestUpdateColumnValidator,
    col_index: usize,
    old_col_def_snapshot: &ColumnDefinition,
    columns_snapshot: &[ColumnDefinition],
    _sheet_name: &str,
    _column_header: &str,
    hierarchy_depth: usize,
) -> Option<(
    Vec<StructureFieldDefinition>,
    Vec<(usize, bool)>,
    Vec<ColumnDefinition>,
)> {
    let sources = match event.structure_source_columns.as_ref() {
        Some(s) => {
            info!("handle_structure_conversion_to: Got {} source columns: {:?}", s.len(), s);
            s
        }
        None => {
            error!("handle_structure_conversion_to: event.structure_source_columns is None! Cannot create structure schema without source columns.");
            error!("  This means no columns were selected in the UI when creating the structure validator.");
            error!("  Please select at least one column to include in the structure.");
            return None;
        }
    };

    // Pre-collect source column definitions to avoid borrowing issues
    let mut seen = std::collections::HashSet::new();
    let mut collected_defs: Vec<StructureFieldDefinition> = Vec::new();
    // (src_index, is_self)
    let mut value_sources: Vec<(usize, bool)> = Vec::new();

    for src in sources.iter().copied() {
        if seen.insert(src) {
            // Get source column to check if it's from parent table (not the column being converted)
            if src != col_index {
                // For columns from parent table, filter out deleted, hidden, and technical
                if let Some(src_col) = columns_snapshot.get(src) {
                    // Filter out deleted columns
                    if src_col.deleted {
                        debug!("Skipping deleted column '{}' from parent table", src_col.header);
                        continue;
                    }
                    
                    // Filter out hidden technical columns from parent
                    if src_col.hidden {
                        debug!("Skipping hidden technical column '{}' from parent table", src_col.header);
                        continue;
                    }
                    
                    // Filter out parent's technical columns by name
                    let is_technical = src_col.header.eq_ignore_ascii_case("row_index")
                        || src_col.header.eq_ignore_ascii_case("parent_key");
                    
                    if is_technical {
                        debug!("Skipping parent's technical column '{}'", src_col.header);
                        continue;
                    }
                } else {
                    warn!("Source column index {} not found in parent table", src);
                    continue;
                }
            }
            
            if src == col_index {
                let mut def = StructureFieldDefinition::from(old_col_def_snapshot);
                if let Some(orig) = event.original_self_validator.clone() {
                    def.validator = Some(orig.clone());
                    def.data_type = match orig {
                        ColumnValidator::Basic(t) => t,
                        ColumnValidator::Linked { .. } => ColumnDataType::String,
                        ColumnValidator::Structure => ColumnDataType::String,
                    };
                }
                if matches!(def.validator, Some(ColumnValidator::Structure)) {
                    def.validator = Some(ColumnValidator::Basic(ColumnDataType::String));
                    def.data_type = ColumnDataType::String;
                }
                collected_defs.push(def);
                value_sources.push((src, true));
            } else if let Some(src_col) = columns_snapshot.get(src) {
                collected_defs.push(StructureFieldDefinition::from(src_col));
                value_sources.push((src, false));
            }
        }
    }
    
    // Create technical columns based on the child's hierarchy depth (parent's depth + 1)
    let child_depth = hierarchy_depth + 1;
    info!(
        "Creating structure sheet: parent depth = {}, child depth = {}, will create {} technical columns",
        hierarchy_depth,
        child_depth,
        if child_depth == 0 { 1 } else if child_depth == 1 { 2 } else { child_depth + 1 }
    );
    let mut structure_columns = create_structure_technical_columns(child_depth);
    info!(
        "Created {} total technical columns for structure sheet",
        structure_columns.len()
    );
    info!("Technical columns created:");
    for (i, col) in structure_columns.iter().enumerate() {
        info!("  [{}] '{}' (type: {:?})", i, col.header, col.data_type);
    }

    // Add columns from the structure schema (user-defined data columns)
    for (j, field_def) in collected_defs.iter().enumerate() {
        let (src_idx, _is_self) = value_sources[j];
        let ui_label = columns_snapshot
            .get(src_idx)
            .and_then(|c| c.display_header.as_ref().cloned())
            .unwrap_or_else(|| columns_snapshot.get(src_idx).map(|c| c.header.clone()).unwrap_or_else(|| field_def.header.clone()));
        let sanitized = sanitize_column_header(&ui_label);
        structure_columns.push(ColumnDefinition {
            header: sanitized,
            display_header: Some(ui_label),
            data_type: field_def.data_type,
            validator: field_def.validator.clone(),
            filter: None,
            ai_context: field_def.ai_context.clone(),
            ai_enable_row_generation: field_def.ai_enable_row_generation,
            ai_include_in_send: field_def.ai_include_in_send,
            deleted: false,
            hidden: false, // User-defined structure field
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        });
    }

    Some((collected_defs, value_sources, structure_columns))
}



