use std::path::{Path, PathBuf};

use crate::audio::preview_tone::PreviewTimbre;
use crate::model::{PlaybackState, Selection, Track};

const MIN_VIEW_SEGMENTS: usize = 8;
const MAX_VIEW_SEGMENTS: usize = 48;
const VIEW_SEGMENTS_PER_MINUTE: f64 = 6.0;
const MIN_VIEW_DURATION_SECONDS: f64 = 0.25;
const MAX_VIEW_ZOOM: f64 = 64.0;
const MAX_PITCH_ZOOM: f64 = 16.0;

pub const PITCH_CLASS_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalePreset {
    Major,
    Minor,
    Pentatonic,
    Chromatic,
}

impl ScalePreset {
    pub const ALL: [Self; 4] = [Self::Major, Self::Minor, Self::Pentatonic, Self::Chromatic];

    pub fn label(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Pentatonic => "Pentatonic",
            Self::Chromatic => "Chromatic",
        }
    }

    fn intervals(self) -> &'static [usize] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Self::Pentatonic => &[0, 2, 4, 7, 9],
            Self::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpectrogramView {
    pub duration_seconds: f64,
    pub total_segments: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PitchView {
    pub min_midi_note: usize,
    pub max_midi_note: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct UiFrameMetrics {
    pub frames_per_second: f32,
    pub frame_time_ms: f32,
    pub pixels_per_point: f32,
}

impl Default for UiFrameMetrics {
    fn default() -> Self {
        Self {
            frames_per_second: 0.0,
            frame_time_ms: 0.0,
            pixels_per_point: 1.0,
        }
    }
}

impl PitchView {
    pub fn pitch_count(self) -> usize {
        self.max_midi_note
            .saturating_sub(self.min_midi_note)
            .saturating_add(1)
    }
}

pub struct AppState {
    pub track: Option<Track>,
    pub playback: PlaybackState,
    pub ui_frame_metrics: UiFrameMetrics,
    /// 音声コールバックの位置をUI用に時間補間した再生位置。
    pub display_playhead_position_seconds: f64,
    pub view_start_seconds: f64,
    pub view_zoom: f64,
    pub pitch_view_center_midi: f64,
    pub pitch_zoom: f64,
    pub selection: Selection,
    pub loaded_file_path: Option<PathBuf>,
    pub status_text: String,
    pub spectrogram_gain_db: f32,
    /// Raw表示から基音候補表示へ寄せる割合。0 はRaw表示、100 は最大強調。
    pub fundamental_emphasis: f32,
    pub scale_root: usize,
    pub scale_preset: ScalePreset,
    pub emphasized_pitch_classes: [bool; 12],
    /// 指定外の音程を表示上で減衰する割合。0 は減衰なし、100 は非表示。
    pub unemphasized_pitch_attenuation: f32,
    /// スペクトログラムを押下している間に試聴している音。MIDI番号をUI間で共有する。
    pub preview_midi_note: Option<u8>,
    /// スペクトログラム押下中の試聴音の出力振幅。
    pub preview_tone_amplitude: f32,
    /// 試聴音のA4基準周波数。スペクトログラムの解析・表示は変更しない。
    pub preview_reference_a4_hz: f32,
    pub preview_timbre: PreviewTimbre,
    pub settings_popup_open: bool,
}

impl AppState {
    pub fn status_message(&self) -> String {
        if let Some(track) = &self.track {
            let file_name = self
                .loaded_file_path
                .as_deref()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("読み込み済み音源");

            format!(
                "{file_name} | {:.2} sec | {} Hz | {} ch | samples: {}",
                track.duration_seconds,
                track.sample_rate,
                track.channels,
                track.samples.len()
            )
        } else if !self.status_text.is_empty() {
            self.status_text.clone()
        } else {
            "Open から音源を読み込んでください。".to_owned()
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status_text = status.into();
    }

    pub fn apply_scale_preset(&mut self) {
        self.emphasized_pitch_classes = [false; 12];
        for interval in self.scale_preset.intervals() {
            self.emphasized_pitch_classes[(self.scale_root + interval) % 12] = true;
        }
    }

    pub fn set_loaded_track(&mut self, path: PathBuf, track: Track) {
        self.loaded_file_path = Some(path);
        self.track = Some(track);
        self.playback = PlaybackState::default();
        self.display_playhead_position_seconds = 0.0;
        self.view_start_seconds = 0.0;
        self.view_zoom = 1.0;
        self.pitch_view_center_midi = pitch_midpoint(&self.track);
        self.pitch_zoom = 1.0;
        self.selection = Selection::default();
        self.status_text.clear();
    }

    pub fn spectrogram_view(&self) -> SpectrogramView {
        let duration_seconds = self
            .track
            .as_ref()
            .map(|track| track.duration_seconds)
            .unwrap_or(0.0);

        spectrogram_view_for_duration(duration_seconds)
    }

    pub fn view_duration_seconds(&self) -> f64 {
        let track_duration = self
            .track
            .as_ref()
            .map(|track| track.duration_seconds)
            .unwrap_or(0.0);
        (self.spectrogram_view().duration_seconds / self.view_zoom).clamp(
            MIN_VIEW_DURATION_SECONDS,
            track_duration.max(MIN_VIEW_DURATION_SECONDS),
        )
    }

    pub fn current_view_start_seconds(&self) -> f64 {
        self.clamped_view_start_seconds(self.view_start_seconds)
    }

    pub fn current_view_end_seconds(&self) -> f64 {
        let duration = self
            .track
            .as_ref()
            .map(|track| track.duration_seconds)
            .unwrap_or(0.0);

        (self.current_view_start_seconds() + self.view_duration_seconds()).min(duration)
    }

    pub fn set_view_start_seconds(&mut self, seconds: f64) {
        self.view_start_seconds = self.clamped_view_start_seconds(seconds);
    }

    pub fn reset_view_to_start(&mut self) {
        self.view_start_seconds = 0.0;
    }

    pub fn zoom_view_at(&mut self, anchor_seconds: f64, factor: f64) {
        let old_duration = self.view_duration_seconds();
        self.view_zoom = (self.view_zoom * factor).clamp(1.0, MAX_VIEW_ZOOM);
        let new_duration = self.view_duration_seconds();
        let anchor_ratio =
            ((anchor_seconds - self.current_view_start_seconds()) / old_duration).clamp(0.0, 1.0);
        self.set_view_start_seconds(anchor_seconds - new_duration * anchor_ratio);
    }

    pub fn pitch_view(&self) -> PitchView {
        let (minimum, maximum) = pitch_bounds(&self.track);
        let total = maximum.saturating_sub(minimum).saturating_add(1);
        let visible = ((total as f64 / self.pitch_zoom).ceil() as usize).clamp(1, total);
        let maximum_start = maximum.saturating_add(1).saturating_sub(visible);
        let start = (self.pitch_view_center_midi - visible as f64 / 2.0)
            .floor()
            .clamp(minimum as f64, maximum_start as f64) as usize;
        PitchView {
            min_midi_note: start,
            max_midi_note: start + visible - 1,
        }
    }

    pub fn zoom_pitch_at(&mut self, anchor_midi: f64, factor: f64) {
        let old_view = self.pitch_view();
        let old_count = old_view.pitch_count() as f64;
        let anchor_ratio =
            ((anchor_midi - old_view.min_midi_note as f64) / old_count).clamp(0.0, 1.0);
        let (minimum, maximum) = pitch_bounds(&self.track);
        let total = maximum.saturating_sub(minimum).saturating_add(1) as f64;
        self.pitch_zoom = (self.pitch_zoom * factor).clamp(1.0, MAX_PITCH_ZOOM.min(total));
        let new_count = self.pitch_view().pitch_count() as f64;
        self.pitch_view_center_midi = anchor_midi - new_count * anchor_ratio + new_count / 2.0;
        self.pitch_view_center_midi = self
            .pitch_view_center_midi
            .clamp(minimum as f64, maximum as f64);
    }

    pub fn advance_view_by_window(&mut self) {
        let next_start = self.current_view_start_seconds() + self.view_duration_seconds();
        self.view_start_seconds = self.clamped_view_start_seconds(next_start);
    }

    pub fn follow_playhead_if_needed(&mut self, previous_position_seconds: f64) {
        if !self.playback.playing {
            return;
        }

        let view_start = self.current_view_start_seconds();
        let view_end = self.current_view_end_seconds();
        let current_position = self.playback.position_seconds;
        let epsilon = 1e-6;

        let was_visible = previous_position_seconds >= view_start - epsilon
            && previous_position_seconds <= view_end + epsilon;
        let crossed_right_edge = previous_position_seconds < view_end - epsilon
            && current_position >= view_end - epsilon;

        if was_visible && crossed_right_edge {
            self.advance_view_by_window();
        }
    }

    fn clamped_view_start_seconds(&self, seconds: f64) -> f64 {
        let duration = self
            .track
            .as_ref()
            .map(|track| track.duration_seconds)
            .unwrap_or(0.0);
        let max_start = (duration - self.view_duration_seconds()).max(0.0);
        seconds.clamp(0.0, max_start)
    }
}

pub fn spectrogram_view_for_duration(duration_seconds: f64) -> SpectrogramView {
    let duration_minutes = (duration_seconds / 60.0).max(0.0);
    let total_segments = (duration_minutes * VIEW_SEGMENTS_PER_MINUTE).ceil() as usize;
    let total_segments = total_segments.clamp(MIN_VIEW_SEGMENTS, MAX_VIEW_SEGMENTS);
    let duration_seconds = if total_segments == 0 {
        1.0
    } else {
        duration_seconds.max(1.0) / total_segments as f64
    };

    SpectrogramView {
        duration_seconds,
        total_segments,
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            track: None,
            playback: PlaybackState::default(),
            ui_frame_metrics: UiFrameMetrics::default(),
            display_playhead_position_seconds: 0.0,
            view_start_seconds: 0.0,
            view_zoom: 1.0,
            pitch_view_center_midi: pitch_midpoint(&None),
            pitch_zoom: 1.0,
            selection: Selection::default(),
            loaded_file_path: None,
            status_text: String::new(),
            spectrogram_gain_db: 0.0,
            fundamental_emphasis: 0.0,
            scale_root: 0,
            scale_preset: ScalePreset::Major,
            emphasized_pitch_classes: [
                true, false, true, false, true, true, false, true, false, true, false, true,
            ],
            unemphasized_pitch_attenuation: 0.0,
            preview_midi_note: None,
            preview_tone_amplitude: 0.16,
            preview_reference_a4_hz: 440.0,
            preview_timbre: PreviewTimbre::Piano,
            settings_popup_open: false,
        }
    }
}

fn pitch_bounds(track: &Option<Track>) -> (usize, usize) {
    track
        .as_ref()
        .and_then(|track| track.spectrogram.as_ref())
        .map(|spectrogram| (spectrogram.min_midi_note, spectrogram.max_midi_note))
        .unwrap_or((
            crate::analysis::spectrum::MIN_MIDI_NOTE,
            crate::analysis::spectrum::MAX_MIDI_NOTE,
        ))
}

fn pitch_midpoint(track: &Option<Track>) -> f64 {
    let (minimum, maximum) = pitch_bounds(track);
    (minimum + maximum) as f64 / 2.0
}

#[cfg(test)]
mod tests {
    use super::{AppState, MIN_VIEW_DURATION_SECONDS, ScalePreset};
    use crate::model::Track;

    #[test]
    fn zoom_keeps_the_anchor_time_in_the_same_relative_position() {
        let mut state = AppState {
            track: Some(Track {
                duration_seconds: 120.0,
                ..Track::default()
            }),
            ..AppState::default()
        };
        let old_duration = state.view_duration_seconds();
        let anchor = old_duration * 0.75;

        state.zoom_view_at(anchor, 2.0);

        let relative =
            (anchor - state.current_view_start_seconds()) / state.view_duration_seconds();
        assert!((relative - 0.75).abs() < 1e-6);
        assert!(state.view_duration_seconds() >= MIN_VIEW_DURATION_SECONDS);
    }

    #[test]
    fn applying_a_scale_preset_updates_the_pitch_classes() {
        let mut state = AppState {
            scale_root: 2,
            scale_preset: ScalePreset::Major,
            ..AppState::default()
        };

        state.apply_scale_preset();

        assert_eq!(
            state.emphasized_pitch_classes,
            [
                false, true, true, false, true, false, true, true, false, true, false, true
            ]
        );
    }

    #[test]
    fn resetting_the_view_returns_the_spectrogram_to_the_start() {
        let mut state = AppState {
            track: Some(Track {
                duration_seconds: 120.0,
                ..Track::default()
            }),
            view_start_seconds: 60.0,
            ..AppState::default()
        };

        state.reset_view_to_start();

        assert_eq!(state.current_view_start_seconds(), 0.0);
    }
}
