use crate::model::{
    ComparisonPhase, DEFAULT_COMPARISON_SEQUENCE, PlaybackDspSettings, comparison_phase_at,
};

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub position_seconds: f64,
    pub playing: bool,
    pub dsp: PlaybackDspSettings,
    pub loop_enabled: bool,
    pub comparison_enabled: bool,
    pub comparison_sequence_index: usize,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position_seconds: 0.0,
            playing: false,
            dsp: PlaybackDspSettings::default(),
            loop_enabled: false,
            comparison_enabled: false,
            comparison_sequence_index: 0,
        }
    }
}

impl PlaybackState {
    pub fn comparison_phase(&self) -> Option<ComparisonPhase> {
        self.comparison_enabled.then(|| {
            comparison_phase_at(DEFAULT_COMPARISON_SEQUENCE, self.comparison_sequence_index)
                .expect("default comparison sequence is not empty")
        })
    }
}
