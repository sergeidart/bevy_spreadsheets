// src/ui/elements/top_panel/sheet_management_bar.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::CloseStructureViewEvent;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, SheetInteractionState};

// Compute a fixed button width approximating `chars` visible characters for Button text
fn fixed_button_width(ui: &egui::Ui, chars: usize) -> f32 {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let ten = "0".repeat(10);
    let ten_w = ui
        .fonts(|f| f.layout_no_wrap(ten, font_id.clone(), ui.style().visuals.text_color()))
        .rect
        .width();
    let per = (ten_w / 10.0).max(4.0);
    // Aim for ~chars average glyphs plus tiny padding
    let w = per * (chars as f32) + 2.0;
    w.clamp(80.0, 420.0)
}

// Helper struct signature might need to use owned EventWriter if cloned,
// or keep &mut if that's the pattern. Let's assume it needs to match what orchestrator passes.
// If orchestrator passes cloned owned writers, this struct changes.
// If orchestrator continues to pass &mut EventWriter<T> (by reborrowing from &mut SheetEventWriters),
// this struct definition is fine.

// Sticking to the previous pattern of this struct taking mutable references to EventWriters
pub struct SheetManagementEventWriters<'a, 'w> {
    #[allow(dead_code)]
    pub close_structure_writer: Option<&'a mut EventWriter<'w, CloseStructureViewEvent>>,
    pub move_sheet_to_category:
        &'a mut EventWriter<'w, crate::sheets::events::RequestMoveSheetToCategory>,
}

/// Wrapper that draws both rows: category row and sheet controls row.
pub fn show_sheet_management_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    event_writers: &mut SheetManagementEventWriters<'a, 'w>,
) {
    ui.vertical(|ui_v| {
        show_category_picker(ui_v, state, registry, event_writers);
        ui_v.add_space(4.0);
        show_sheet_controls(ui_v, state, registry, event_writers);
    });
}

/// First bottom row: Category dropdown only
pub fn show_category_picker<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    event_writers: &mut SheetManagementEventWriters<'a, 'w>,
) {
    // Category dropdown row: [Category button] [scrollable category tabs...]
    // Ensure this row does not expand vertically beyond a single line height
    let line_h = ui.text_style_height(&egui::TextStyle::Body) + ui.style().spacing.item_spacing.y;
    let row_size = egui::Vec2::new(ui.available_width(), line_h + 6.0);
    let categories = registry.get_categories();
    const MAX_LABEL_CHARS: usize = 32;
    // Use owned copies to avoid borrowing from `state` across the closure
    let selected_category_full: String = state
        .selected_category
        .clone()
        .unwrap_or_else(|| "--Root--".to_string());
    let selected_category_text_owned: String = selected_category_full
        .chars()
        .take(MAX_LABEL_CHARS)
        .collect();
    let previous_selected_category = state.selected_category.clone();

    let mut drop_consumed = false;
    ui.allocate_ui_with_layout(row_size, egui::Layout::left_to_right(egui::Align::Min), |row| {
        // Left side: category selector and tabs
        // Prepare popup ids and filter keys
        let category_combo_id = format!(
            "category_selector_top_panel_refactored_{}",
            selected_category_full
        );
        let category_filter_key = format!("{}_filter", category_combo_id);

        // Button with fixed width ~32 chars
        let btn_w = fixed_button_width(row, MAX_LABEL_CHARS);
        let category_button = row.add_sized([btn_w, 0.0], egui::Button::new(&selected_category_text_owned));
        let category_popup_id = egui::Id::new(category_combo_id.clone());
        if category_button.clicked() {
            row.ctx().memory_mut(|mem| mem.open_popup(category_popup_id));
        }
        egui::containers::popup::popup_below_widget(
            row,
            category_popup_id,
            &category_button,
            egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
            |popup_ui| {
                // Filter row and entries
                let primary_released_popup = popup_ui.ctx().input(|i| i.pointer.primary_released());
                let mut filter_text = popup_ui
                    .memory(|mem| mem.data.get_temp::<String>(category_filter_key.clone().into()).unwrap_or_default());
                let char_w = 8.0_f32;
                let max_name_len = categories
                    .iter()
                    .filter_map(|o| o.as_ref())
                    .map(|s| s.len())
                    .max()
                    .unwrap_or(12);
                let mut popup_min_width = (max_name_len as f32) * char_w + 24.0;
                popup_min_width = popup_min_width.clamp(160.0, 900.0);
                popup_ui.set_min_width(popup_min_width);

                popup_ui.horizontal(|ui_h| {
                    ui_h.label("Filter:");
                    let avail = ui_h.available_width();
                    let default_chars = 28usize;
                    let desired = (default_chars as f32) * char_w;
                    let width = desired.min(avail).min(popup_min_width - 40.0);
                    let resp = ui_h.add(
                        egui::TextEdit::singleline(&mut filter_text)
                            .desired_width(width)
                            .hint_text("type to filter categories"),
                    );
                    if resp.changed() {
                        ui_h.memory_mut(|mem| mem.data.insert_temp(category_filter_key.clone().into(), filter_text.clone()));
                    }
                    if ui_h.small_button("x").clicked() {
                        filter_text.clear();
                        ui_h.memory_mut(|mem| mem.data.insert_temp(category_filter_key.clone().into(), filter_text.clone()));
                    }
                });

                let is_selected_root = state.selected_category.is_none();
                let pointer_pos_popup = popup_ui.ctx().input(|i| i.pointer.hover_pos());
                let current_filter = filter_text.clone();
                let root_match = "--root--".to_string();
                if current_filter.is_empty() || root_match.contains(&current_filter.to_lowercase()) || "root (uncategorized)".contains(&current_filter.to_lowercase()) {
                    let root_resp = popup_ui.selectable_label(is_selected_root, "--Root--");
                    if let Some((from_cat, ref sheet_name)) = state.dragged_sheet.as_ref() {
                        if let Some(pos) = pointer_pos_popup {
                            if root_resp.rect.contains(pos) {
                                let droppable = from_cat.is_some() && registry.get_sheet(&None, sheet_name).is_none();
                                let (fill, stroke, cursor) = if droppable {
                                    (egui::Color32::from_rgba_premultiplied(60, 200, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(60, 200, 60)), None)
                                } else {
                                    (egui::Color32::from_rgba_premultiplied(200, 60, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(200, 60, 60)), Some(egui::CursorIcon::NotAllowed))
                                };
                                if let Some(icon) = cursor { popup_ui.output_mut(|o| o.cursor_icon = icon); }
                                let painter = popup_ui.ctx().debug_painter();
                                painter.rect(root_resp.rect, egui::CornerRadius::same(4), fill, stroke, egui::StrokeKind::Outside);
                            }
                        }
                    }
                    // Drop detection consistent with column DnD: on primary release while hovered
                    if primary_released_popup {
                        if let Some(pos) = pointer_pos_popup {
                            if root_resp.rect.contains(pos) {
                                if let Some((from_cat, sheet)) = state.dragged_sheet.take() {
                                    // Only drop if valid: destination root differs and no name conflict
                                    if from_cat != None && registry.get_sheet(&None, &sheet).is_none() {
                                        event_writers.move_sheet_to_category.write(crate::sheets::events::RequestMoveSheetToCategory { from_category: from_cat, sheet_name: sheet.clone(), to_category: None });
                                        state.selected_category = None;
                                        state.selected_sheet_name = Some(sheet);
                                        state.reset_interaction_modes_and_selections();
                                        state.force_filter_recalculation = true;
                                        popup_ui.memory_mut(|mem| mem.close_popup());
                                        drop_consumed = true;
                                        popup_ui.ctx().set_dragged_id(egui::Id::NULL);
                                    } else {
                                        // Invalid drop: restore drag state to None without moving
                                        popup_ui.ctx().set_dragged_id(egui::Id::NULL);
                                    }
                                }
                            }
                        }
                    }
                    if root_resp.clicked() {
                        if !is_selected_root {
                            state.selected_category = None;
                            state.selected_sheet_name = None;
                            state.reset_interaction_modes_and_selections();
                            state.force_filter_recalculation = true;
                            popup_ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                }

                for cat_opt in categories.iter() {
                    if let Some(cat_name) = cat_opt {
                        if !current_filter.is_empty() && !cat_name.to_lowercase().contains(&current_filter.to_lowercase()) {
                            continue;
                        }
                        let is_selected_cat = state.selected_category.as_deref() == Some(cat_name.as_str());
                        let display_name: String = cat_name.chars().take(MAX_LABEL_CHARS).collect();
                        let cat_resp = popup_ui.selectable_label(is_selected_cat, display_name).on_hover_text(cat_name);
                        if let Some((from_cat, ref sheet_name)) = state.dragged_sheet.as_ref() {
                            if let Some(pos) = pointer_pos_popup {
                                if cat_resp.rect.contains(pos) {
                                    let to_cat = Some(cat_name.clone());
                                    let droppable = from_cat != &to_cat && registry.get_sheet(&to_cat, sheet_name).is_none();
                                    let (fill, stroke, cursor) = if droppable {
                                        (egui::Color32::from_rgba_premultiplied(60, 200, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(60, 200, 60)), None)
                                    } else {
                                        (egui::Color32::from_rgba_premultiplied(200, 60, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(200, 60, 60)), Some(egui::CursorIcon::NotAllowed))
                                    };
                                    if let Some(icon) = cursor { popup_ui.output_mut(|o| o.cursor_icon = icon); }
                                    let painter = popup_ui.ctx().debug_painter();
                                    painter.rect(cat_resp.rect, egui::CornerRadius::same(4), fill, stroke, egui::StrokeKind::Outside);
                                }
                            }
                        }
                        // Drop detection onto category item inside popup
                        if primary_released_popup {
                            if let Some(pos) = pointer_pos_popup {
                                if cat_resp.rect.contains(pos) {
                                    if let Some((from_cat, sheet)) = state.dragged_sheet.take() {
                                        let to_cat = Some(cat_name.clone());
                                        if from_cat != to_cat && registry.get_sheet(&to_cat, &sheet).is_none() {
                                            event_writers.move_sheet_to_category.write(crate::sheets::events::RequestMoveSheetToCategory { from_category: from_cat, sheet_name: sheet.clone(), to_category: to_cat.clone() });
                                            state.selected_category = to_cat;
                                            state.selected_sheet_name = Some(sheet);
                                            state.reset_interaction_modes_and_selections();
                                            state.force_filter_recalculation = true;
                                            popup_ui.memory_mut(|mem| mem.close_popup());
                                            drop_consumed = true;
                                            popup_ui.ctx().set_dragged_id(egui::Id::NULL);
                                            continue;
                                        } else {
                                            popup_ui.ctx().set_dragged_id(egui::Id::NULL);
                                        }
                                    }
                                }
                            }
                        }
                        if cat_resp.clicked() {
                            if !is_selected_cat {
                                state.selected_category = Some(cat_name.clone());
                                state.selected_sheet_name = None;
                                state.reset_interaction_modes_and_selections();
                                state.force_filter_recalculation = true;
                            }
                            popup_ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                }
            },
        );

    // Rename Category (icon-only) button
        let has_selected_category = state.selected_category.is_some();
        if row
            .add_enabled(has_selected_category, egui::Button::new("✏"))
            .on_hover_text("Rename category")
            .clicked()
        {
            // Open a generic rename popup state; reuse sheet rename style or a new category rename popup
            state.rename_target_category = state.selected_category.clone();
            state.rename_target_sheet.clear();
            state.new_name_input = state.rename_target_category.clone().unwrap_or_default();
            // We'll reuse the rename popup but interpret as category when sheet is empty
            state.show_rename_popup = true;
        }

        // Delete Category (icon-only) button
        if row
            .add_enabled(has_selected_category, egui::Button::new("🗑"))
            .on_hover_text("Delete current category and all its sheets (double confirmation)")
            .clicked()
        {
            state.delete_category_name = state.selected_category.clone();
            state.show_delete_category_confirm_popup = true; // first step
        }

        // Hide / Extend button
    row.add_space(6.0);
        let expanded = state.category_picker_expanded;
        let toggle_label = if expanded { "<" } else { ">" };
        if row
            .button(toggle_label)
            .on_hover_text(if expanded { "Shrink category row" } else { "Expand category row" })
            .clicked()
        {
            state.category_picker_expanded = !expanded;
        }

        // Categories list to the right (like tabs), only when expanded
        if state.category_picker_expanded {
            row.add_space(8.0);
            let line_h = row.text_style_height(&egui::TextStyle::Body) + row.style().spacing.item_spacing.y;
            // Limit the scroll area to the current row height so it doesn't expand vertically
            let avail_w = row.available_width();
            row.allocate_ui_with_layout(
                egui::Vec2::new(avail_w, line_h + 6.0),
                egui::Layout::left_to_right(egui::Align::Min),
                |inner| {
                    egui::ScrollArea::horizontal()
                        .id_salt("category_tabs_list")
                        .auto_shrink([false, false])
                        .max_height(line_h + 6.0)
                        .min_scrolled_height(0.0)
                        .show(inner, |tabs_ui| {
                    let primary_released_tabs = tabs_ui.ctx().input(|i| i.pointer.primary_released());
                    let pointer_pos_tabs = tabs_ui.ctx().input(|i| i.pointer.hover_pos());
                    tabs_ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui_th| {
                        // Root first
                        let is_root = state.selected_category.is_none();
                        let root_resp = ui_th.selectable_label(is_root, "--Root--");
                        // Visual highlight for drop target when dragging and hovered
                        if let Some((from_cat, ref sheet_name)) = state.dragged_sheet.as_ref() {
                            if let Some(pos) = pointer_pos_tabs {
                                if root_resp.rect.contains(pos) {
                                    let droppable = from_cat.is_some() && registry.get_sheet(&None, sheet_name).is_none();
                                    let (fill, stroke, cursor) = if droppable {
                                        (egui::Color32::from_rgba_premultiplied(60, 200, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(60, 200, 60)), None)
                                    } else {
                                        (egui::Color32::from_rgba_premultiplied(200, 60, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(200, 60, 60)), Some(egui::CursorIcon::NotAllowed))
                                    };
                                    if let Some(icon) = cursor { ui_th.output_mut(|o| o.cursor_icon = icon); }
                                    let painter = ui_th.ctx().debug_painter();
                                    painter.rect(root_resp.rect, egui::CornerRadius::same(4), fill, stroke, egui::StrokeKind::Outside);
                                }
                            }
                        }
                        // Drop detection for root tab: on primary release while hovered
                        if primary_released_tabs {
                            if let Some(pos) = pointer_pos_tabs {
                                if root_resp.rect.contains(pos) {
                                    if let Some((from_cat, sheet)) = state.dragged_sheet.take() {
                                        if from_cat != None && registry.get_sheet(&None, &sheet).is_none() {
                                            event_writers.move_sheet_to_category.write(crate::sheets::events::RequestMoveSheetToCategory { from_category: from_cat, sheet_name: sheet.clone(), to_category: None });
                                            state.selected_category = None;
                                            state.selected_sheet_name = Some(sheet);
                                            state.reset_interaction_modes_and_selections();
                                            state.force_filter_recalculation = true;
                                            drop_consumed = true;
                                            ui_th.ctx().set_dragged_id(egui::Id::NULL);
                                        } else {
                                            ui_th.ctx().set_dragged_id(egui::Id::NULL);
                                        }
                                    }
                                }
                            }
                        }
                        if root_resp.clicked() && !is_root {
                            state.selected_category = None;
                            state.selected_sheet_name = None;
                            state.reset_interaction_modes_and_selections();
                            state.force_filter_recalculation = true;
                        }
                        for cat_opt in categories.iter() {
                            if let Some(cat) = cat_opt {
                                let is_sel = state.selected_category.as_deref() == Some(cat.as_str());
                                let disp: String = cat.chars().take(MAX_LABEL_CHARS).collect();
                                let resp = ui_th.selectable_label(is_sel, disp).on_hover_text(cat);
                                // Visual highlight for drop target when dragging and hovered
                                if let Some((from_cat, ref sheet_name)) = state.dragged_sheet.as_ref() {
                                    if let Some(pos) = pointer_pos_tabs {
                                        if resp.rect.contains(pos) {
                                            let to_cat = Some(cat.clone());
                                            let droppable = from_cat != &to_cat && registry.get_sheet(&to_cat, sheet_name).is_none();
                                            let (fill, stroke, cursor) = if droppable {
                                                (egui::Color32::from_rgba_premultiplied(60, 200, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(60, 200, 60)), None)
                                            } else {
                                                (egui::Color32::from_rgba_premultiplied(200, 60, 60, 40), egui::Stroke::new(3.0, egui::Color32::from_rgb(200, 60, 60)), Some(egui::CursorIcon::NotAllowed))
                                            };
                                            if let Some(icon) = cursor { ui_th.output_mut(|o| o.cursor_icon = icon); }
                                            let painter = ui_th.ctx().debug_painter();
                                            painter.rect(resp.rect, egui::CornerRadius::same(4), fill, stroke, egui::StrokeKind::Outside);
                                        }
                                    }
                                }
                                // Drop detection for category tab: on primary release while hovered
                                if primary_released_tabs {
                                    if let Some(pos) = pointer_pos_tabs {
                                        if resp.rect.contains(pos) {
                                            if let Some((from_cat, sheet)) = state.dragged_sheet.take() {
                                                let to_cat = Some(cat.clone());
                                                if from_cat != to_cat && registry.get_sheet(&to_cat, &sheet).is_none() {
                                                    event_writers.move_sheet_to_category.write(crate::sheets::events::RequestMoveSheetToCategory { from_category: from_cat, sheet_name: sheet.clone(), to_category: to_cat.clone() });
                                                    state.selected_category = to_cat;
                                                    state.selected_sheet_name = Some(sheet);
                                                    state.reset_interaction_modes_and_selections();
                                                    state.force_filter_recalculation = true;
                                                    drop_consumed = true;
                                                    ui_th.ctx().set_dragged_id(egui::Id::NULL);
                                                } else {
                                                    ui_th.ctx().set_dragged_id(egui::Id::NULL);
                                                }
                                            }
                                        }
                                    }
                                }
                                if resp.clicked() && !is_sel {
                                    state.selected_category = Some(cat.clone());
                                    state.selected_sheet_name = None;
                                    state.reset_interaction_modes_and_selections();
                                    state.force_filter_recalculation = true;
                                }
                            }
                        }
                            });
                        });
                },
            );
        }
        // + Category button rightmost, left from Log
        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r| {
            r.add_space(12.0);
            let label = if state.ai_output_panel_visible { "Close" } else { "Log" };
            if r
                .button(label)
                .on_hover_text("Open/close the Log panel")
                .clicked()
            {
                state.ai_output_panel_visible = !state.ai_output_panel_visible;
            }
            // Place + Category left of Log
            if r
                .button("📁+ Category")
                .on_hover_text("Create a new category (folder)")
                .clicked()
            {
                state.show_new_category_popup = true;
                state.new_category_name_input.clear();
            }
        });
    });
    // If the primary button was released but no drop target consumed it, clear the drag state
    let released_primary = ui.input(|i| i.pointer.primary_released());
    if released_primary && state.dragged_sheet.is_some() && !drop_consumed {
        state.dragged_sheet = None;
        ui.ctx().set_dragged_id(egui::Id::NULL);
    }
    if state.selected_category != previous_selected_category {
        if state.selected_sheet_name.is_none() {
            state.reset_interaction_modes_and_selections();
        }
        state.force_filter_recalculation = true;
    }
}

/// Second bottom row: Sheet dropdown, rename/delete, tabs, and rightmost New Sheet
pub fn show_sheet_controls<'a, 'w>(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    _event_writers: &mut SheetManagementEventWriters<'a, 'w>,
) {
    // Compute sheets in current category
    let sheets_in_category = registry.get_sheet_names_in_category(&state.selected_category);

    // Layout row with rightmost Add button
    let line_h = ui.text_style_height(&egui::TextStyle::Body) + ui.style().spacing.item_spacing.y;
    let row_size = egui::Vec2::new(ui.available_width(), line_h + 6.0);
    ui.allocate_ui_with_layout(
        row_size,
        egui::Layout::right_to_left(egui::Align::Min),
        |ui_r| {
            // Rightmost: Add button
            ui_r.add_space(12.0); // right padding
            if ui_r
                .button("➕ New Sheet")
                .on_hover_text("Create a new empty sheet in the current category")
                .clicked()
            {
                state.new_sheet_target_category = state.selected_category.clone();
                state.new_sheet_name_input.clear();
                state.show_new_sheet_popup = true;
            }

            // Left side contents (sheet dropdown + rename/delete + tabs)
            ui_r.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                // Sheet dropdown button with filterable popup (fixed-width ~32 chars)
                let sheet_combo_id = format!(
                    "sheet_selector_{}",
                    state.selected_category.as_deref().unwrap_or("root")
                );
                let sheet_filter_key = format!("{}_filter", sheet_combo_id);

                // Owned display text for sheet button
                let selected_sheet_text_owned =
                    state.selected_sheet_name.as_deref().unwrap_or("--Select--");
                const MAX_LABEL_CHARS: usize = 32;
                let display_trunc: String = selected_sheet_text_owned
                    .chars()
                    .take(MAX_LABEL_CHARS)
                    .collect();
                let target_width = fixed_button_width(ui, MAX_LABEL_CHARS);

                ui.add_enabled_ui(
                    !sheets_in_category.is_empty() || state.selected_sheet_name.is_some(),
                    |ui| {
                        let previous_sheet = state.selected_sheet_name.clone();
                        let sheet_button =
                            ui.add_sized([target_width, 0.0], egui::Button::new(&display_trunc));
                        let sheet_popup_id = egui::Id::new(sheet_combo_id.clone());
                        if sheet_button.clicked() {
                            ui.ctx().memory_mut(|mem| mem.open_popup(sheet_popup_id));
                        }
                        egui::containers::popup::popup_below_widget(
                            ui,
                            sheet_popup_id,
                            &sheet_button,
                            egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                            |popup_ui| {
                                // filter input inside popup; size popup according to longest sheet name
                                let mut filter_text = popup_ui.memory(|mem| {
                                    mem.data
                                        .get_temp::<String>(sheet_filter_key.clone().into())
                                        .unwrap_or_default()
                                });
                                // compute desired popup width from longest sheet name
                                let char_w = 10.0_f32;
                                let max_name_len = sheets_in_category
                                    .iter()
                                    .map(|s| s.len())
                                    .max()
                                    .unwrap_or(12);
                                let mut popup_min_width =
                                    (max_name_len.max(12) as f32) * char_w + 24.0;
                                if popup_min_width > 900.0 {
                                    popup_min_width = 900.0;
                                }
                                if popup_min_width < 160.0 {
                                    popup_min_width = 160.0;
                                }
                                popup_ui.set_min_width(popup_min_width);

                                popup_ui.horizontal(|ui_h| {
                                    ui_h.label("Filter:");
                                    let avail = ui_h.available_width();
                                    let default_chars = 28usize;
                                    let desired = (default_chars as f32) * char_w;
                                    let width = desired.min(avail).min(popup_min_width * 0.95);
                                    let resp = ui_h.add(
                                        egui::TextEdit::singleline(&mut filter_text)
                                            .desired_width(width)
                                            .hint_text("type to filter sheets"),
                                    );
                                    if resp.changed() {
                                        ui_h.memory_mut(|mem| {
                                            mem.data.insert_temp(
                                                sheet_filter_key.clone().into(),
                                                filter_text.clone(),
                                            )
                                        });
                                    }
                                    if ui_h.small_button("x").clicked() {
                                        filter_text.clear();
                                        ui_h.memory_mut(|mem| {
                                            mem.data.insert_temp(
                                                sheet_filter_key.clone().into(),
                                                filter_text.clone(),
                                            )
                                        });
                                    }
                                });

                                popup_ui.selectable_value(
                                    &mut state.selected_sheet_name,
                                    None,
                                    "--Select--",
                                );
                                for name in sheets_in_category.iter().filter(|n| {
                                    filter_text.is_empty()
                                        || n.to_lowercase().contains(&filter_text.to_lowercase())
                                }) {
                                    let truncated: String =
                                        name.chars().take(MAX_LABEL_CHARS).collect();
                                    if popup_ui
                                        .selectable_label(
                                            state.selected_sheet_name.as_deref()
                                                == Some(name.as_str()),
                                            truncated,
                                        )
                                        .on_hover_text(name)
                                        .clicked()
                                    {
                                        state.selected_sheet_name = Some(name.clone());
                                        state.reset_interaction_modes_and_selections();
                                        state.force_filter_recalculation = true;
                                        state.show_column_options_popup = false;
                                        popup_ui.memory_mut(|mem| mem.close_popup());
                                    }
                                }
                            },
                        );

                        // after popup usage, handle selection change
                        if state.selected_sheet_name != previous_sheet {
                            if state.selected_sheet_name.is_none() {
                                // No-op: do not close any panels; just reset modes
                                state.reset_interaction_modes_and_selections();
                            }
                            state.force_filter_recalculation = true;
                            state.show_column_options_popup = false;
                        }

                        if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
                            if !registry
                                .get_sheet_names_in_category(&state.selected_category)
                                .contains(current_sheet_name)
                            {
                                state.selected_sheet_name = None;
                                state.reset_interaction_modes_and_selections();
                                state.force_filter_recalculation = true;
                                state.show_column_options_popup = false;
                            }
                        }
                    },
                );

                // Rename/Delete icon-only buttons
                let is_sheet_selected = state.selected_sheet_name.is_some();
                let can_manage_sheet = is_sheet_selected
                    && state.current_interaction_mode == SheetInteractionState::Idle;

                if ui
                    .add_enabled(can_manage_sheet, egui::Button::new("✏"))
                    .on_hover_text("Rename sheet")
                    .clicked()
                {
                    if let Some(ref name_to_rename) = state.selected_sheet_name {
                        state.rename_target_category = state.selected_category.clone();
                        state.rename_target_sheet = name_to_rename.clone();
                        state.new_name_input = state.rename_target_sheet.clone();
                        state.show_rename_popup = true;
                    }
                }
                if ui
                    .add_enabled(can_manage_sheet, egui::Button::new("🗑"))
                    .on_hover_text("Delete sheet")
                    .clicked()
                {
                    if let Some(ref name_to_delete) = state.selected_sheet_name {
                        state.delete_target_category = state.selected_category.clone();
                        state.delete_target_sheet = name_to_delete.clone();
                        state.show_delete_confirm_popup = true;
                    }
                }

                // Expand/shrink toggle for sheet row, placed to the right of Delete
                ui.add_space(6.0);
                let s_expanded = state.sheet_picker_expanded;
                let s_toggle_label = if s_expanded { "<" } else { ">" };
                if ui
                    .button(s_toggle_label)
                    .on_hover_text(if s_expanded {
                        "Shrink sheet row"
                    } else {
                        "Expand sheet row"
                    })
                    .clicked()
                {
                    state.sheet_picker_expanded = !s_expanded;
                }

                // Tabs list: all sheets in current category (selected one is highlighted)
                if state.sheet_picker_expanded && !sheets_in_category.is_empty() {
                    let line_h = ui.text_style_height(&egui::TextStyle::Body)
                        + ui.style().spacing.item_spacing.y;
                    egui::ScrollArea::horizontal()
                        .id_salt("sheet_tabs_list")
                        .auto_shrink([false, false])
                        .max_height(line_h + 6.0)
                        .min_scrolled_height(0.0)
                        .show(ui, |ui_tabs| {
                            ui_tabs.horizontal(|ui_th| {
                                for name in sheets_in_category.iter() {
                                    let is_sel =
                                        state.selected_sheet_name.as_deref() == Some(name.as_str());
                                    let resp =
                                        ui_th.selectable_label(is_sel, name).on_hover_text(name);
                                    // Start dragging using interact pattern (consistent with column DnD)
                                    let dnd_id_source = egui::Id::new("sheet_dnd_context")
                                        .with(&state.selected_category);
                                    let item_id = dnd_id_source.with(name);
                                    let interact = resp.interact(egui::Sense::drag());
                                    if interact.drag_started_by(egui::PointerButton::Primary) {
                                        state.dragged_sheet =
                                            Some((state.selected_category.clone(), name.clone()));
                                        ui_th.output_mut(|o| {
                                            o.cursor_icon = egui::CursorIcon::Grabbing
                                        });
                                        ui_th.ctx().set_dragged_id(item_id);
                                    }
                                    // Drag preview overlay near cursor (like columns)
                                    if ui_th.ctx().is_being_dragged(item_id) {
                                        egui::Area::new(item_id.with("drag_preview"))
                                            .order(egui::Order::Tooltip)
                                            .interactable(false)
                                            .current_pos(ui_th.ctx().input(|i| {
                                                i.pointer.hover_pos().unwrap_or(resp.rect.center())
                                            }))
                                            .movable(false)
                                            .show(ui_th.ctx(), |ui_preview| {
                                                let frame = egui::Frame::popup(ui_preview.style());
                                                frame.show(ui_preview, |fui| {
                                                    fui.label(format!("Moving: {}", name));
                                                });
                                            });
                                    }
                                    if resp.clicked() && !is_sel {
                                        state.selected_sheet_name = Some(name.clone());
                                        state.reset_interaction_modes_and_selections();
                                        state.force_filter_recalculation = true;
                                        state.show_column_options_popup = false;
                                    }
                                }
                            });
                        });
                }
            });
        },
    );

    // App Exit and Settings are in the top row (orchestrator)
}
