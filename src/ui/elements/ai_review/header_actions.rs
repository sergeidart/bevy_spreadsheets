// Header actions (Accept All / Decline All) extraction
use crate::sheets::systems::ai_review::structure_persistence::persist_structure_detail_changes;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
use bevy_egui::egui::{self, Color32, RichText};

pub struct HeaderActionResult {
    pub accept_all: bool,
    pub decline_all: bool,
}

pub fn draw_header_actions(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    pending_structures: bool,
    registry: &SheetRegistry,
) -> HeaderActionResult {
    let mut accept_all_clicked = false;
    let mut decline_all_clicked = false;

    // Check for ESC key to exit structure detail view OR navigation drill-down
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
            // Persist changes before exiting
            persist_structure_detail_changes(state, detail_ctx);
            // Restore top-level reviews before exiting
            state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
            state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
            state.ai_structure_detail_context = None;
        } else if !state.ai_navigation_stack.is_empty() {
            // Navigate back from child table drill-down
            use crate::ui::elements::ai_review::navigation::navigate_back;
            navigate_back(state, registry);
        }
    }

    ui.horizontal(|ui| {
        // Show back button if in structure detail mode OR navigation drill-down
        let in_navigation_drilldown = !state.ai_navigation_stack.is_empty();
        let in_structure_detail = state.ai_structure_detail_context.is_some();
        
        if in_structure_detail || in_navigation_drilldown {
            if ui.button(RichText::new("◀ Back").strong()).clicked() {
                if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                    // Persist changes before exiting
                    persist_structure_detail_changes(state, detail_ctx);
                    // Restore top-level reviews before exiting
                    state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                    state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
                    state.ai_structure_detail_context = None;
                } else if in_navigation_drilldown {
                    // Navigate back from child table drill-down
                    use crate::ui::elements::ai_review::navigation::navigate_back;
                    navigate_back(state, registry);
                }
            }
            ui.add_space(8.0);
            
            // Show appropriate title based on context
            if in_structure_detail {
                ui.label(RichText::new("AI Review - Structure Detail").heading());
            } else if in_navigation_drilldown {
                // Build breadcrumb trail for child table navigation
                let mut breadcrumb = String::new();
                for (idx, nav_ctx) in state.ai_navigation_stack.iter().enumerate() {
                    if idx > 0 {
                        breadcrumb.push_str(" › ");
                    }
                    breadcrumb.push_str(&nav_ctx.sheet_name);
                    if let Some(ref display_name) = nav_ctx.parent_display_name {
                        breadcrumb.push_str(&format!(" ({})", display_name));
                    }
                }
                breadcrumb.push_str(" › ");
                breadcrumb.push_str(&state.ai_current_sheet);
                
                ui.label(RichText::new(format!("AI Review - {}", breadcrumb)).heading());
            }
        } else {
            // Regular review mode: simple header (virtual structures deprecated)
            ui.label(RichText::new("AI Review").heading());
        }
        ui.add_space(12.0);
        let has_actionable = !state.ai_row_reviews.is_empty()
            || state.ai_new_row_reviews.iter().any(|nr| {
                nr.duplicate_match_row.is_none()
                    || (nr.duplicate_match_row.is_some() && nr.merge_decided)
            });
        let accept_all_enabled =
            has_actionable && !state.ai_batch_has_undecided_merge && !pending_structures;
        let accept_btn = ui.add_enabled(
            accept_all_enabled,
            egui::Button::new(RichText::new("Accept All").strong()),
        );
        if accept_btn.clicked() && accept_all_enabled {
            accept_all_clicked = true;
        }
        if !accept_all_enabled {
            let mut reason = String::new();
            if state.ai_batch_has_undecided_merge {
                reason.push_str("Pending Merge/Separate decisions (press Decide). ");
            }
            if pending_structures {
                reason.push_str("Pending structure reviews (click structures to review). ");
            }
            if !has_actionable {
                reason.push_str("No changes to accept.");
            }
            if !reason.is_empty() {
                accept_btn.on_hover_text(reason);
            }
        } else {
            accept_btn.on_hover_text("Apply all AI and merge decisions");
        }
        let decline_btn = ui.button(RichText::new("Decline All").color(Color32::LIGHT_RED));
        if decline_btn.clicked() {
            decline_all_clicked = true;
        }
        decline_btn.on_hover_text("Discard remaining suggestions");
    });
    HeaderActionResult {
        accept_all: accept_all_clicked,
        decline_all: decline_all_clicked,
    }
}
