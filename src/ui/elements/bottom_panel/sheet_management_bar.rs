// src/ui/elements/bottom_panel/sheet_management_bar.rs
//! Sheet Management Bar - Main orchestrator for category and sheet selection UI
//! This module coordinates the rendering of the bottom panel which contains:
//! - Category selection row with dropdown, tabs, and controls
//! - Sheet selection row with dropdown, tabs, and controls

use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestMoveSheetToCategory;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::database::daemon_client::DaemonClient;
use crate::ui::elements::editor::state::EditorWindowState;

// Use sibling modules
use super::{category_row, sheet_row};

/// Event writers needed for sheet management operations
pub struct SheetManagementEventWriters<'a, 'w> {
    pub move_sheet_to_category: &'a mut EventWriter<'w, RequestMoveSheetToCategory>,
}

/// Main entry point: draws both category and sheet rows
pub fn show_sheet_management_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
    event_writers: &mut SheetManagementEventWriters<'a, 'w>,
    daemon_client: &DaemonClient,
) {
    ui.vertical(|ui_v| {
        category_row::show_category_picker(ui_v, state, registry, event_writers);
        ui_v.add_space(4.0);
        sheet_row::show_sheet_controls(ui_v, state, registry, event_writers, daemon_client);
    });
}
