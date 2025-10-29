// src/ui/widgets/technical_column_widget.rs

use bevy::prelude::*;
use bevy_egui::egui;
use crate::sheets::{
    resources::SheetRegistry,
    systems::logic::lineage_helpers::walk_parent_lineage,
};
use crate::ui::elements::editor::state::EditorWindowState;

/// Checks if a column is a technical column (row_index, parent_key, temp_new_row_index, etc.)
/// that should be displayed as green read-only when structure technical columns are hidden.
///
/// # Arguments
/// * `registry` - The sheet registry
/// * `category` - Optional category for the sheet
/// * `sheet_name` - Name of the sheet
/// * `col_index` - Column index to check
/// * `state` - Editor window state to check hide settings
///
/// # Returns
/// `true` if this is a technical column and should be rendered as read-only green text
pub fn is_technical_column(
    registry: &SheetRegistry,
    category: &Option<String>,
    sheet_name: &str,
    col_index: usize,
    state: &EditorWindowState,
) -> bool {
    if !state.should_hide_structure_technical_columns(category, sheet_name) {
        return false;
    }

    registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index))
        .map(|col_def| {
            col_def.header.eq_ignore_ascii_case("row_index")
                || col_def.header.eq_ignore_ascii_case("parent_key")
                || col_def.header.eq_ignore_ascii_case("temp_new_row_index")
                || col_def.header.eq_ignore_ascii_case("_obsolete_temp_new_row_index")
        })
        .unwrap_or(false)
}

/// Renders a technical column as green read-only text with special handling for parent_key lineage.
///
/// For parent_key columns, displays the lineage breadcrumb with › separator and adds a tooltip
/// showing the full table names and row indices.
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `col_index` - Column index being rendered
/// * `current_display_text` - The current cell value
/// * `registry` - The sheet registry
/// * `state` - Mutable editor window state (for caching lineage)
/// * `category` - Optional category for the sheet
/// * `sheet_name` - Name of the sheet
///
/// # Returns
/// `true` if the column was rendered as a technical column, `false` otherwise
pub fn render_technical_column(
    ui: &mut egui::Ui,
    col_index: usize,
    current_display_text: &str,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
) -> bool {
    if !is_technical_column(registry, category, sheet_name, col_index, state) {
        return false;
    }

    // Check if this is specifically a parent_key column
    let is_parent_key = registry
        .get_sheet(category, sheet_name)
        .and_then(|sd| sd.metadata.as_ref())
        .and_then(|meta| meta.columns.get(col_index))
        .map(|col_def| col_def.header.eq_ignore_ascii_case("parent_key"))
        .unwrap_or(false);

    // Build the display text
    let display = if is_parent_key && !current_display_text.is_empty() {
        build_parent_key_display(current_display_text, registry, state, category, sheet_name)
    } else if current_display_text.is_empty() {
        "(empty)".to_string()
    } else {
        current_display_text.to_string()
    };

    // Render the green label
    let desired_size = egui::vec2(ui.available_width(), ui.style().spacing.interact_size.y);
    let (_id, rect) = ui.allocate_space(desired_size);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |inner| {
        inner.allocate_ui_with_layout(
            rect.size(),
            egui::Layout::left_to_right(egui::Align::Center),
            |row_ui| {
                row_ui.vertical_centered(|vc| {
                    let label = vc.label(
                        egui::RichText::new(&display).color(egui::Color32::from_rgb(0, 180, 0)),
                    );

                    // Add tooltip for parent_key showing table names
                    if is_parent_key && !current_display_text.is_empty() {
                        add_parent_key_tooltip(
                            label,
                            current_display_text,
                            registry,
                            state,
                            category,
                            sheet_name,
                        );
                    }
                });
            },
        );
    });

    true
}

/// Builds the display text for a parent_key column with lineage breadcrumb.
fn build_parent_key_display(
    current_display_text: &str,
    registry: &SheetRegistry,
    state: &mut EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
) -> String {
    // When in structure navigation context, use ancestor_keys from navigation state
    if let Some(nav_ctx) = state.structure_navigation_stack.last() {
        if !nav_ctx.ancestor_keys.is_empty() {
            return nav_ctx.ancestor_keys.join(" › ");
        } else {
            return current_display_text.to_string();
        }
    }

    // Not in structure navigation - walk the parent chain
    if let Ok(parent_row_idx) = current_display_text.parse::<usize>() {
        // Try to get parent table info from metadata
        if let Some(parent_link) = registry
            .get_sheet(category, sheet_name)
            .and_then(|sd| sd.metadata.as_ref())
            .and_then(|meta| meta.structure_parent.as_ref())
        {
            let parent_category = parent_link.parent_category.clone();
            let parent_sheet = parent_link.parent_sheet.clone();

            // Check cache first
            let cache_key = (parent_category.clone(), parent_sheet.clone(), parent_row_idx);
            let lineage = if let Some(cached) = state.parent_lineage_cache.get(&cache_key) {
                cached.clone()
            } else {
                // Build lineage and cache it
                let lineage = walk_parent_lineage(
                    registry,
                    &parent_category,
                    &parent_sheet,
                    parent_row_idx,
                );
                state.parent_lineage_cache.insert(cache_key, lineage.clone());
                lineage
            };

            // Format lineage with › separator
            if !lineage.is_empty() {
                return lineage
                    .iter()
                    .map(|(_, display_val, _)| display_val.as_str())
                    .collect::<Vec<_>>()
                    .join(" › ");
            }
        }
    }

    current_display_text.to_string()
}

/// Adds a tooltip to a parent_key label showing the full table names and row indices.
fn add_parent_key_tooltip(
    label: egui::Response,
    current_display_text: &str,
    registry: &SheetRegistry,
    state: &EditorWindowState,
    category: &Option<String>,
    sheet_name: &str,
) {
    if let Ok(parent_row_idx) = current_display_text.parse::<usize>() {
        if let Some(parent_link) = registry
            .get_sheet(category, sheet_name)
            .and_then(|sd| sd.metadata.as_ref())
            .and_then(|meta| meta.structure_parent.as_ref())
        {
            let cache_key = (
                parent_link.parent_category.clone(),
                parent_link.parent_sheet.clone(),
                parent_row_idx,
            );
            if let Some(lineage) = state.parent_lineage_cache.get(&cache_key) {
                if !lineage.is_empty() {
                    let tooltip_text = lineage
                        .iter()
                        .map(|(table, display, idx)| format!("{} ({}[{}])", display, table, idx))
                        .collect::<Vec<_>>()
                        .join(" › ");
                    label.on_hover_text(tooltip_text);
                }
            }
        }
    }
}
