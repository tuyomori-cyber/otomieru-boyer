use std::sync::Arc;

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

#[derive(Debug, Clone, Copy)]
pub struct StftSettings {
    pub window_size: usize,
    pub hop_size: usize,
}

impl Default for StftSettings {
    fn default() -> Self {
        Self {
            window_size: 4096,
            hop_size: 512,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StftResult {
    pub sample_rate: u32,
    pub window_size: usize,
    pub hop_size: usize,
    pub frames: Vec<Vec<f32>>,
}

pub fn compute_stft(
    interleaved_samples: &[f32],
    channels: u16,
    sample_rate: u32,
    settings: StftSettings,
) -> StftResult {
    let mono = downmix_to_mono(interleaved_samples, channels.max(1) as usize);
    if mono.len() < settings.window_size || sample_rate == 0 {
        return StftResult {
            sample_rate,
            window_size: settings.window_size,
            hop_size: settings.hop_size,
            frames: Vec::new(),
        };
    }

    let window = hann_window(settings.window_size);
    let fft = Arc::new(FftPlanner::<f32>::new().plan_fft_forward(settings.window_size));
    let mut frames = Vec::new();
    let mut buffer = vec![Complex32::default(); settings.window_size];

    for start in (0..=(mono.len() - settings.window_size)).step_by(settings.hop_size) {
        for i in 0..settings.window_size {
            buffer[i] = Complex32::new(mono[start + i] * window[i], 0.0);
        }

        fft.process(&mut buffer);

        let mut magnitudes = Vec::with_capacity(settings.window_size / 2);
        for bin in buffer.iter().take(settings.window_size / 2) {
            magnitudes.push(bin.norm_sqr().sqrt());
        }
        frames.push(magnitudes);
    }

    StftResult {
        sample_rate,
        window_size: settings.window_size,
        hop_size: settings.hop_size,
        frames,
    }
}

fn downmix_to_mono(interleaved_samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved_samples.to_vec();
    }

    interleaved_samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
        .collect()
}

fn hann_window(window_size: usize) -> Vec<f32> {
    let denom = (window_size.saturating_sub(1)).max(1) as f32;
    (0..window_size)
        .map(|i| {
            let phase = 2.0 * std::f32::consts::PI * i as f32 / denom;
            0.5 - 0.5 * phase.cos()
        })
        .collect()
}
