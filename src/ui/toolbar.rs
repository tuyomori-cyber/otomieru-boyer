use eframe::egui;

use crate::app::state::AppState;

#[derive(Debug, Default, Clone, Copy)]
pub struct ToolbarActions {
    pub open_requested: bool,
    pub play_pause_requested: bool,
    pub seek_to_start_requested: bool,
    pub stop_requested: bool,
}

pub fn show(ctx: &egui::Context, state: &mut AppState) -> ToolbarActions {
    let mut actions = ToolbarActions::default();

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            let dsp_controls_enabled = state.track.is_some() && !state.playback.playing;

            if ui.button("Open").clicked() {
                actions.open_requested = true;
            }

            let play_label = if state.playback.playing {
                "Pause"
            } else {
                "Play"
            };
            if ui
                .add_enabled(state.track.is_some(), egui::Button::new(play_label))
                .clicked()
            {
                actions.play_pause_requested = true;
            }

            if ui
                .add_enabled(state.track.is_some(), egui::Button::new("|<"))
                .clicked()
            {
                actions.seek_to_start_requested = true;
            }

            if ui
                .add_enabled(state.track.is_some(), egui::Button::new("Stop"))
                .clicked()
            {
                actions.stop_requested = true;
            }

            ui.separator();

            ui.add_enabled_ui(dsp_controls_enabled, |ui| {
                egui::ComboBox::from_label("Speed")
                    .selected_text(format!("{:.2}x", state.playback.dsp.speed_ratio))
                    .show_ui(ui, |ui| {
                        for speed in [0.50_f32, 0.75, 1.00, 1.25, 1.50] {
                            ui.selectable_value(
                                &mut state.playback.dsp.speed_ratio,
                                speed,
                                format!("{speed:.2}x"),
                            );
                        }
                    });
            });

            ui.separator();

            ui.add_enabled_ui(dsp_controls_enabled, |ui| {
                egui::ComboBox::from_label("Pitch")
                    .selected_text(format_pitch_label(state.playback.dsp.pitch_shift_semitones))
                    .show_ui(ui, |ui| {
                        for semitones in [-12_i32, 0, 12] {
                            ui.selectable_value(
                                &mut state.playback.dsp.pitch_shift_semitones,
                                semitones,
                                format_pitch_label(semitones),
                            );
                        }
                    });
            });

            ui.separator();

            ui.checkbox(&mut state.playback.loop_enabled, "Loop");

            ui.separator();

            ui.label("Heat");
            ui.add(
                egui::Slider::new(&mut state.spectrogram_gain_db, -24.0..=24.0)
                    .suffix(" dB")
                    .step_by(1.0),
            );

            ui.separator();

            let duration = state
                .track
                .as_ref()
                .map(|track| track.duration_seconds)
                .unwrap_or(0.0);

            ui.monospace(format!(
                "{} / {}",
                format_mm_ss(state.playback.position_seconds),
                format_mm_ss(duration)
            ));
        });

        ui.add_space(4.0);
    });

    actions
}

fn format_mm_ss(seconds: f64) -> String {
    let total_seconds = seconds.max(0.0).floor() as u64;
    let minutes = total_seconds / 60;
    let secs = total_seconds % 60;
    format!("{minutes:02}:{secs:02}")
}

fn format_pitch_label(semitones: i32) -> String {
    match semitones {
        -12 => "-1 oct".to_owned(),
        12 => "+1 oct".to_owned(),
        0 => "0 st".to_owned(),
        _ => format!("{semitones:+} st"),
    }
}
