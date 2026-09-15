use crate::analysis::spectrum::SpectrogramData;
use crate::analysis::spectrum::build_spectrogram;
use crate::analysis::stft::{StftSettings, compute_stft};
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
        let mut track = Self::from_decoded_without_spectrogram(decoded);
        track.rebuild_spectrogram();
        track
    }

    pub fn from_decoded_without_spectrogram(decoded: DecodedAudio) -> Self {
        Self {
            sample_rate: decoded.sample_rate,
            duration_seconds: decoded.duration_seconds(),
            channels: decoded.channels,
            samples: decoded.samples,
            spectrogram: None,
        }
    }

    pub fn rebuild_spectrogram(&mut self) {
        let stft = compute_stft(
            &self.samples,
            self.channels,
            self.sample_rate,
            StftSettings::default(),
        );
        self.spectrogram = Some(build_spectrogram(&stft));
    }
}
