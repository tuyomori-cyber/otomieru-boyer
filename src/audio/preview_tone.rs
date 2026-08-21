use std::f32::consts::TAU;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::analysis::pitch_map::midi_to_frequency;

#[derive(Debug, Clone, Copy)]
pub struct PreviewToneRequest {
    pub midi_note: u8,
}

pub struct PreviewTonePlayer {
    state: Arc<PreviewToneState>,
    _stream: Stream,
}

struct PreviewToneState {
    active: AtomicBool,
    frequency_bits: AtomicU32,
}

#[derive(Debug)]
pub enum PreviewToneError {
    NoOutputDevice,
    DefaultConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
}

impl fmt::Display for PreviewToneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no default output device found"),
            Self::DefaultConfig(error) => write!(f, "failed to get default output config: {error}"),
            Self::BuildStream(error) => write!(f, "failed to build preview stream: {error}"),
            Self::PlayStream(error) => write!(f, "failed to start preview stream: {error}"),
        }
    }
}

impl std::error::Error for PreviewToneError {}

impl PreviewTonePlayer {
    pub fn new() -> Result<Self, PreviewToneError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PreviewToneError::NoOutputDevice)?;
        let config = device
            .default_output_config()
            .map_err(PreviewToneError::DefaultConfig)?;
        let stream_config: StreamConfig = config.config();
        let state = Arc::new(PreviewToneState {
            active: AtomicBool::new(false),
            frequency_bits: AtomicU32::new(440.0f32.to_bits()),
        });
        let stream_state = Arc::clone(&state);

        let stream = match config.sample_format() {
            SampleFormat::F32 => build_preview_stream::<f32>(&device, &stream_config, stream_state),
            SampleFormat::I16 => build_preview_stream::<i16>(&device, &stream_config, stream_state),
            SampleFormat::U16 => build_preview_stream::<u16>(&device, &stream_config, stream_state),
            _ => build_preview_stream::<f32>(&device, &stream_config, stream_state),
        }
        .map_err(PreviewToneError::BuildStream)?;

        stream.play().map_err(PreviewToneError::PlayStream)?;

        Ok(Self {
            state,
            _stream: stream,
        })
    }

    pub fn update_preview(&self, request: PreviewToneRequest) {
        let frequency = midi_to_frequency(request.midi_note as f32);
        self.state
            .frequency_bits
            .store(frequency.to_bits(), Ordering::Relaxed);
        self.state.active.store(true, Ordering::Relaxed);
    }

    pub fn stop_preview(&self) {
        self.state.active.store(false, Ordering::Relaxed);
    }
}

fn build_preview_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    state: Arc<PreviewToneState>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;
    let mut phase = 0.0_f32;

    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            for frame in data.chunks_mut(channels) {
                let active = state.active.load(Ordering::Relaxed);
                let value = if active {
                    let frequency = f32::from_bits(state.frequency_bits.load(Ordering::Relaxed));
                    let sample = phase.sin() * 0.16;
                    phase = (phase + TAU * frequency / sample_rate) % TAU;
                    sample
                } else {
                    0.0
                };

                for out in frame {
                    *out = T::from_sample(value);
                }
            }
        },
        move |error| eprintln!("preview stream error: {error}"),
        None,
    )
}
