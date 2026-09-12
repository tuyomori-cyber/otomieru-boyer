use eframe::egui;

use crate::app::state::{AppState, PITCH_CLASS_NAMES, ScalePreset};
use crate::audio::preview_tone::PreviewTimbre;
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

            ui.checkbox(&mut state.playback.loop_enabled, "Loop");

            ui.separator();

            ui.label("Heat");
            ui.add(
                egui::Slider::new(&mut state.spectrogram_gain_db, -24.0..=24.0)
                    .suffix(" dB")
                    .step_by(1.0),
            );

            if ui.button("調整・設定").clicked() {
                state.settings_popup_open = true;
            }

            ui.add(
                egui::Slider::new(&mut state.preview_tone_amplitude, 0.0..=0.5)
                    .text("試聴音量")
                    .suffix("")
                    .custom_formatter(|amplitude, _| format!("{:.0}%", amplitude * 100.0)),
            );

            ui.separator();

            let duration = state
                .track
                .as_ref()
                .map(|track| track.duration_seconds)
                .unwrap_or(0.0);

            ui.monospace(format!(
                "{} / {}",
                format_mm_ss(state.display_playhead_position_seconds),
                format_mm_ss(duration)
            ));
        });

        ui.add_space(4.0);
    });

    show_settings_dialog(ctx, state);

    actions
}

fn show_settings_dialog(ctx: &egui::Context, state: &mut AppState) {
    let mut settings_popup_open = state.settings_popup_open;
    egui::Window::new("調整・設定")
        .open(&mut settings_popup_open)
        .resizable(false)
        .default_width(720.0)
        .show(ctx, |ui| {
            ui.columns(2, |columns| {
                columns[0].heading("EQ");
                show_equalizer_settings(&mut columns[0], state);
                columns[1].heading("基音強調");
                show_fundamental_settings(&mut columns[1], state);
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("音色");
                show_timbre_settings(ui, state);
            });
            ui.separator();
            ui.heading("試聴チューニング");
            ui.label("元音源に合わせて、スペクトログラム押下時の試聴音だけを調整します。");
            ui.add(
                egui::Slider::new(&mut state.preview_reference_a4_hz, 430.0..=450.0)
                    .text("A4 基準ピッチ")
                    .suffix(" Hz")
                    .step_by(0.1),
            );
        });
    state.settings_popup_open = settings_popup_open;
}

fn show_equalizer_settings(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("再生停止中に調整できます。表示にも反映されます。");
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
}

fn show_fundamental_settings(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("倍音列から基音候補を強調します。再生音には影響しません。");
    ui.add(
        egui::Slider::new(&mut state.fundamental_emphasis, 0.0..=100.0)
            .text("強調")
            .suffix(" %")
            .step_by(5.0),
    );
    let mut scale_changed = false;
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("Root")
            .selected_text(PITCH_CLASS_NAMES[state.scale_root])
            .show_ui(ui, |ui| {
                for (index, name) in PITCH_CLASS_NAMES.iter().enumerate() {
                    scale_changed |= ui
                        .selectable_value(&mut state.scale_root, index, *name)
                        .changed();
                }
            });
        egui::ComboBox::from_label("Scale")
            .selected_text(state.scale_preset.label())
            .show_ui(ui, |ui| {
                for preset in ScalePreset::ALL {
                    scale_changed |= ui
                        .selectable_value(&mut state.scale_preset, preset, preset.label())
                        .changed();
                }
            });
    });
    if scale_changed {
        state.apply_scale_preset();
    }
    ui.label("強調する音（クリックで個別に変更）");
    egui::Grid::new("emphasized_pitch_classes")
        .num_columns(4)
        .show(ui, |ui| {
            for (index, name) in PITCH_CLASS_NAMES.iter().enumerate() {
                ui.checkbox(&mut state.emphasized_pitch_classes[index], *name);
                if index % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    ui.add(
        egui::Slider::new(&mut state.unemphasized_pitch_attenuation, 0.0..=100.0)
            .text("指定外の減衰")
            .suffix(" %")
            .step_by(5.0),
    );
}

fn show_timbre_settings(ui: &mut egui::Ui, state: &mut AppState) {
    for timbre in PreviewTimbre::ALL {
        ui.radio_value(&mut state.preview_timbre, timbre, timbre.label());
    }
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
