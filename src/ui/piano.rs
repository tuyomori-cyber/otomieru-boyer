use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::app::state::AppState;

pub fn show(ui: &mut egui::Ui, _state: &AppState) {
    let desired_size = Vec2::new(96.0, 420.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, Color32::from_rgb(242, 238, 229));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(110, 104, 92)),
        egui::StrokeKind::Inside,
    );

    for i in 0..12 {
        let y = rect.top() + rect.height() * (i as f32 / 12.0);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgb(194, 188, 175)),
        );
    }

    painter.text(
        rect.center_top() + egui::vec2(0.0, 10.0),
        Align2::CENTER_TOP,
        "Piano",
        FontId::proportional(16.0),
        Color32::from_rgb(66, 62, 54),
    );

    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "C6\n...\nC4",
        FontId::monospace(15.0),
        Color32::from_rgb(90, 84, 73),
    );
}
