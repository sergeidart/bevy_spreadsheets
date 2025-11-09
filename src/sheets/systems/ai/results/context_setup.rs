// src/sheets/systems/ai/results/context_setup.rs
// Setup AI context prefixes for key columns

use crate::sheets::events::AiBatchTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Setup AI context prefixes for key columns
/// No longer uses virtual structure navigation (deprecated)
pub fn setup_context_prefixes(
    state: &mut EditorWindowState,
    _registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
) {
    state.ai_context_only_prefix_count = ev.key_prefix_count;
    // Virtual structure system removed - prefixes stored at send time remain available for rendering
}

