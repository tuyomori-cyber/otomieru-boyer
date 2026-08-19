use std::path::{Path, PathBuf};

use crate::model::{PlaybackState, Selection, Track};

const MIN_SPECTROGRAM_PAGES: usize = 8;
const MAX_SPECTROGRAM_PAGES: usize = 48;
const PAGES_PER_MINUTE: f64 = 6.0;

#[derive(Debug, Clone, Copy)]
pub struct SpectrogramPaging {
    pub page_count: usize,
    pub page_duration_seconds: f64,
}

pub struct AppState {
    pub track: Option<Track>,
    pub playback: PlaybackState,
    pub selection: Selection,
    pub loaded_file_path: Option<PathBuf>,
    pub status_text: String,
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
        self.selection = Selection::default();
        self.status_text.clear();
    }

    pub fn spectrogram_paging(&self) -> SpectrogramPaging {
        let duration_seconds = self
            .track
            .as_ref()
            .map(|track| track.duration_seconds)
            .unwrap_or(0.0);

        spectrogram_paging_for_duration(duration_seconds)
    }

    pub fn current_page_start_seconds(&self) -> f64 {
        let paging = self.spectrogram_paging();
        let page_index =
            (self.playback.position_seconds / paging.page_duration_seconds.max(0.001)).floor();
        page_index.max(0.0) * paging.page_duration_seconds
    }

    pub fn current_page_end_seconds(&self) -> f64 {
        let paging = self.spectrogram_paging();
        let duration = self
            .track
            .as_ref()
            .map(|track| track.duration_seconds)
            .unwrap_or(0.0);

        (self.current_page_start_seconds() + paging.page_duration_seconds).min(duration)
    }

    pub fn current_page_index(&self) -> usize {
        let paging = self.spectrogram_paging();
        let raw_index =
            (self.playback.position_seconds / paging.page_duration_seconds.max(0.001)).floor()
                as usize;
        raw_index.min(paging.page_count.saturating_sub(1))
    }
}

pub fn spectrogram_paging_for_duration(duration_seconds: f64) -> SpectrogramPaging {
    let duration_minutes = (duration_seconds / 60.0).max(0.0);
    let page_count = (duration_minutes * PAGES_PER_MINUTE).ceil() as usize;
    let page_count = page_count.clamp(MIN_SPECTROGRAM_PAGES, MAX_SPECTROGRAM_PAGES);
    let page_duration_seconds = if page_count == 0 {
        1.0
    } else {
        duration_seconds.max(1.0) / page_count as f64
    };

    SpectrogramPaging {
        page_count,
        page_duration_seconds,
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            track: None,
            playback: PlaybackState::default(),
            selection: Selection::default(),
            loaded_file_path: None,
            status_text: String::new(),
        }
    }
}
