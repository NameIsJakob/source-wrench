use eframe::egui::{Rect, Response, Sense, Shape, Ui, Vec2, Widget, pos2};

#[derive(Debug, Default)]
#[allow(dead_code)]
pub enum IconType {
    #[default]
    Add,
    Remove,
    Up,
    Down,
    Check,
    X,
}

fn icon_button_ui(ui: &mut Ui, icon: IconType) -> Response {
    let scale = 1.5;
    let button_size = Vec2::splat(ui.spacing().icon_width) * scale;
    let (button_rect, response) = ui.allocate_exact_size(button_size, Sense::all());

    if ui.is_rect_visible(button_rect) {
        let style = ui.visuals().widgets.noninteractive;
        let icon_rect = Rect::from_center_size(button_rect.center(), Vec2::splat(ui.spacing().icon_width_inner) * scale);

        ui.painter().rect_filled(button_rect, 1, style.bg_fill);

        match icon {
            IconType::Add => {
                ui.painter().line_segment(
                    [pos2(icon_rect.left(), icon_rect.center().y), pos2(icon_rect.right(), icon_rect.center().y)],
                    style.fg_stroke,
                );

                ui.painter().line_segment(
                    [pos2(icon_rect.center().x, icon_rect.top()), pos2(icon_rect.center().x, icon_rect.bottom())],
                    style.fg_stroke,
                );
            }
            IconType::Remove => {
                ui.painter().line_segment(
                    [pos2(icon_rect.left(), icon_rect.center().y), pos2(icon_rect.right(), icon_rect.center().y)],
                    style.fg_stroke,
                );
            }
            IconType::Up => {
                ui.painter().add(Shape::convex_polygon(
                    vec![icon_rect.center_top(), icon_rect.right_bottom(), icon_rect.left_bottom()],
                    style.fg_stroke.color,
                    style.fg_stroke,
                ));
            }
            IconType::Down => {
                ui.painter().add(Shape::convex_polygon(
                    vec![icon_rect.left_top(), icon_rect.right_top(), icon_rect.center_bottom()],
                    style.fg_stroke.color,
                    style.fg_stroke,
                ));
            }
            IconType::Check => {
                ui.painter().add(Shape::line(
                    vec![
                        pos2(icon_rect.left(), icon_rect.center().y),
                        pos2(icon_rect.center().x, icon_rect.bottom()),
                        pos2(icon_rect.right(), icon_rect.top()),
                    ],
                    style.fg_stroke,
                ));
            }
            IconType::X => {
                ui.painter().line_segment([icon_rect.left_top(), icon_rect.right_bottom()], style.fg_stroke);
                ui.painter().line_segment([icon_rect.right_top(), icon_rect.left_bottom()], style.fg_stroke);
            }
        }
    }

    response
}

pub fn icon(icon: IconType) -> impl Widget + 'static {
    move |ui: &mut Ui| icon_button_ui(ui, icon)
}
