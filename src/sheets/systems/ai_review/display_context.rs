use crate::sheets::definitions::StructureFieldDefinition;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Contains all the display context needed for rendering the AI batch review UI
pub struct ReviewDisplayContext {
    pub in_structure_mode: bool,
    pub active_sheet_name: String,
    pub active_category: Option<String>,
    pub merged_columns: Vec<ColumnEntry>,
    pub structure_schema: Vec<StructureFieldDefinition>,
    pub ancestor_key_columns: Vec<(String, String)>,
    pub show_pending_structures: bool,
}

/// Prepares all display context for the AI batch review panel
pub fn prepare_display_context(
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
) -> Option<ReviewDisplayContext> {
    use crate::sheets::systems::ai_review::review_logic::{
        build_merged_columns, build_union_columns, gather_ancestor_key_columns,
        has_undecided_structures, resolve_active_sheet_name,
    };

    // Detect which of the three AI review modes we're in
    let in_structure_detail_mode = state.ai_structure_detail_context.is_some();
    let in_navigation_drilldown = !state.ai_navigation_stack.is_empty();
    let in_virtual_structure_review = false; // Virtual structure system deprecated
    let in_structure_mode = in_structure_detail_mode || in_navigation_drilldown;

    // Resolve active sheet name (considers navigation drill-down)
    let Some(active_sheet_name) =
        resolve_active_sheet_name(state, selected_sheet_name_clone, in_structure_mode)
    else {
        return None;
    };

    // Resolve active category (use ai_current_category when in navigation drill-down)
    let active_category = if !state.ai_navigation_stack.is_empty() {
        &state.ai_current_category
    } else {
        selected_category_clone
    };

    // Build column lists
    let union_cols = build_union_columns(state);
    let (merged_columns, structure_schema) = build_merged_columns(
        state,
        &union_cols,
        in_structure_mode,
        in_virtual_structure_review,
        &active_sheet_name,
        active_category,
        registry,
    );

    // Gather ancestor key columns
    let ancestor_key_columns = gather_ancestor_key_columns(
        state,
        in_structure_mode,
        &active_sheet_name,
        active_category,
        registry,
    );

    // Check for undecided structures
    let undecided_structures = has_undecided_structures(state);
    let show_pending_structures = !in_structure_mode && undecided_structures;

    Some(ReviewDisplayContext {
        in_structure_mode,
        active_sheet_name,
        active_category: active_category.clone(),
        merged_columns,
        structure_schema,
        ancestor_key_columns,
        show_pending_structures,
    })
}
