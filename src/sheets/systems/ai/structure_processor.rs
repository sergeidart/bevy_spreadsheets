// src/sheets/systems/ai/structure_processor.rs

use crate::sheets::definitions::default_ai_model_id;
use crate::sheets::events::{AiBatchResultKind, AiBatchTaskResult, StructureProcessingContext};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai::control_handler::BatchPayload;
use crate::ui::elements::ai_review::ai_context_utils::decorate_context_with_type;
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::systems::SendEvent;
use crate::SessionApiKey;
use bevy::prelude::*;
use bevy_tokio_tasks::TokioTasksRuntime;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use std::ffi::CString;

/// System that processes queued structure jobs and spawns AI tasks
pub fn process_structure_ai_jobs(
    mut state: ResMut<EditorWindowState>,
    registry: Res<SheetRegistry>,
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    session_api_key: Res<SessionApiKey>,
) {
    // PARALLEL MODE: Process ALL pending jobs simultaneously for faster testing
    // Pop jobs until queue is empty
    let Some(job) = state.ai_pending_structure_jobs.pop_front() else {
        return;
    };

    info!(
        "Processing structure AI job for {:?}/{} path {:?} with {} target rows: {:?}",
        job.root_category,
        job.root_sheet,
        job.structure_path,
        job.target_rows.len(),
        job.target_rows
    );

    // PARALLEL MODE: Don't set ai_active_structure_job to allow multiple jobs to run simultaneously
    // state.ai_active_structure_job = Some(job.clone());

    // Get the root sheet and metadata
    let Some(root_sheet) = registry.get_sheet(&job.root_category, &job.root_sheet) else {
        error!(
            "Structure AI job failed: root sheet {:?}/{} not found",
            job.root_category, job.root_sheet
        );
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    let Some(root_meta) = root_sheet.metadata.as_ref() else {
        error!(
            "Structure AI job failed: metadata missing for {:?}/{}",
            job.root_category, job.root_sheet
        );
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    // Get structure schema fields for the path
    let Some(structure_fields) = root_meta.structure_fields_for_path(&job.structure_path) else {
        error!(
            "Structure AI job failed: no schema fields for path {:?} in {:?}/{}",
            job.structure_path, job.root_category, job.root_sheet
        );
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    };

    // Build the structure headers - will be filtered later to match included_indices
    let all_structure_headers: Vec<String> = structure_fields.iter().map(|f| f.header.clone()).collect();
    
    // Build field path (headers) for nested structure navigation
    // This converts structure_path indices to field names by walking through the schema
    let nested_field_path: Vec<String> = if job.structure_path.len() > 1 {
        let mut field_path = Vec::new();
        
        // Start with the first column
        if let Some(&first_col_idx) = job.structure_path.first() {
            if let Some(first_col) = root_meta.columns.get(first_col_idx) {
                let mut current_schema = first_col.structure_schema.as_ref();
                
                // Navigate through each nested level
                for &nested_idx in job.structure_path.iter().skip(1) {
                    if let Some(schema) = current_schema {
                        if let Some(field) = schema.get(nested_idx) {
                            field_path.push(field.header.clone());
                            current_schema = field.structure_schema.as_ref();
                        }
                    }
                }
            }
        }
        
        field_path
    } else {
        Vec::new()
    };
    
    info!(
        "Nested field path for structure navigation: {:?}",
        nested_field_path
    );
    
    // Navigate through structure_path to get the key column index and ai_enable_row_generation
    // from the correct structure level (important for nested structures)
    // If not explicitly set, fall back to the sheet's ai_enable_row_generation setting
    let sheet_allow_add_rows = root_meta.ai_enable_row_generation;
    
    let (key_col_index, allow_row_additions) = if let Some(&first_path_idx) = job.structure_path.first() {
        if let Some(first_col) = root_meta.columns.get(first_path_idx) {
            if job.structure_path.len() == 1 {
                // Direct structure column - get from column definition, fall back to sheet setting
                let key_idx = first_col.structure_key_parent_column_index;
                let allow_add = first_col.ai_enable_row_generation.unwrap_or(sheet_allow_add_rows);
                (key_idx, allow_add)
            } else {
                // Nested structure - navigate through the path
                let mut current_schema = first_col.structure_schema.as_ref();
                let mut key_idx = first_col.structure_key_parent_column_index;
                let mut allow_add = first_col.ai_enable_row_generation.unwrap_or(sheet_allow_add_rows);
                
                for &nested_idx in job.structure_path.iter().skip(1) {
                    if let Some(schema) = current_schema {
                        if let Some(field) = schema.get(nested_idx) {
                            key_idx = field.structure_key_parent_column_index;
                            // Fall back to previous level's setting if not explicitly set at this level
                            allow_add = field.ai_enable_row_generation.unwrap_or(allow_add);
                            current_schema = field.structure_schema.as_ref();
                        }
                    }
                }
                (key_idx, allow_add)
            }
        } else {
            (None, sheet_allow_add_rows)
        }
    } else {
        (None, sheet_allow_add_rows)
    };
    
    info!(
        "Structure settings: key_col_index={:?}, allow_row_additions={}, structure_path={:?}",
        key_col_index, allow_row_additions, job.structure_path
    );
    
    // Verify by re-checking the specific field for the last element of the path
    if let Some(&first_idx) = job.structure_path.first() {
        if let Some(first_col) = root_meta.columns.get(first_idx) {
            if job.structure_path.len() == 1 {
                info!(
                    "Direct structure column {}: ai_enable_row_generation = {:?}",
                    first_idx, first_col.ai_enable_row_generation
                );
            } else {
                // Navigate to the final field
                let mut current_schema = first_col.structure_schema.as_ref();
                for (i, &nested_idx) in job.structure_path.iter().skip(1).enumerate() {
                    if let Some(schema) = current_schema {
                        if let Some(field) = schema.get(nested_idx) {
                            if i == job.structure_path.len() - 2 {
                                // This is the final nested field
                                info!(
                                    "Nested structure field at path {:?}: ai_enable_row_generation = {:?}",
                                    job.structure_path, field.ai_enable_row_generation
                                );
                            }
                            current_schema = field.structure_schema.as_ref();
                        }
                    }
                }
            }
        }
    }
    
    // Build column contexts and included indices: filter by ai_include_in_send to respect schema groups
    // Collect indices of non-structure fields that should be included
    let mut included_indices: Vec<usize> = Vec::new();
    let mut column_contexts: Vec<Option<String>> = Vec::new();
    
    for (idx, field) in structure_fields.iter().enumerate() {
        // Skip structure columns (nested structures)
        if matches!(field.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
            continue;
        }
        // Skip columns explicitly excluded by schema groups
        if matches!(field.ai_include_in_send, Some(false)) {
            continue;
        }
        included_indices.push(idx);
        column_contexts.push(decorate_context_with_type(field.ai_context.as_ref(), field.data_type));
    }
    
    // Store key header for later use
    let key_header = if let Some(key_idx) = key_col_index {
        if let Some(key_col) = root_meta.columns.get(key_idx) {
            Some(key_col.header.clone())
        } else {
            None
        }
    } else {
        None
    };

    // Build grouped parent-child data for structure requests
    let mut parent_groups: Vec<crate::sheets::systems::ai::control_handler::ParentGroup> = Vec::new();
    let mut row_partitions: Vec<usize> = Vec::new();
    
    // Helper to extract nested structure field by navigating through JSON using field headers
    // Returns the JSON string representing the nested structure data
    fn extract_nested_structure_json(
        cell_json: &str,
        field_path: &[String],
    ) -> Option<String> {
        if field_path.is_empty() {
            return Some(cell_json.to_string());
        }
        
        let trimmed = cell_json.trim();
        if trimmed.is_empty() {
            return None;
        }
        
        let mut current_value = match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(v) => v,
            Err(_) => return None,
        };
        
        // Navigate through each level of the field path
        for (depth, field_name) in field_path.iter().enumerate() {
            let is_last = depth == field_path.len() - 1;
            
            match current_value {
                serde_json::Value::Array(arr) => {
                    // For arrays, we need to extract the field from each object in the array
                    // and reconstruct as an array
                    let mut extracted_values = Vec::new();
                    
                    for item in arr {
                        if let serde_json::Value::Object(map) = item {
                            if let Some(nested_value) = map.get(field_name) {
                                extracted_values.push(nested_value.clone());
                            }
                        }
                    }
                    
                    if extracted_values.is_empty() {
                        return None;
                    }
                    
                    if is_last {
                        // This is the target field - return all extracted values as array
                        return Some(serde_json::to_string(&extracted_values).unwrap_or_default());
                    } else {
                        // Continue navigating - if there are multiple values, we take the first one
                        // (this is a simplification; in practice, nested arrays are complex)
                        current_value = extracted_values.into_iter().next()?;
                    }
                }
                serde_json::Value::Object(map) => {
                    // For a single object, extract the field
                    current_value = map.get(field_name)?.clone();
                    
                    if is_last {
                        // This is the target field - return it
                        return Some(serde_json::to_string(&current_value).unwrap_or_default());
                    }
                }
                _ => return None,
            }
        }
        
        Some(serde_json::to_string(&current_value).unwrap_or_default())
    }
    
    // Helper to parse structure cell JSON into rows matching schema headers
    fn parse_structure_cell_to_rows(cell_str: &str, headers: &[String]) -> Vec<Vec<String>> {
        let trimmed = cell_str.trim();
        if trimmed.is_empty() {
            return vec![vec![String::new(); headers.len()]];
        }
        
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(serde_json::Value::Array(arr)) => {
                if arr.is_empty() {
                    return vec![vec![String::new(); headers.len()]];
                }
                
                // Check if it's array of objects or array of arrays
                if arr.iter().all(|v| v.is_object()) {
                    // Array of objects: [{"field1":"val",...}]
                    arr.into_iter()
                        .map(|obj| {
                            if let serde_json::Value::Object(map) = obj {
                                headers
                                    .iter()
                                    .map(|h| map.get(h).and_then(|v| v.as_str()).unwrap_or("").to_string())
                                    .collect()
                            } else {
                                vec![String::new(); headers.len()]
                            }
                        })
                        .collect()
                } else if arr.iter().all(|v| v.is_string()) {
                    // Array of strings: ["val1","val2"]
                    vec![arr
                        .into_iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()]
                } else if arr.iter().all(|v| v.is_array()) {
                    // Array of arrays: [["val1","val2"]]
                    arr.into_iter()
                        .filter_map(|inner| {
                            if let serde_json::Value::Array(inner_arr) = inner {
                                Some(
                                    inner_arr
                                        .into_iter()
                                        .map(|v| v.as_str().unwrap_or("").to_string())
                                        .collect(),
                                )
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    vec![vec![String::new(); headers.len()]]
                }
            }
            Ok(serde_json::Value::Object(map)) => {
                // Single object: {"field1":"val",...}
                vec![headers
                    .iter()
                    .map(|h| map.get(h).and_then(|v| v.as_str()).unwrap_or("").to_string())
                    .collect()]
            }
            _ => vec![vec![String::new(); headers.len()]],
        }
    }
    
    for &target_row in &job.target_rows {
        // Check if this is a regular row index or a new row context token
        if let Some(context) = state.ai_structure_new_row_contexts.get(&target_row) {
            // This is a new row context - check if there's existing undecided structure data
            
            // Extract key column value from the new row's data
            let key_value = if key_col_index.is_some() {
                // Find the new row review for this context
                if let Some(new_row_review) = state.ai_new_row_reviews.get(context.new_row_index) {
                    // The key should be in the first element of the non_structure_columns
                    if let Some(&first_col_idx) = new_row_review.non_structure_columns.first() {
                        if first_col_idx == key_col_index.unwrap() {
                            // First non-structure column is the key column
                            new_row_review.ai.first().cloned().unwrap_or_default()
                        } else {
                            // Find the key column in the non_structure_columns
                            new_row_review.non_structure_columns
                                .iter()
                                .position(|&col| col == key_col_index.unwrap())
                                .and_then(|pos| new_row_review.ai.get(pos).cloned())
                                .unwrap_or_default()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            
            info!(
                "New row context {}: extracted key_value='{}'",
                target_row, key_value
            );
            
            // Build parent key info for this new row context
            let parent_key = crate::sheets::systems::ai::control_handler::ParentKeyInfo {
                context: if key_header.is_some() && key_col_index.is_some() {
                    root_meta.columns.get(key_col_index.unwrap())
                        .and_then(|col| col.ai_context.clone())
                } else {
                    None
                },
                key: key_value,
            };

            // First check if this new row is a duplicate of an existing row
            // If yes, we should extract structure data from the matched existing row, not from AI data
            let duplicate_match_row = state.ai_new_row_reviews
                .get(context.new_row_index)
                .and_then(|nr| nr.duplicate_match_row);
            
            // Check if there's an existing structure review entry for this new row
            let existing_structure_rows = state.ai_structure_reviews.iter()
                .find(|sr| {
                    sr.parent_new_row_index == Some(context.new_row_index)
                    && sr.structure_path == job.structure_path
                    && !sr.decided
                })
                .map(|sr| {
                    // Use merged_rows if they have content (user decisions applied), otherwise ai_rows
                    // This ensures we send the latest categorized data, not the original AI suggestions
                    let source_rows = if !sr.merged_rows.is_empty() && sr.merged_rows.len() >= sr.ai_rows.len() {
                        &sr.merged_rows
                    } else {
                        &sr.ai_rows
                    };
                    
                    source_rows.iter()
                        .map(|row| {
                            included_indices.iter()
                                .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                                .collect::<Vec<String>>()
                        })
                        .collect::<Vec<Vec<String>>>()
                });

            let group_rows = if let Some(existing_rows) = existing_structure_rows {
                info!(
                    "New row context {}: using {} existing undecided structure rows",
                    target_row, existing_rows.len()
                );
                row_partitions.push(existing_rows.len());
                existing_rows
            } else if let Some(matched_row_idx) = duplicate_match_row {
                // This new row is a duplicate of an existing row
                // Extract structure data from the matched existing row's grid cell
                info!(
                    "New row context {}: detected duplicate of existing row {}, extracting structure data from matched row",
                    target_row, matched_row_idx
                );
                
                let structure_col_idx = job.structure_path[0];
                if let Some(existing_grid_row) = root_sheet.grid.get(matched_row_idx) {
                    if let Some(structure_cell_json) = existing_grid_row.get(structure_col_idx) {
                        // Parse the structure JSON from the existing row
                        let target_json = if job.structure_path.len() > 1 {
                            extract_nested_structure_json(structure_cell_json, &nested_field_path)
                        } else {
                            Some(structure_cell_json.clone())
                        };
                        
                        if let Some(json_str) = target_json {
                            // Use the same parsing logic as for full_ai_row
                            let parsed_rows = crate::sheets::systems::ai::utils::parse_structure_rows_from_cell(
                                &json_str,
                                &structure_fields
                            );
                            
                            if !parsed_rows.is_empty() {
                                info!(
                                    "New row context {}: extracted {} structure rows from matched existing row {}",
                                    target_row, parsed_rows.len(), matched_row_idx
                                );
                                
                                // Filter to included columns
                                let filtered_rows: Vec<Vec<String>> = parsed_rows.iter()
                                    .map(|row| {
                                        included_indices.iter()
                                            .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                                            .collect()
                                    })
                                    .collect();
                                
                                row_partitions.push(filtered_rows.len());
                                filtered_rows
                            } else {
                                info!(
                                    "New row context {}: matched row {} has no structure data, using empty row",
                                    target_row, matched_row_idx
                                );
                                let row = vec![String::new(); included_indices.len()];
                                row_partitions.push(1);
                                vec![row]
                            }
                        } else {
                            info!(
                                "New row context {}: could not extract nested structure from matched row {}, using empty row",
                                target_row, matched_row_idx
                            );
                            let row = vec![String::new(); included_indices.len()];
                            row_partitions.push(1);
                            vec![row]
                        }
                    } else {
                        info!(
                            "New row context {}: matched row {} has no structure column {}, using empty row",
                            target_row, matched_row_idx, structure_col_idx
                        );
                        let row = vec![String::new(); included_indices.len()];
                        row_partitions.push(1);
                        vec![row]
                    }
                } else {
                    warn!(
                        "New row context {}: matched row {} not found in grid, using empty row",
                        target_row, matched_row_idx
                    );
                    let row = vec![String::new(); included_indices.len()];
                    row_partitions.push(1);
                    vec![row]
                }
            } else if let Some(full_row) = &context.full_ai_row {
                // Try to extract structure data from the full AI row
                let structure_col_idx = job.structure_path[0];
                
                if let Some(structure_cell_json) = full_row.get(structure_col_idx) {
                    // Extract nested structure if needed (for nested paths)
                    let target_json = if job.structure_path.len() > 1 {
                        extract_nested_structure_json(structure_cell_json, &nested_field_path)
                    } else {
                        Some(structure_cell_json.clone())
                    };
                    
                    if let Some(json_str) = target_json {
                        // Parse JSON to extract rows
                        match serde_json::from_str::<serde_json::Value>(&json_str) {
                            Ok(serde_json::Value::Array(arr)) => {
                                let parsed_rows: Vec<Vec<String>> = arr.iter()
                                    .filter_map(|item| {
                                        if let serde_json::Value::Array(row_arr) = item {
                                            let row: Vec<String> = row_arr.iter()
                                                .map(|val| match val {
                                                    serde_json::Value::String(s) => s.clone(),
                                                    serde_json::Value::Number(n) => n.to_string(),
                                                    serde_json::Value::Bool(b) => b.to_string(),
                                                    serde_json::Value::Null => String::new(),
                                                    _ => serde_json::to_string(val).unwrap_or_default(),
                                                })
                                                .collect();
                                            Some(row)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                
                                if !parsed_rows.is_empty() {
                                    info!(
                                        "New row context {}: extracted {} structure rows from AI response",
                                        target_row, parsed_rows.len()
                                    );
                                    row_partitions.push(parsed_rows.len());
                                    parsed_rows
                                } else {
                                    info!(
                                        "New row context {}: parsed JSON but no valid rows, using empty row",
                                        target_row
                                    );
                                    let row = vec![String::new(); included_indices.len()];
                                    row_partitions.push(1);
                                    vec![row]
                                }
                            }
                            _ => {
                                info!(
                                    "New row context {}: structure JSON is not an array, using empty row",
                                    target_row
                                );
                                let row = vec![String::new(); included_indices.len()];
                                row_partitions.push(1);
                                vec![row]
                            }
                        }
                    } else {
                        info!(
                            "New row context {}: could not extract nested structure JSON, using empty row",
                            target_row
                        );
                        let row = vec![String::new(); included_indices.len()];
                        row_partitions.push(1);
                        vec![row]
                    }
                } else {
                    info!(
                        "New row context {}: structure column {} not found in full_ai_row, using empty row",
                        target_row, structure_col_idx
                    );
                    let row = vec![String::new(); included_indices.len()];
                    row_partitions.push(1);
                    vec![row]
                }
            } else {
                // No existing structure data and no full_ai_row - create single empty row
                info!(
                    "New row context {}: no existing data and no full_ai_row, creating empty structure row",
                    target_row
                );
                let row = vec![String::new(); included_indices.len()];
                row_partitions.push(1);
                vec![row]
            };
            
            // Create parent group
            parent_groups.push(crate::sheets::systems::ai::control_handler::ParentGroup {
                parent_key,
                rows: group_rows,
            });
        } else if let Some(root_row) = root_sheet.grid.get(target_row) {
            // This is an existing row - extract structure data
            info!(
                "Processing target_row {}: row has {} cells, first 3 cells: {:?}",
                target_row,
                root_row.len(),
                root_row.iter().take(3).collect::<Vec<_>>()
            );
            
            // Get the key column value for this row
            let key_value = if let Some(key_idx) = key_col_index {
                let val = root_row.get(key_idx).cloned().unwrap_or_default();
                info!(
                    "Extracting from row {}: key_col_index={}, key_value='{}', row has {} cells",
                    target_row, key_idx, val, root_row.len()
                );
                if val.is_empty() {
                    warn!(
                        "Row {} has empty key value at index {} (row length: {})",
                        target_row, key_idx, root_row.len()
                    );
                }
                val
            } else {
                info!("Extracting from row {}: no key column", target_row);
                String::new()
            };
            
            // Navigate through the structure path to get the correct structure cell data
            let structure_cell_data = if let Some(&first_col_idx) = job.structure_path.first() {
                if let Some(root_cell) = root_row.get(first_col_idx) {
                    if nested_field_path.is_empty() {
                        // Direct structure column (no nesting)
                        Some(root_cell.clone())
                    } else {
                        // Nested structure - extract the nested field data using recursive navigation
                        extract_nested_structure_json(root_cell, &nested_field_path)
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            // Build parent key info for this target row
            let parent_key = crate::sheets::systems::ai::control_handler::ParentKeyInfo {
                context: if key_header.is_some() && key_col_index.is_some() {
                    root_meta.columns.get(key_col_index.unwrap())
                        .and_then(|col| col.ai_context.clone())
                } else {
                    None
                },
                key: key_value.clone(),
            };
            
            // Parse structure cell data to get rows for this parent
            let group_rows = if let Some(structure_cell) = structure_cell_data {
                info!(
                    "Row {}: extracted structure cell data (first 100 chars): {}",
                    target_row,
                    &structure_cell.chars().take(100).collect::<String>()
                );
                // Parse with all headers first
                let all_rows = parse_structure_cell_to_rows(&structure_cell, &all_structure_headers);
                info!(
                    "Row {}: parsed into {} structure rows",
                    target_row, all_rows.len()
                );
                
                // Filter each row to only include columns that match included_indices
                let filtered_rows: Vec<Vec<String>> = all_rows.into_iter()
                    .map(|row| {
                        included_indices.iter()
                            .map(|&idx| row.get(idx).cloned().unwrap_or_default())
                            .collect()
                    })
                    .collect();
                
                row_partitions.push(filtered_rows.len());
                filtered_rows
            } else {
                // No structure data - create single empty row with only included columns
                let row = vec![String::new(); included_indices.len()];
                row_partitions.push(1);
                vec![row]
            };
            
            // Create parent group with its rows
            parent_groups.push(crate::sheets::systems::ai::control_handler::ParentGroup {
                parent_key,
                rows: group_rows,
            });
        }
    }

    if parent_groups.is_empty() {
        warn!(
            "Structure AI job for {:?}/{} path {:?} has no valid parent groups",
            job.root_category, job.root_sheet, job.structure_path
        );
        // PARALLEL MODE: No need to clear ai_active_structure_job
        state.mark_structure_result_received();
        return;
    }

    let total_rows: usize = parent_groups.iter().map(|g| g.rows.len()).sum();
    info!(
        "Built {} parent groups with {} total rows for structure batch",
        parent_groups.len(),
        total_rows
    );

    // Build payload using the same BatchPayload struct as regular requests
    let structure_label = job.label.as_deref().unwrap_or("structure");
    let user_prompt = format!(
        "Fill in the missing structure data for '{}'. Provide complete and accurate information for each field based on the context.",
        structure_label
    );

    let payload = BatchPayload {
        ai_model_id: default_ai_model_id(),
        general_sheet_rule: root_meta.ai_general_rule.clone(),
        column_contexts,
        rows_data: Vec::new(), // Empty for structure requests - use parent_groups instead
        requested_grounding_with_google_search: root_meta.requested_grounding_with_google_search.unwrap_or(false),
        allow_row_additions,
        // For structures, don't use key prefix (legacy approach)
        key_prefix_count: None,
        key_prefix_headers: None,
        // Use grouped parent-child structure for clarity
        parent_groups: Some(parent_groups),
        user_prompt,
    };

    let payload_json = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialize structure payload: {}", e);
            // PARALLEL MODE: No need to clear ai_active_structure_job
            state.mark_structure_result_received();
            return;
        }
    };
    
    // Log the structure request
    if let Ok(pretty_payload) = serde_json::to_string_pretty(&payload) {
        let status = format!("Sending structure request for {} row(s)...", job.target_rows.len());
        state.add_ai_call_log(status, None, Some(pretty_payload), false);
    }

    // Clone data for the async task
    let api_key_for_task = session_api_key.0.clone();
    // For structure batches, included_cols should be the actual indices of included fields
    // These map to the filtered structure_headers and column_contexts we built above
    let included_cols_clone: Vec<usize> = included_indices.clone();
    let job_clone = job.clone();
    let row_partitions_clone = row_partitions;

    let commands_entity = commands.spawn_empty().id();
    runtime.spawn_background_task(move |mut ctx| async move {
        let api_key_value = match api_key_for_task {
            Some(k) if !k.is_empty() => k,
            _ => {
                let err_msg = "API Key not set".to_string();
                ctx.run_on_main_thread(move |world_ctx| {
                    world_ctx
                        .world
                        .commands()
                        .entity(commands_entity)
                        .insert(SendEvent::<AiBatchTaskResult> {
                            event: AiBatchTaskResult {
                                original_row_indices: job_clone.target_rows.clone(),
                                result: Err(err_msg),
                                raw_response: None,
                                included_non_structure_columns: included_cols_clone,
                                key_prefix_count: 0,
                                prompt_only: false,
                                kind: AiBatchResultKind::Root {
                                    structure_context: Some(StructureProcessingContext {
                                        root_category: job_clone.root_category.clone(),
                                        root_sheet: job_clone.root_sheet.clone(),
                                        structure_path: job_clone.structure_path.clone(),
                                        target_rows: job_clone.target_rows.clone(),
                                        original_row_partitions: row_partitions_clone.clone(),
                                        row_partitions: row_partitions_clone,
                                        generation_id: job_clone.generation_id,
                                    }),
                                },
                            },
                        });
                })
                .await;
                return;
            }
        };

        let (result, raw_response, updated_partitions) = tokio::task::spawn_blocking(move || {
            Python::with_gil(
                |py| -> PyResult<(Result<Vec<Vec<String>>, String>, Option<String>, Option<Vec<usize>>)> {
                    let python_file_path = "script/ai_processor.py";
                    let processor_code_string = std::fs::read_to_string(python_file_path)?;
                    let code_c_str = CString::new(processor_code_string)
                        .map_err(|e| PyValueError::new_err(format!("CString error: {}", e)))?;
                    let file_name_c_str = CString::new(python_file_path)
                        .map_err(|e| PyValueError::new_err(format!("File name CString error: {}", e)))?;
                    let module_name_c_str = CString::new("ai_processor")
                        .map_err(|e| PyValueError::new_err(format!("Module name CString error: {}", e)))?;
                    let module = PyModule::from_code(
                        py,
                        code_c_str.as_c_str(),
                        file_name_c_str.as_c_str(),
                        module_name_c_str.as_c_str(),
                    )?;
                    let binding = module.call_method1("execute_ai_query", (api_key_value, payload_json))?;
                    let result_str: &str = binding.downcast::<PyString>()?.to_str()?;
                    let response_text = result_str;

                    match serde_json::from_str::<serde_json::Value>(response_text) {
                        Ok(parsed) => {
                            if let Some(success) = parsed.get("success").and_then(|v| v.as_bool()) {
                                if success {
                                    if let Some(data) = parsed.get("data").and_then(|v| v.as_array()) {
                                        let mut out: Vec<Vec<String>> = Vec::new();
                                        
                                        // Check if this is a grouped response (3D: array of groups)
                                        // vs flat response (2D: array of rows)
                                        let is_grouped = !data.is_empty() 
                                            && data[0].is_array()
                                            && data[0].as_array().map_or(false, |arr| 
                                                !arr.is_empty() && arr[0].is_array()
                                            );
                                        
                                        let mut new_partitions: Option<Vec<usize>> = None;
                                        
                                        if is_grouped {
                                            // Grouped response: flatten groups into single array
                                            // Track actual group sizes for row_partitions update
                                            info!("Detected grouped response with {} groups", data.len());
                                            let mut partitions = Vec::new();
                                            
                                            for (group_idx, group_val) in data.iter().enumerate() {
                                                if let Some(group_array) = group_val.as_array() {
                                                    let group_size = group_array.len();
                                                    info!("Processing group {}: {} rows", group_idx, group_size);
                                                    partitions.push(group_size);
                                                    
                                                    for row_val in group_array {
                                                        if let Some(row_array) = row_val.as_array() {
                                                            let row: Vec<String> = row_array
                                                                .iter()
                                                                .map(|cell| {
                                                                    cell.as_str().unwrap_or("").to_string()
                                                                })
                                                                .collect();
                                                            out.push(row);
                                                        } else {
                                                            return Ok((
                                                                Err(format!("Group {} row not an array: {}", group_idx, row_val)),
                                                                parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                                                None,
                                                            ));
                                                        }
                                                    }
                                                } else {
                                                    return Ok((
                                                        Err(format!("Group {} not an array: {}", group_idx, group_val)),
                                                        parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                                        None,
                                                    ));
                                                }
                                            }
                                            info!("Flattened {} groups into {} total rows. Partitions: {:?}", data.len(), out.len(), partitions);
                                            new_partitions = Some(partitions);
                                        } else {
                                            // Flat response: parse normally
                                            info!("Detected flat response with {} rows", data.len());
                                            for row_val in data {
                                                if let Some(row_array) = row_val.as_array() {
                                                    let row: Vec<String> = row_array
                                                        .iter()
                                                        .map(|cell| {
                                                            cell.as_str().unwrap_or("").to_string()
                                                        })
                                                        .collect();
                                                    out.push(row);
                                                } else {
                                                    return Ok((
                                                        Err(format!("Row not an array: {}", row_val)),
                                                        parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                                        None,
                                                    ));
                                                }
                                            }
                                        }
                                        Ok((Ok(out), parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()), new_partitions))
                                    } else {
                                        Ok((
                                            Err("Expected data array".to_string()),
                                            parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                            None,
                                        ))
                                    }
                                } else {
                                    Ok((
                                        Err(parsed
                                            .get("error")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Unknown structure error")
                                            .to_string()),
                                        parsed.get("raw_response").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                        None,
                                    ))
                                }
                            } else {
                                Ok((
                                    Err("Invalid response format".to_string()),
                                    Some(response_text.to_string()),
                                    None,
                                ))
                            }
                        }
                        Err(e) => Ok((
                            Err(format!("JSON parse error: {}", e)),
                            Some(response_text.to_string()),
                            None,
                        )),
                    }
                },
            )
        })
        .await
        .unwrap_or_else(|e| Ok((Err(format!("Tokio panic: {}", e)), None, None)))
        .unwrap_or_else(|e| (Err(format!("PyO3 error: {}", e)), Some(e.to_string()), None));

        // Use updated partitions if provided (for grouped responses with AI-added rows)
        let final_partitions = updated_partitions.unwrap_or_else(|| row_partitions_clone.clone());
        // Keep original partitions for identifying which rows are original vs AI-added
        let original_partitions = row_partitions_clone;

        ctx.run_on_main_thread(move |world_ctx| {
            world_ctx
                .world
                .commands()
                .entity(commands_entity)
                .insert(SendEvent::<AiBatchTaskResult> {
                    event: AiBatchTaskResult {
                        original_row_indices: job_clone.target_rows.clone(),
                        result,
                        raw_response,
                        included_non_structure_columns: included_cols_clone,
                        key_prefix_count: 0,
                        prompt_only: false,
                        kind: AiBatchResultKind::Root {
                            structure_context: Some(StructureProcessingContext {
                                root_category: job_clone.root_category.clone(),
                                root_sheet: job_clone.root_sheet.clone(),
                                structure_path: job_clone.structure_path.clone(),
                                target_rows: job_clone.target_rows.clone(),
                                original_row_partitions: original_partitions,
                                row_partitions: final_partitions,
                                generation_id: job_clone.generation_id,
                            }),
                        },
                    },
                });
        })
        .await;
    });
}