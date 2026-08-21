use eframe::egui;

use crate::app::state::AppState;
use crate::model::{EQ_BAND_COUNT, EQ_BAND_FREQUENCIES_HZ};

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

            if ui
                .add_enabled(dsp_controls_enabled, egui::Button::new("EQ"))
                .clicked()
            {
                state.equalizer_popup_open = true;
            }

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

    let mut equalizer_popup_open = state.equalizer_popup_open;
    egui::Window::new("Equalizer")
        .open(&mut equalizer_popup_open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("再生停止中に調整できます。スペクトラム表示にも反映されます。");
            ui.add_enabled_ui(state.track.is_some() && !state.playback.playing, |ui| {
                for (index, frequency_hz) in EQ_BAND_FREQUENCIES_HZ.iter().copied().enumerate() {
                    ui.add(
                        egui::Slider::new(
                            &mut state.playback.dsp.equalizer.gains_db[index],
                            -12.0..=12.0,
                        )
                        .text(format_frequency(frequency_hz))
                        .suffix(" dB")
                        .step_by(0.5),
                    );
                }
                if ui.button("Reset EQ").clicked() {
                    state.playback.dsp.equalizer.gains_db = [0.0; EQ_BAND_COUNT];
                }
            });
        });
    state.equalizer_popup_open = equalizer_popup_open;

    actions
}

fn format_frequency(frequency_hz: f32) -> String {
    if frequency_hz >= 1_000.0 {
        format!("{:.0} kHz", frequency_hz / 1_000.0)
    } else {
        format!("{frequency_hz:.0} Hz")
    }
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
