use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::app::state::AppState;

#[derive(Debug, Default, Clone, Copy)]
pub struct SpectrogramActions {
    pub seek_seconds: Option<f64>,
    pub page_seek_seconds: Option<f64>,
}

pub fn show(ui: &mut egui::Ui, state: &AppState) -> SpectrogramActions {
    let mut actions = SpectrogramActions::default();
    let desired_size = Vec2::new((ui.available_width() - 8.0).max(240.0), 420.0);
    let (rect, response) = ui.allocate_exact_size(
        desired_size,
        if state.playback.playing {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
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

    let page_start = state.current_page_start_seconds();
    let page_end = state.current_page_end_seconds();
    let paging = state.spectrogram_paging();
    let current_page_index = state.current_page_index();
    let page_duration = (page_end - page_start).max(0.001);
    let normalized = ((state.playback.position_seconds - page_start) / page_duration).clamp(0.0, 1.0);
    let current_x = egui::lerp(rect.left()..=rect.right(), normalized as f32);

    let page_bar_height = 16.0;
    let page_bar_margin = 14.0;
    let page_bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + page_bar_margin, rect.bottom() - 28.0),
        egui::pos2(rect.right() - page_bar_margin, rect.bottom() - 28.0 + page_bar_height),
    );

    painter.line_segment(
        [
            egui::pos2(current_x, rect.top()),
            egui::pos2(current_x, rect.bottom()),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 209, 102)),
    );

    painter.rect_filled(
        page_bar_rect,
        999.0,
        Color32::from_rgba_premultiplied(255, 255, 255, 24),
    );
    painter.rect_stroke(
        page_bar_rect,
        999.0,
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 48)),
        egui::StrokeKind::Inside,
    );

    let current_page_left = egui::lerp(
        page_bar_rect.left()..=page_bar_rect.right(),
        current_page_index as f32 / paging.page_count.max(1) as f32,
    );
    let current_page_right = egui::lerp(
        page_bar_rect.left()..=page_bar_rect.right(),
        (current_page_index + 1) as f32 / paging.page_count.max(1) as f32,
    );
    let current_page_rect = egui::Rect::from_min_max(
        egui::pos2(current_page_left, page_bar_rect.top()),
        egui::pos2(current_page_right.max(current_page_left + 2.0), page_bar_rect.bottom()),
    );
    painter.rect_filled(current_page_rect, 4.0, Color32::from_rgb(90, 168, 204));

    for page in 1..paging.page_count {
        let x = egui::lerp(
            page_bar_rect.left()..=page_bar_rect.right(),
            page as f32 / paging.page_count as f32,
        );
        painter.line_segment(
            [egui::pos2(x, page_bar_rect.top()), egui::pos2(x, page_bar_rect.bottom())],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 30)),
        );
    }

    painter.line_segment(
        [
            egui::pos2(
                current_x.clamp(page_bar_rect.left(), page_bar_rect.right()),
                page_bar_rect.top(),
            ),
            egui::pos2(
                current_x.clamp(page_bar_rect.left(), page_bar_rect.right()),
                page_bar_rect.bottom(),
            ),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 209, 102)),
    );

    let overlay = if state.playback.playing {
        "再生中はページ送りのみ / 縦線追従"
    } else {
        "停止中はクリックでシーク"
    };

    painter.text(
        rect.left_top() + egui::vec2(16.0, 16.0),
        Align2::LEFT_TOP,
        "Spectrogram",
        FontId::proportional(18.0),
        Color32::from_rgb(221, 235, 245),
    );

    painter.text(
        rect.left_top() + egui::vec2(16.0, 42.0),
        Align2::LEFT_TOP,
        format!(
            "Page {}/{} | {:.0} - {:.0} sec\n{}",
            current_page_index + 1,
            paging.page_count,
            page_start,
            page_end,
            overlay
        ),
        FontId::proportional(16.0),
        Color32::from_rgb(175, 205, 220),
    );

    painter.text(
        page_bar_rect.left_bottom() + egui::vec2(0.0, 20.0),
        Align2::LEFT_BOTTOM,
        "Page 1",
        FontId::proportional(13.0),
        Color32::from_rgb(165, 188, 204),
    );
    painter.text(
        page_bar_rect.right_bottom() + egui::vec2(0.0, 20.0),
        Align2::RIGHT_BOTTOM,
        format!("Page {}", paging.page_count),
        FontId::proportional(13.0),
        Color32::from_rgb(165, 188, 204),
    );

    if !state.playback.playing && response.clicked() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            if page_bar_rect.contains(pointer_pos) {
                let t = ((pointer_pos.x - page_bar_rect.left()) / page_bar_rect.width())
                    .clamp(0.0, 0.999_999) as f64;
                let page_index = (t * paging.page_count as f64).floor() as usize;
                let page_seek = page_index.min(paging.page_count.saturating_sub(1)) as f64
                    * paging.page_duration_seconds;
                actions.page_seek_seconds = Some(page_seek);
            } else {
                let t = ((pointer_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
                actions.seek_seconds = Some(page_start + page_duration * t);
            }
        }
    }

    actions
}
