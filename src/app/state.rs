use std::path::{Path, PathBuf};

use crate::model::{PlaybackState, Selection, Track};

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
