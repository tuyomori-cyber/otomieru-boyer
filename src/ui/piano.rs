use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, Vec2};

use crate::analysis::spectrum::{MAX_MIDI_NOTE, MIN_MIDI_NOTE};
use crate::app::state::AppState;

pub fn show(ui: &mut egui::Ui, state: &AppState) {
    let desired_size = Vec2::new(108.0, 420.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, Color32::from_rgb(238, 234, 224));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(110, 104, 92)),
        egui::StrokeKind::Inside,
    );

    let (min_midi, _max_midi, pitches) = state
        .track
        .as_ref()
        .and_then(|track| track.spectrogram.as_ref())
        .map(|spectrogram| {
            (
                spectrogram.min_midi_note,
                spectrogram.max_midi_note,
                spectrogram.pitches,
            )
        })
        .unwrap_or((
            MIN_MIDI_NOTE,
            MAX_MIDI_NOTE,
            MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1,
        ));

    let content_rect = egui::Rect::from_min_max(
        rect.left_top(),
        rect.right_bottom() - egui::vec2(0.0, 40.0),
    );

    for pitch in 0..pitches {
        let midi_note = min_midi + pitch;
        let y_top = egui::lerp(
            content_rect.bottom()..=content_rect.top(),
            (pitch + 1) as f32 / pitches.max(1) as f32,
        );
        let y_bottom = egui::lerp(
            content_rect.bottom()..=content_rect.top(),
            pitch as f32 / pitches.max(1) as f32,
        );
        let key_rect = egui::Rect::from_min_max(
            egui::pos2(content_rect.left(), y_top),
            egui::pos2(content_rect.right(), y_bottom),
        );

        let is_black = is_black_key(midi_note);
        let fill = if is_black {
            Color32::from_rgb(40, 38, 36)
        } else {
            Color32::from_rgb(247, 244, 237)
        };
        let stroke = if is_black {
            Color32::from_rgb(92, 88, 80)
        } else {
            Color32::from_rgb(196, 188, 175)
        };

        painter.rect_filled(key_rect, 0.0, fill);
        painter.line_segment(
            [
                egui::pos2(key_rect.left(), key_rect.top()),
                egui::pos2(key_rect.right(), key_rect.top()),
            ],
            Stroke::new(1.0, stroke),
        );

        if midi_note % 12 == 0 {
            painter.text(
                key_rect.left_center() + egui::vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                note_label(midi_note),
                FontId::monospace(12.0),
                if is_black {
                    Color32::from_rgb(230, 226, 218)
                } else {
                    Color32::from_rgb(70, 66, 60)
                },
            );
        }
    }

    painter.text(
        rect.center_top() + egui::vec2(0.0, 10.0),
        Align2::CENTER_TOP,
        "Piano",
        FontId::proportional(16.0),
        Color32::from_rgb(66, 62, 54),
    );
}

fn is_black_key(midi_note: usize) -> bool {
    matches!(midi_note % 12, 1 | 3 | 6 | 8 | 10)
}

fn note_label(midi_note: usize) -> String {
    let octave = (midi_note / 12) as isize - 1;
    let name = match midi_note % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => "?",
    };
    format!("{name}{octave}")
}
