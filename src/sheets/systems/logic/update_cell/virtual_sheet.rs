// src/sheets/systems/logic/update_cell/virtual_sheet.rs
//! Virtual sheet synchronization logic

use crate::sheets::{
    definitions::SheetMetadata,
    events::SheetDataModifiedInRegistryEvent,
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::{EditorWindowState, StructureParentContext};
use bevy::prelude::*;
use std::collections::HashMap;

/// Checks if a sheet is virtual and returns its parent context
pub fn get_virtual_sheet_context(
    sheet_name: &str,
    editor_state: &Option<Res<EditorWindowState>>,
) -> Option<StructureParentContext> {
    if !sheet_name.starts_with("__virtual__") {
        return None;
    }
    
    if let Some(state) = editor_state.as_ref() {
        state
            .virtual_structure_stack
            .iter()
            .find(|v| v.virtual_sheet_name == sheet_name)
            .map(|vctx| vctx.parent.clone())
    } else {
        None
    }
}

/// Synchronizes virtual sheet changes back to parent cell
pub fn sync_virtual_sheet_to_parent(
    registry: &mut SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    parent_ctx: &StructureParentContext,
    sheets_to_save: &mut HashMap<(Option<String>, String), SheetMetadata>,
    sheets_to_revalidate: &mut HashMap<(Option<String>, String), ()>,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
) -> Result<(), String> {
    // Get virtual sheet grid (immutable borrow)
    let v_rows = {
        let vsheet = registry.get_sheet(category, sheet_name)
            .ok_or("Virtual sheet not found")?;
        
        if vsheet.metadata.is_none() {
            return Err("Virtual sheet has no metadata".to_string());
        }
        
        vsheet.grid.clone()
    }; // Release immutable borrow
    
    // Update parent cell (mutable borrow)
    let parent_sheet_data = registry
        .get_sheet_mut(&parent_ctx.parent_category, &parent_ctx.parent_sheet)
        .ok_or("Parent sheet not found")?;
    
    if parent_ctx.parent_row >= parent_sheet_data.grid.len() {
        return Err("Parent row out of bounds".to_string());
    }
    
    let parent_row = parent_sheet_data.grid.get_mut(parent_ctx.parent_row)
        .ok_or("Parent row not found")?;
    
    if parent_ctx.parent_col >= parent_row.len() {
        return Err("Parent column out of bounds".to_string());
    }
    
    // Build JSON representation of virtual sheet
    let new_json = if v_rows.len() <= 1 {
        // Single row => store as array of strings
        let row_vals = v_rows.get(0).cloned().unwrap_or_default();
        serde_json::Value::Array(
            row_vals
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        )
        .to_string()
    } else {
        // Multi row => array of arrays
        let outer: Vec<serde_json::Value> = v_rows
            .iter()
            .map(|r| {
                serde_json::Value::Array(
                    r.iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                )
            })
            .collect();
        serde_json::Value::Array(outer).to_string()
    };
    
    let cell_ref = parent_row.get_mut(parent_ctx.parent_col)
        .ok_or("Parent cell not found")?;
    
    if *cell_ref != new_json {
        *cell_ref = new_json.clone();
        
        if let Some(pmeta) = &parent_sheet_data.metadata {
            let key = (
                parent_ctx.parent_category.clone(),
                parent_ctx.parent_sheet.clone(),
            );
            sheets_to_save.insert(key.clone(), pmeta.clone());
            sheets_to_revalidate.insert(key.clone(), ());
            
            data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                category: parent_ctx.parent_category.clone(),
                sheet_name: parent_ctx.parent_sheet.clone(),
            });
        }
    }
    
    Ok(())
}
