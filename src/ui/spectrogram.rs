use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::app::state::AppState;

#[derive(Debug, Default, Clone, Copy)]
pub struct SpectrogramActions {
    pub seek_seconds: Option<f64>,
    pub page_seek_seconds: Option<f64>,
    pub preview_midi_note: Option<u8>,
    pub stop_preview: bool,
}

pub fn show(ui: &mut egui::Ui, state: &AppState) -> SpectrogramActions {
    let mut actions = SpectrogramActions::default();
    let desired_size = Vec2::new((ui.available_width() - 8.0).max(240.0), 420.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, Color32::from_rgb(15, 24, 35));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(56, 82, 102)),
        egui::StrokeKind::Inside,
    );

    let page_start = state.current_page_start_seconds();
    let page_end = state.current_page_end_seconds();
    let paging = state.spectrogram_paging();
    let current_page_index = state.current_page_index();
    let page_duration = (page_end - page_start).max(0.001);
    let normalized = ((state.playback.position_seconds - page_start) / page_duration).clamp(0.0, 1.0);
    let current_x = egui::lerp(rect.left()..=rect.right(), normalized as f32);

    draw_spectrogram_body(&painter, rect, state, page_start, page_end);

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

    let content_rect = egui::Rect::from_min_max(rect.left_top(), rect.right_bottom() - egui::vec2(0.0, 40.0));

    if let Some(pointer_pos) = response.interact_pointer_pos() {
        if page_bar_rect.contains(pointer_pos) && !state.playback.playing && response.clicked() {
            let t = ((pointer_pos.x - page_bar_rect.left()) / page_bar_rect.width())
                .clamp(0.0, 0.999_999) as f64;
            let page_index = (t * paging.page_count as f64).floor() as usize;
            let page_seek = page_index.min(paging.page_count.saturating_sub(1)) as f64
                * paging.page_duration_seconds;
            actions.page_seek_seconds = Some(page_seek);
        } else if content_rect.contains(pointer_pos) {
            if !state.playback.playing && response.clicked() {
                let t = ((pointer_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
                actions.seek_seconds = Some(page_start + page_duration * t);
            }

            let pointer_down = ui.input(|input| input.pointer.primary_down());
            if pointer_down {
                if let Some(track) = &state.track {
                    if let Some(spectrogram) = &track.spectrogram {
                        let pitch_t = ((content_rect.bottom() - pointer_pos.y) / content_rect.height())
                            .clamp(0.0, 0.999_999);
                        let pitch_index =
                            (pitch_t * spectrogram.pitches.max(1) as f32).floor() as usize;
                        let midi_note = spectrogram.min_midi_note
                            + pitch_index.min(spectrogram.pitches.saturating_sub(1));
                        actions.preview_midi_note = Some(midi_note as u8);
                    }
                }
            } else if state.preview_tone_active {
                actions.stop_preview = true;
            }
        }
    } else if state.preview_tone_active && !ui.input(|input| input.pointer.primary_down()) {
        actions.stop_preview = true;
    }

    actions
}

fn draw_spectrogram_body(
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &AppState,
    page_start: f64,
    page_end: f64,
) {
    let Some(track) = &state.track else {
        draw_placeholder_grid(painter, rect);
        return;
    };
    let Some(spectrogram) = &track.spectrogram else {
        draw_placeholder_grid(painter, rect);
        return;
    };
    if spectrogram.frames == 0 || spectrogram.pitches == 0 {
        draw_placeholder_grid(painter, rect);
        return;
    }

    let content_rect = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(0.0, 0.0),
        rect.right_bottom() - egui::vec2(0.0, 40.0),
    );

    let start_frame = ((page_start / spectrogram.frame_duration_seconds).floor() as usize)
        .min(spectrogram.frames.saturating_sub(1));
    let end_frame = ((page_end / spectrogram.frame_duration_seconds).ceil() as usize)
        .clamp(start_frame + 1, spectrogram.frames);
    let visible_frames = end_frame.saturating_sub(start_frame).max(1);

    for local_frame in 0..visible_frames {
        let frame_index = start_frame + local_frame;
        let x0 = egui::lerp(
            content_rect.left()..=content_rect.right(),
            local_frame as f32 / visible_frames as f32,
        );
        let x1 = egui::lerp(
            content_rect.left()..=content_rect.right(),
            (local_frame + 1) as f32 / visible_frames as f32,
        );

        for pitch in 0..spectrogram.pitches {
            let intensity = apply_display_gain(
                spectrogram.intensity_at(frame_index, pitch),
                state.spectrogram_gain_db,
            );
            if intensity <= 0.01 {
                continue;
            }

            let y0 = egui::lerp(
                content_rect.bottom()..=content_rect.top(),
                pitch as f32 / spectrogram.pitches.max(1) as f32,
            );
            let y1 = egui::lerp(
                content_rect.bottom()..=content_rect.top(),
                (pitch + 1) as f32 / spectrogram.pitches.max(1) as f32,
            );

            let color = spectrogram_color(intensity);
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(x0, y1), egui::pos2(x1.max(x0 + 1.0), y0)),
                0.0,
                color,
            );
        }
    }

    draw_pitch_guides(painter, content_rect, spectrogram.pitches);
}

fn draw_placeholder_grid(painter: &egui::Painter, rect: egui::Rect) {
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
}

fn draw_pitch_guides(painter: &egui::Painter, rect: egui::Rect, pitches: usize) {
    for i in 0..=pitches {
        if i % 12 != 0 {
            continue;
        }
        let y = egui::lerp(rect.bottom()..=rect.top(), i as f32 / pitches.max(1) as f32);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 22)),
        );
    }
}

fn spectrogram_color(intensity: f32) -> Color32 {
    let t = intensity.clamp(0.0, 1.0);
    let (r, g, b) = thermal_gradient(t);
    let a = egui::lerp(20.0..=255.0, t) as u8;
    Color32::from_rgba_premultiplied(r, g, b, a)
}

fn apply_display_gain(intensity: f32, gain_db: f32) -> f32 {
    let gain = 10.0_f32.powf(gain_db / 20.0);
    (intensity * gain).clamp(0.0, 1.0)
}

fn thermal_gradient(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);

    if t < 0.2 {
        lerp_rgb((8, 6, 26), (42, 20, 90), t / 0.2)
    } else if t < 0.4 {
        lerp_rgb((42, 20, 90), (24, 110, 182), (t - 0.2) / 0.2)
    } else if t < 0.6 {
        lerp_rgb((24, 110, 182), (0, 188, 156), (t - 0.4) / 0.2)
    } else if t < 0.8 {
        lerp_rgb((0, 188, 156), (255, 196, 0), (t - 0.6) / 0.2)
    } else {
        lerp_rgb((255, 196, 0), (255, 72, 32), (t - 0.8) / 0.2)
    }
}

fn lerp_rgb(from: (u8, u8, u8), to: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        egui::lerp(from.0 as f32..=to.0 as f32, t) as u8,
        egui::lerp(from.1 as f32..=to.1 as f32, t) as u8,
        egui::lerp(from.2 as f32..=to.2 as f32, t) as u8,
    )
}
