pub mod state;

use eframe::egui;
use std::sync::Arc;
use std::time::Instant;

use crate::app::state::{AppState, UiFrameMetrics};
use crate::audio::decoder::decode_file;
use crate::audio::player::{AudioPlayer, TransportState, UI_REPAINT_INTERVAL};
use crate::audio::preview_tone::{PreviewTonePlayer, PreviewToneRequest};
use crate::model::PlaybackDspSettings;
use crate::ui::{piano, spectrogram, timeline, toolbar};

pub struct OtomieruApp {
    state: AppState,
    player: AudioPlayer,
    preview_tone_player: Option<PreviewTonePlayer>,
    last_applied_dsp_settings: Option<PlaybackDspSettings>,
    playhead_interpolator: PlayheadInterpolator,
    ui_frame_monitor: UiFrameMonitor,
}

impl OtomieruApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_japanese_fonts(&cc.egui_ctx);
        Self {
            state: AppState::default(),
            player: AudioPlayer::default(),
            preview_tone_player: PreviewTonePlayer::new().ok(),
            last_applied_dsp_settings: None,
            playhead_interpolator: PlayheadInterpolator::default(),
            ui_frame_monitor: UiFrameMonitor::default(),
        }
    }

    fn open_audio_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("audio", &["wav", "mp3", "flac", "ogg", "m4a", "aac"])
            .pick_file()
        else {
            self.state.set_status("ファイル選択をキャンセルしました。");
            return;
        };

        self.state
            .set_status(format!("読み込み中: {}", path.display()));

        match decode_file(&path) {
            Ok(decoded) => {
                let track = crate::model::Track::from_decoded(decoded);
                match self.player.load_track(&track) {
                    Ok(()) => {
                        self.state.set_loaded_track(path, track);
                        self.playhead_interpolator.reset(0.0);
                        self.last_applied_dsp_settings = None;
                    }
                    Err(error) => {
                        self.state.track = None;
                        self.state.loaded_file_path = None;
                        self.state.display_playhead_position_seconds = 0.0;
                        self.playhead_interpolator.reset(0.0);
                        self.last_applied_dsp_settings = None;
                        self.state
                            .set_status(format!("再生準備に失敗しました: {error}"));
                    }
                }
            }
            Err(error) => {
                self.state.track = None;
                self.state.loaded_file_path = None;
                self.state.display_playhead_position_seconds = 0.0;
                self.playhead_interpolator.reset(0.0);
                self.last_applied_dsp_settings = None;
                self.state
                    .set_status(format!("読み込みに失敗しました: {error}"));
            }
        }
    }

    fn sync_transport_state(&mut self) {
        self.player.set_loop_enabled(
            self.state.playback.loop_enabled && self.state.selection.normalized().is_some(),
        );
        self.player
            .set_loop_range(self.state.selection.normalized());
    }

    fn sync_dsp_settings(&mut self) {
        if self.state.track.is_none() {
            self.last_applied_dsp_settings = None;
            return;
        }

        if !self.state.playback.playing
            && self.last_applied_dsp_settings != Some(self.state.playback.dsp)
        {
            self.player.set_dsp_settings(self.state.playback.dsp);
            self.last_applied_dsp_settings = Some(self.state.playback.dsp);
        }
    }
}

fn configure_japanese_fonts(ctx: &egui::Context) {
    const FONT_PATHS: [&str; 3] = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/ipaexg.ttf",
    ];

    let Some(font_bytes) = FONT_PATHS.iter().find_map(|path| std::fs::read(path).ok()) else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    let font_name = "japanese-ui".to_owned();
    fonts.font_data.insert(
        font_name.clone(),
        Arc::new(egui::FontData::from_owned(font_bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(font_name.clone());
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for OtomieruApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.state.ui_frame_metrics = self.ui_frame_monitor.observe(ctx.pixels_per_point());
        let space_pressed =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Space));
        let actions = toolbar::show(ctx, &mut self.state);
        if actions.open_requested {
            self.open_audio_file();
        }
        self.sync_transport_state();
        self.sync_dsp_settings();
        if actions.seek_to_start_requested {
            self.player.seek_to_start();
            self.state.playback.position_seconds = 0.0;
            self.state.display_playhead_position_seconds = 0.0;
            self.playhead_interpolator.reset(0.0);
        }
        if actions.play_pause_requested || (space_pressed && self.state.track.is_some()) {
            if self.state.playback.playing {
                self.player.pause();
                self.state.playback.playing = false;
            } else if let Err(error) = self.player.play() {
                self.state
                    .set_status(format!("再生開始に失敗しました: {error}"));
            } else {
                self.state.playback.playing = true;
            }
        }
        if actions.stop_requested {
            self.player.stop();
            self.state.playback.playing = false;
            self.state.playback.position_seconds = 0.0;
            self.state.display_playhead_position_seconds = 0.0;
            self.playhead_interpolator.reset(0.0);
        }

        let previous_position_seconds = self.state.playback.position_seconds;
        let snapshot = self.player.snapshot();
        self.state.playback.playing = snapshot.transport == TransportState::Playing;
        self.state.playback.position_seconds = snapshot.position_seconds;
        self.state.display_playhead_position_seconds = self.playhead_interpolator.update(
            snapshot.position_seconds,
            self.state.playback.playing,
            self.state.playback.dsp.speed_ratio as f64,
        );
        self.state
            .follow_playhead_if_needed(previous_position_seconds);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                let _timeline_actions = timeline::show(ui, &mut self.state, 108.0);
                self.sync_transport_state();
                self.sync_dsp_settings();
                ui.add_space(4.0);

                // ステータス行と余白を残し、それ以外をスペクトラムとピアノへ使う。
                let visualization_height = (ui.available_height() - 32.0).max(240.0);
                ui.horizontal(|ui| {
                    piano::show(ui, &self.state, visualization_height);
                    let spectrogram_actions =
                        spectrogram::show(ui, &self.state, visualization_height);
                    if let Some(seconds) = spectrogram_actions.seek_seconds {
                        self.player.seek_to_seconds(seconds);
                        self.state.playback.position_seconds = seconds;
                        self.state.display_playhead_position_seconds = seconds;
                        self.playhead_interpolator.reset(seconds);
                    }
                    if let Some(view_start_seconds) = spectrogram_actions.view_start_seconds {
                        self.state.set_view_start_seconds(view_start_seconds);
                    }
                    if let Some((anchor_seconds, factor)) = spectrogram_actions.zoom_at {
                        self.state.zoom_view_at(anchor_seconds, factor);
                    }
                    if let Some((anchor_midi, factor)) = spectrogram_actions.pitch_zoom_at {
                        self.state.zoom_pitch_at(anchor_midi, factor);
                    }
                    if let Some(midi_note) = spectrogram_actions.preview_midi_note {
                        let preview_changed = self.state.preview_midi_note != Some(midi_note);
                        self.state.preview_midi_note = Some(midi_note);
                        if let Some(preview_tone_player) = &self.preview_tone_player {
                            preview_tone_player.update_preview(PreviewToneRequest { midi_note });
                        }
                        if preview_changed {
                            ctx.request_repaint();
                        }
                    }
                    if spectrogram_actions.stop_preview {
                        if let Some(preview_tone_player) = &self.preview_tone_player {
                            preview_tone_player.stop_preview();
                        }
                        if self.state.preview_midi_note.take().is_some() {
                            ctx.request_repaint();
                        }
                    }
                });

                ui.add_space(8.0);
                ui.label(self.state.status_message());
            });
        });

        if self.state.playback.playing {
            ctx.request_repaint_after(UI_REPAINT_INTERVAL);
        }
    }
}

#[derive(Default)]
struct UiFrameMonitor {
    last_frame_at: Option<Instant>,
    smoothed_frames_per_second: f32,
}

impl UiFrameMonitor {
    fn observe(&mut self, pixels_per_point: f32) -> UiFrameMetrics {
        self.observe_at(Instant::now(), pixels_per_point)
    }

    fn observe_at(&mut self, now: Instant, pixels_per_point: f32) -> UiFrameMetrics {
        let frame_time_ms = self
            .last_frame_at
            .map(|previous| now.duration_since(previous).as_secs_f32() * 1_000.0)
            .unwrap_or(0.0);
        self.last_frame_at = Some(now);

        if frame_time_ms > 0.0 && frame_time_ms < 250.0 {
            let instantaneous_fps = 1_000.0 / frame_time_ms;
            self.smoothed_frames_per_second = if self.smoothed_frames_per_second <= 0.0 {
                instantaneous_fps
            } else {
                self.smoothed_frames_per_second * 0.85 + instantaneous_fps * 0.15
            };
        } else if frame_time_ms >= 250.0 {
            self.smoothed_frames_per_second = 0.0;
        }

        UiFrameMetrics {
            frames_per_second: self.smoothed_frames_per_second,
            frame_time_ms,
            pixels_per_point,
        }
    }
}

#[derive(Default)]
struct PlayheadInterpolator {
    anchor_position_seconds: f64,
    anchor_time: Option<Instant>,
    last_snapshot_position_seconds: Option<f64>,
}

impl PlayheadInterpolator {
    fn reset(&mut self, position_seconds: f64) {
        self.anchor_position_seconds = position_seconds;
        self.anchor_time = Some(Instant::now());
        self.last_snapshot_position_seconds = Some(position_seconds);
    }

    fn update(&mut self, snapshot_position_seconds: f64, playing: bool, speed_ratio: f64) -> f64 {
        self.update_at(
            snapshot_position_seconds,
            playing,
            speed_ratio,
            Instant::now(),
        )
    }

    fn update_at(
        &mut self,
        snapshot_position_seconds: f64,
        playing: bool,
        speed_ratio: f64,
        now: Instant,
    ) -> f64 {
        let previous_snapshot = self.last_snapshot_position_seconds;
        let position_changed = previous_snapshot
            .map(|previous| (snapshot_position_seconds - previous).abs() > 1e-6)
            .unwrap_or(true);
        let position_rewound = previous_snapshot
            .map(|previous| snapshot_position_seconds + 1e-6 < previous)
            .unwrap_or(false);

        if !playing || position_changed || position_rewound || self.anchor_time.is_none() {
            self.anchor_position_seconds = snapshot_position_seconds;
            self.anchor_time = Some(now);
        }
        self.last_snapshot_position_seconds = Some(snapshot_position_seconds);

        if !playing {
            return snapshot_position_seconds;
        }

        let elapsed = now
            .duration_since(self.anchor_time.expect("playing playhead has an anchor"))
            .as_secs_f64();
        self.anchor_position_seconds + elapsed * speed_ratio.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{PlayheadInterpolator, UiFrameMonitor};
    use std::time::{Duration, Instant};

    #[test]
    fn interpolator_advances_between_audio_snapshots() {
        let now = Instant::now();
        let mut interpolator = PlayheadInterpolator::default();

        assert_eq!(interpolator.update_at(1.0, true, 1.0, now), 1.0);
        let interpolated = interpolator.update_at(1.0, true, 1.0, now + Duration::from_millis(10));

        assert!((interpolated - 1.01).abs() < 1e-6);
    }

    #[test]
    fn interpolator_resets_immediately_on_a_rewind_or_pause() {
        let now = Instant::now();
        let mut interpolator = PlayheadInterpolator::default();
        interpolator.update_at(5.0, true, 1.0, now);

        assert_eq!(
            interpolator.update_at(1.0, true, 1.0, now + Duration::from_millis(10)),
            1.0
        );
        assert_eq!(
            interpolator.update_at(1.0, false, 1.0, now + Duration::from_millis(20)),
            1.0
        );
    }

    #[test]
    fn frame_monitor_reports_update_cadence_and_scale() {
        let now = Instant::now();
        let mut monitor = UiFrameMonitor::default();
        monitor.observe_at(now, 1.0);

        let metrics = monitor.observe_at(now + Duration::from_millis(20), 1.5);

        assert!((metrics.frame_time_ms - 20.0).abs() < 1e-3);
        assert!((metrics.frames_per_second - 50.0).abs() < 1e-3);
        assert_eq!(metrics.pixels_per_point, 1.5);
    }
}
