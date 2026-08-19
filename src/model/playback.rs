#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub position_seconds: f64,
    pub playing: bool,
    pub speed: f32,
    pub loop_enabled: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position_seconds: 0.0,
            playing: false,
            speed: 1.0,
            loop_enabled: false,
        }
    }
}
