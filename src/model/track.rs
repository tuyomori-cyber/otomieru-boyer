use crate::analysis::spectrum::build_spectrogram;
use crate::analysis::spectrum::SpectrogramData;
use crate::analysis::stft::{compute_stft, StftSettings};
use crate::audio::decoder::DecodedAudio;

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub sample_rate: u32,
    pub duration_seconds: f64,
    pub channels: u16,
    pub samples: Vec<f32>,
    pub spectrogram: Option<SpectrogramData>,
}

impl Track {
    pub fn from_decoded(decoded: DecodedAudio) -> Self {
        let stft = compute_stft(
            &decoded.samples,
            decoded.channels,
            decoded.sample_rate,
            StftSettings::default(),
        );
        let spectrogram = build_spectrogram(&stft);

        Self {
            sample_rate: decoded.sample_rate,
            duration_seconds: decoded.duration_seconds(),
            channels: decoded.channels,
            samples: decoded.samples,
            spectrogram: Some(spectrogram),
        }
    }
}
