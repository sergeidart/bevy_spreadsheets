use crate::sheets::definitions::StructureFieldDefinition;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Contains all the display context needed for rendering the AI batch review UI
pub struct ReviewDisplayContext {
    pub in_structure_detail_mode: bool,
    pub in_virtual_structure_review: bool,
    pub in_structure_mode: bool,
    pub active_sheet_name: String,
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
    let in_virtual_structure_review =
        !in_structure_detail_mode && !state.virtual_structure_stack.is_empty();
    let in_structure_mode = in_structure_detail_mode;

    // Resolve active sheet name
    let Some(active_sheet_name) =
        resolve_active_sheet_name(state, selected_sheet_name_clone, in_structure_mode)
    else {
        return None;
    };

    // Build column lists
    let union_cols = build_union_columns(state);
    let (merged_columns, structure_schema) = build_merged_columns(
        state,
        &union_cols,
        in_structure_mode,
        in_virtual_structure_review,
        &active_sheet_name,
        selected_category_clone,
        registry,
    );

    // Gather ancestor key columns
    let ancestor_key_columns = gather_ancestor_key_columns(
        state,
        in_structure_mode,
        &active_sheet_name,
        selected_category_clone,
        registry,
    );

    // Check for undecided structures
    let undecided_structures = has_undecided_structures(state);
    let show_pending_structures = !in_structure_mode && undecided_structures;

    Some(ReviewDisplayContext {
        in_structure_detail_mode,
        in_virtual_structure_review,
        in_structure_mode,
        active_sheet_name,
        merged_columns,
        structure_schema,
        ancestor_key_columns,
        show_pending_structures,
    })
}
