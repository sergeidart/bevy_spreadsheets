// Moved linked column cache logic from UI into AI systems layer
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::validation::normalize_for_link_cmp;
use std::collections::HashSet;
use std::sync::Arc;

pub enum CacheResult {
    Success {
        raw: Arc<HashSet<String>>,
        normalized: Arc<HashSet<String>>,
    },
    Error(()),
}

pub fn get_or_populate_linked_options(
    target_sheet_name: &str,
    target_column_index: usize,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
) -> CacheResult {
    let cache_key = (target_sheet_name.to_string(), target_column_index);
    if !state.linked_column_cache.contains_key(&cache_key) {
        let mut error = false;
        let target_sheet_data_opt = registry
            .iter_sheets()
            .find(|(_, name, _)| *name == target_sheet_name)
            .map(|(_, _, data)| data);
        if let Some(target_sheet) = target_sheet_data_opt {
            if let Some(meta) = &target_sheet.metadata {
                if target_column_index < meta.columns.len() {
                    let unique_values: HashSet<String> = target_sheet
                        .grid
                        .iter()
                        .filter_map(|row| row.get(target_column_index))
                        .filter(|cell| !cell.is_empty())
                        .cloned()
                        .collect();
                    let normalized_values: HashSet<String> = unique_values
                        .iter()
                        .map(|v| normalize_for_link_cmp(v))
                        .collect();
                    let unique_values_arc = Arc::new(unique_values);
                    let normalized_values_arc = Arc::new(normalized_values);
                    state
                        .linked_column_cache
                        .insert(cache_key.clone(), Arc::clone(&unique_values_arc));
                    state
                        .linked_column_cache_normalized
                        .insert(cache_key.clone(), Arc::clone(&normalized_values_arc));
                } else {
                    error = true;
                }
            } else {
                error = true;
            }
        } else {
            error = true;
        }
        if error {
            state
                .linked_column_cache
                .entry(cache_key.clone())
                .or_insert_with(|| Arc::new(HashSet::new()));
            state
                .linked_column_cache_normalized
                .entry(cache_key.clone())
                .or_insert_with(|| Arc::new(HashSet::new()));
            return CacheResult::Error(());
        }
    }
    match (
        state.linked_column_cache.get(&cache_key),
        state.linked_column_cache_normalized.get(&cache_key),
    ) {
        (Some(raw), Some(norm)) => CacheResult::Success {
            raw: Arc::clone(raw),
            normalized: Arc::clone(norm),
        },
        _ => CacheResult::Error(()),
    }
}
