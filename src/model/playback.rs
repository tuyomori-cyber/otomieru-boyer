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
    pub comparison_sequence: Vec<ComparisonPhase>,
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
            comparison_sequence: DEFAULT_COMPARISON_SEQUENCE.to_vec(),
            comparison_sequence_index: 0,
        }
    }
}

impl PlaybackState {
    pub fn comparison_phase(&self) -> Option<ComparisonPhase> {
        self.comparison_enabled.then(|| {
            comparison_phase_at(&self.comparison_sequence, self.comparison_sequence_index)
                .expect("comparison sequence is not empty")
        })
    }
}
