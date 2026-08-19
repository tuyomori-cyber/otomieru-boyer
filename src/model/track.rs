use crate::analysis::spectrum::SpectrogramData;

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub sample_rate: u32,
    pub duration_seconds: f64,
    pub channels: u16,
    pub samples: Vec<f32>,
    pub spectrogram: Option<SpectrogramData>,
}
