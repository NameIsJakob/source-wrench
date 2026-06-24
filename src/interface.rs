use eframe::egui;

mod icons;
mod lists;
pub mod tabs;

fn fix_naming_conflicts<T: crate::input::NamedData>(entries: &mut [T], check_index: usize) {
    while entries
        .iter()
        .enumerate()
        .any(|(entry_index, entry)| entry_index != check_index && entry.get_name().eq(entries[check_index].get_name()))
    {
        let check_entry = &mut entries[check_index];
        let check_entry_name = check_entry.get_name();
        if let Some(numbered_index) = check_entry_name.rfind('#') {
            let (name, number) = check_entry_name.split_at(numbered_index);
            if let Ok(index) = number[1..].parse::<usize>() {
                check_entry.set_name(format!("{}#{}", name, index + 1));
                continue;
            }
        }
        check_entry.set_name(format!("{} #0", check_entry_name));
    }
}

fn toggle_ui_compact(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, ""));

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter().rect(rect, radius, visuals.bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter().circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    response
}
