use crate::model::PlaybackDspSettings;

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub position_seconds: f64,
    pub playing: bool,
    pub dsp: PlaybackDspSettings,
    pub loop_enabled: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position_seconds: 0.0,
            playing: false,
            dsp: PlaybackDspSettings::default(),
            loop_enabled: false,
        }
    }
}
