// src/ui/widgets/structure_column_widget.rs

use bevy::prelude::*;
use bevy_egui::egui;
use crate::sheets::{
    events::OpenStructureViewEvent,
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::{EditorWindowState, StructureNavigationContext};

/// Renders a structure column as a button that opens the child structure sheet.
///
/// This widget handles:
/// - Button rendering with appropriate text (column header or cell value)
/// - Row count caching for performance
/// - Tooltip with structure information
/// - Navigation stack management when clicked
/// - Event emission for structure creation if needed
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `col_index` - Column index being rendered
/// * `row_index` - Row index being rendered
/// * `current_display_text` - The current cell value
/// * `registry` - The sheet registry
/// * `state` - Mutable editor window state
/// * `category` - Optional category for the sheet
/// * `sheet_name` - Name of the current sheet
/// * `structure_open_events` - Event writer for opening structure views
///
/// # Returns
/// `(Option<Response>, Option<String>)` - The response and any new value (always None for structure columns)
pub fn render_structure_column(
    ui: &mut egui::Ui,
    col_index: usize,
    row_index: usize,
    current_display_text: &str,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    structure_open_events: &mut EventWriter<OpenStructureViewEvent>,
) -> (Option<egui::Response>, Option<String>) {
    let column_def = registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index));

    if let Some(col_def) = column_def {
        let structure_sheet_name = format!("{}_{}", sheet_name, col_def.header);

        // Get parent's row_index to use for filtering children
        let parent_row_index = get_parent_row_index(registry, category, sheet_name, row_index);

        // Get human-readable display key
        let parent_display_key = get_parent_display_key(registry, category, sheet_name, row_index);

        // Get UI header (display_header or fallback to header)
        let ui_header = col_def
            .display_header
            .as_ref()
            .cloned()
            .unwrap_or_else(|| col_def.header.clone());

        // Button text: use header if cell is empty, otherwise use cell value
        let button_text = if current_display_text.trim().is_empty() {
            ui_header.clone()
        } else {
            current_display_text.to_string()
        };

        // Render button
        let button = egui::Button::new(button_text);
        let mut resp = ui.add_sized(ui.available_size(), button);

        // Get or calculate row count
        let rows_count = get_structure_row_count(
            registry,
            state,
            category,
            &structure_sheet_name,
            &parent_row_index,
            row_index,
            col_index,
        );

        // Add hover tooltip
        resp = resp.on_hover_text(format!(
            "Structure: {}\nParent: {} (row_index: {})\nRows: {}\nPreview: {}\n\nClick to open",
            structure_sheet_name, parent_display_key, parent_row_index, rows_count, current_display_text
        ));

        // Handle click
        if resp.clicked() {
            handle_structure_click(
                registry,
                state,
                category,
                sheet_name,
                &structure_sheet_name,
                row_index,
                col_index,
                current_display_text,
                &ui_header,
                structure_open_events,
            );
        }

        (Some(resp), None)
    } else {
        // Fallback if column definition not found
        let resp = ui.label("?");
        (Some(resp), None)
    }
}

/// Gets the parent row's row_index value (always at column 0).
fn get_parent_row_index(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
) -> String {
    registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.grid.get(row_index))
        .and_then(|row| row.get(0)) // row_index is always at column 0
        .map(|s| s.clone())
        .unwrap_or_else(|| row_index.to_string())
}

/// Gets the human-readable display key for the parent row (first non-technical column).
fn get_parent_display_key(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
) -> String {
    registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| {
            let row = sd.grid.get(row_index)?;
            let key_idx_dyn = sd.metadata.as_ref().and_then(|meta| {
                meta.columns.iter().position(|c| {
                    let h = c.header.to_ascii_lowercase();
                    h != "row_index"
                        && h != "parent_key"
                        && h != "temp_new_row_index"
                        && h != "_obsolete_temp_new_row_index"
                })
            }).or(Some(0));
            key_idx_dyn.and_then(|idx| row.get(idx)).cloned()
        })
        .unwrap_or_else(|| row_index.to_string())
}

/// Gets the structure row count with caching for performance.
fn get_structure_row_count(
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    category: &Option<String>,
    structure_sheet_name: &str,
    parent_row_index: &str,
    row_index: usize,
    col_index: usize,
) -> usize {
    let cache_key = (
        category.clone(),
        structure_sheet_name.to_string(),
        row_index,
        col_index,
        1usize,
    );

    let mut count_opt = state.ui_structure_row_count_cache.get(&cache_key).copied();
    if count_opt.is_none() {
        if let Some(struct_sheet) = registry.get_sheet(category, structure_sheet_name) {
            // Filter by parent_row_index (numeric comparison)
            // Children's parent_key column (index 1) stores the parent's row_index
            let c = struct_sheet
                .grid
                .iter()
                .filter(|r| r.get(1).map(|v| v == parent_row_index).unwrap_or(false))
                .count();
            state.ui_structure_row_count_cache.insert(cache_key.clone(), c);
            count_opt = Some(c);
        }
    }

    count_opt.unwrap_or(0)
}

/// Handles clicking on a structure button to navigate into the child structure.
fn handle_structure_click(
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    structure_sheet_name: &str,
    row_index: usize,
    col_index: usize,
    current_display_text: &str,
    ui_header: &str,
    structure_open_events: &mut EventWriter<OpenStructureViewEvent>,
) {
    if registry.get_sheet(category, structure_sheet_name).is_some() {
        // Structure sheet exists - navigate to it
        build_and_push_navigation_context(
            registry,
            state,
            category,
            sheet_name,
            structure_sheet_name,
            row_index,
            current_display_text,
            ui_header,
        );
    } else {
        // Structure sheet doesn't exist - emit event to create it
        structure_open_events.write(OpenStructureViewEvent {
            parent_category: category.clone(),
            parent_sheet: sheet_name.to_string(),
            row_index,
            col_index,
        });
    }
}

/// Builds navigation context and pushes it to the navigation stack.
fn build_and_push_navigation_context(
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
    structure_sheet_name: &str,
    row_index: usize,
    current_display_text: &str,
    ui_header: &str,
) {
    // Build ancestor_keys (display values) and ancestor_row_indices (numeric) from navigation stack
    let mut ancestor_keys = if let Some(current_nav) = state.structure_navigation_stack.last() {
        current_nav.ancestor_keys.clone()
    } else {
        Vec::new()
    };

    let mut ancestor_row_indices = if let Some(current_nav) = state.structure_navigation_stack.last() {
        current_nav.ancestor_row_indices.clone()
    } else {
        Vec::new()
    };

    // Get parent row's row_index value to use as parent_key
    let parent_row_index_value = registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.grid.get(row_index))
        .and_then(|row| row.get(0)) // row_index is always at column 0
        .map(|s| s.clone())
        .unwrap_or_else(|| row_index.to_string());

    // Get the display value for the current row (for UI breadcrumb)
    let display_value = registry
        .get_sheet(category, sheet_name)
        .and_then(|sheet_data| {
            sheet_data.metadata.as_ref().and_then(|metadata| {
                sheet_data.grid.get(row_index).map(|row| {
                    crate::ui::elements::editor::structure_navigation::get_first_content_column_value(metadata, row)
                })
            })
        })
        .unwrap_or_else(|| current_display_text.to_string());

    // Add to lineage arrays
    ancestor_keys.push(display_value.clone());
    ancestor_row_indices.push(parent_row_index_value.clone());

    info!(
        "Opening structure: {} -> {} | parent_row_index='{}' (display: '{}') | ancestor_keys={:?} | ancestor_row_indices={:?}",
        sheet_name, structure_sheet_name, parent_row_index_value, display_value, ancestor_keys, ancestor_row_indices
    );

    let nav_context = StructureNavigationContext {
        structure_sheet_name: structure_sheet_name.to_string(),
        parent_category: category.clone(),
        parent_sheet_name: sheet_name.to_string(),
        parent_row_key: parent_row_index_value,
        ancestor_keys,
        ancestor_row_indices,
        parent_column_name: ui_header.to_string(),
    };

    state.structure_navigation_stack.push(nav_context);
    state.selected_category = category.clone();
    state.selected_sheet_name = Some(structure_sheet_name.to_string());
}
