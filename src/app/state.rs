use std::path::{Path, PathBuf};

use crate::model::{PlaybackState, Selection, Track};

const MIN_VIEW_SEGMENTS: usize = 8;
const MAX_VIEW_SEGMENTS: usize = 48;
const VIEW_SEGMENTS_PER_MINUTE: f64 = 6.0;

#[derive(Debug, Clone, Copy)]
pub struct SpectrogramView {
    pub duration_seconds: f64,
    pub total_segments: usize,
}

pub struct AppState {
    pub track: Option<Track>,
    pub playback: PlaybackState,
    pub view_start_seconds: f64,
    pub selection: Selection,
    pub loaded_file_path: Option<PathBuf>,
    pub status_text: String,
    pub spectrogram_gain_db: f32,
    pub preview_tone_active: bool,
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
            "Open から音源を読み込む MVP の土台です。次にデコードと再生を実装します。"
                .to_owned()
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status_text = status.into();
    }

    pub fn set_loaded_track(&mut self, path: PathBuf, track: Track) {
        self.loaded_file_path = Some(path);
        self.track = Some(track);
        self.playback = PlaybackState::default();
        self.view_start_seconds = 0.0;
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
        self.spectrogram_view().duration_seconds
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
            view_start_seconds: 0.0,
            selection: Selection::default(),
            loaded_file_path: None,
            status_text: String::new(),
            spectrogram_gain_db: 0.0,
            preview_tone_active: false,
        }
    }
}
