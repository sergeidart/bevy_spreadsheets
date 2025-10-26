// src/ui/elements/popups/column_options_validator/schema_helpers.rs
// Helper functions for extracting schema information from metadata

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Gets the headers and key information for existing structure validator
/// 
/// Returns: (headers, current_key_index, exclude_index)
pub fn get_existing_structure_key_info(
    state: &EditorWindowState,
    meta: &crate::sheets::definitions::SheetMetadata,
    registry_immut: &SheetRegistry,
) -> (Vec<String>, Option<usize>, Option<usize>) {
    if let Some(parent_link) = &meta.structure_parent {
        // In virtual structure view (Str1): choose from THIS level's headers (Str1 fields),
        // but read the key from the parent field's stored key for this virtual column (authoritative store)
        let headers = meta
            .columns
            .iter()
            .map(|c| c
                .display_header
                .as_ref()
                .cloned()
                .unwrap_or_else(|| c.header.clone()))
            .collect::<Vec<_>>();
        let current_key = if let Some(parent_sheet) = registry_immut
            .get_sheet(
                &parent_link.parent_category,
                &parent_link.parent_sheet,
            ) {
            if let Some(parent_meta) = &parent_sheet.metadata {
                parent_meta
                    .columns
                    .get(parent_link.parent_column_index)
                    .and_then(|pcol| pcol.structure_schema.as_ref())
                    .and_then(|fields| {
                        fields.get(state.options_column_target_index)
                    })
                    .and_then(|f| f.structure_key_parent_column_index)
            } else {
                None
            }
        } else {
            None
        };
        (
            headers,
            current_key,
            Some(state.options_column_target_index),
        )
    } else {
        // Editing on the parent (non-virtual): use this sheet's headers and this column's key index
        let headers = meta
            .columns
            .iter()
            .map(|c| c
                .display_header
                .as_ref()
                .cloned()
                .unwrap_or_else(|| c.header.clone()))
            .collect::<Vec<_>>();
        let current_key = meta
            .columns
            .get(state.options_column_target_index)
            .and_then(|c| c.structure_key_parent_column_index);
        (
            headers,
            current_key,
            Some(state.options_column_target_index),
        )
    }
}

/// Gets headers for new structure creation (excludes deleted columns)
pub fn get_new_structure_headers(
    meta: &crate::sheets::definitions::SheetMetadata,
) -> Vec<String> {
    // Preserve indices: return a label per column using display_header when present.
    meta.columns
        .iter()
        .map(|c| c
            .display_header
            .as_ref()
            .cloned()
            .unwrap_or_else(|| c.header.clone()))
        .collect()
}

/// Gets all headers from metadata
pub fn get_all_headers(
    meta: &crate::sheets::definitions::SheetMetadata,
) -> Vec<String> {
    meta.columns
        .iter()
        .map(|c| c
            .display_header
            .as_ref()
            .cloned()
            .unwrap_or_else(|| c.header.clone()))
        .collect()
}

/// Gets headers with their indices for linked column selection
pub fn get_headers_with_indices(
    meta: &crate::sheets::definitions::SheetMetadata,
) -> Vec<(usize, String)> {
    meta.columns
        .iter()
        .map(|c| c.header.clone())
        .enumerate()
        .collect()
}

