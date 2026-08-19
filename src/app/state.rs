use crate::model::{PlaybackState, Selection, Track};

pub struct AppState {
    pub track: Option<Track>,
    pub playback: PlaybackState,
    pub selection: Selection,
}

impl AppState {
    pub fn status_message(&self) -> &'static str {
        if self.track.is_some() {
            "音源読み込み後、ここに再生状態と解析状況を表示します。"
        } else {
            "Open から音源を読み込む MVP の土台です。次にデコードと再生を実装します。"
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            track: None,
            playback: PlaybackState::default(),
            selection: Selection::default(),
        }
    }
}
