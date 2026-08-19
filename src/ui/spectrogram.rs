use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::app::state::AppState;

pub fn show(ui: &mut egui::Ui, state: &AppState) {
    let desired_size = Vec2::new((ui.available_width() - 8.0).max(240.0), 420.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, Color32::from_rgb(15, 24, 35));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(56, 82, 102)),
        egui::StrokeKind::Inside,
    );

    let bands = 24;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let y = egui::lerp(rect.bottom()..=rect.top(), t);
        let color = if i % 3 == 0 {
            Color32::from_rgba_premultiplied(114, 173, 196, 36)
        } else {
            Color32::from_rgba_premultiplied(255, 255, 255, 14)
        };
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, color),
        );
    }

    let current_x = rect.center().x;
    painter.line_segment(
        [
            egui::pos2(current_x, rect.top()),
            egui::pos2(current_x, rect.bottom()),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 209, 102)),
    );

    let overlay = if state.playback.playing {
        "再生同期は次段で接続します"
    } else {
        "静止スペクトログラムの土台"
    };

    painter.text(
        rect.left_top() + egui::vec2(16.0, 16.0),
        Align2::LEFT_TOP,
        "Spectrogram",
        FontId::proportional(18.0),
        Color32::from_rgb(221, 235, 245),
    );

    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        overlay,
        FontId::proportional(18.0),
        Color32::from_rgb(175, 205, 220),
    );
}
