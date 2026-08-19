use std::fmt;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::model::Track;

pub struct AudioPlayer {
    shared: Arc<Mutex<PlaybackShared>>,
    stream: Option<Stream>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug)]
pub enum PlayerError {
    NoOutputDevice,
    DefaultConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no default output device found"),
            Self::DefaultConfig(error) => write!(f, "failed to get default output config: {error}"),
            Self::BuildStream(error) => write!(f, "failed to build output stream: {error}"),
            Self::PlayStream(error) => write!(f, "failed to start output stream: {error}"),
        }
    }
}

impl std::error::Error for PlayerError {}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(PlaybackShared::default())),
            stream: None,
        }
    }

    pub fn load_track(&mut self, track: &Track) -> Result<(), PlayerError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlayerError::NoOutputDevice)?;
        let config = device
            .default_output_config()
            .map_err(PlayerError::DefaultConfig)?;
        let stream_config: StreamConfig = config.config();

        {
            let mut shared = self.shared.lock().expect("playback mutex poisoned");
            *shared = PlaybackShared::from_track(track, &stream_config);
        }

        let shared = Arc::clone(&self.shared);
        let stream = match config.sample_format() {
            SampleFormat::F32 => build_output_stream::<f32>(&device, &stream_config, shared),
            SampleFormat::I16 => build_output_stream::<i16>(&device, &stream_config, shared),
            SampleFormat::U16 => build_output_stream::<u16>(&device, &stream_config, shared),
            _ => build_output_stream::<f32>(&device, &stream_config, shared),
        }
        .map_err(PlayerError::BuildStream)?;

        stream.play().map_err(PlayerError::PlayStream)?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn play(&mut self) -> Result<(), PlayerError> {
        if let Some(stream) = &self.stream {
            stream.play().map_err(PlayerError::PlayStream)?;
        }
        let mut shared = self.shared.lock().expect("playback mutex poisoned");
        if shared.is_finished() {
            shared.position_frames = 0.0;
        }
        shared.transport = TransportState::Playing;
        Ok(())
    }

    pub fn pause(&mut self) {
        let mut shared = self.shared.lock().expect("playback mutex poisoned");
        shared.transport = TransportState::Paused;
    }

    pub fn stop(&mut self) {
        let mut shared = self.shared.lock().expect("playback mutex poisoned");
        shared.transport = TransportState::Stopped;
        shared.position_frames = 0.0;
    }

    pub fn seek_to_start(&mut self) {
        let mut shared = self.shared.lock().expect("playback mutex poisoned");
        shared.position_frames = 0.0;
    }

    pub fn transport_state(&self) -> TransportState {
        self.shared
            .lock()
            .expect("playback mutex poisoned")
            .transport
    }

    pub fn current_position_seconds(&self) -> f64 {
        self.shared
            .lock()
            .expect("playback mutex poisoned")
            .current_position_seconds()
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct PlaybackShared {
    samples: Vec<f32>,
    source_channels: u16,
    source_sample_rate: u32,
    output_channels: u16,
    output_sample_rate: u32,
    position_frames: f64,
    transport: TransportState,
}

impl PlaybackShared {
    fn from_track(track: &Track, config: &StreamConfig) -> Self {
        Self {
            samples: track.samples.clone(),
            source_channels: track.channels.max(1),
            source_sample_rate: track.sample_rate.max(1),
            output_channels: config.channels.max(1),
            output_sample_rate: config.sample_rate.0.max(1),
            position_frames: 0.0,
            transport: TransportState::Stopped,
        }
    }

    fn current_position_seconds(&self) -> f64 {
        if self.source_sample_rate == 0 {
            0.0
        } else {
            self.position_frames / self.source_sample_rate as f64
        }
    }

    fn total_source_frames(&self) -> usize {
        self.samples.len() / self.source_channels.max(1) as usize
    }

    fn is_finished(&self) -> bool {
        self.position_frames >= self.total_source_frames() as f64
    }

    fn next_value(&mut self, output_channel: usize) -> f32 {
        if self.transport != TransportState::Playing {
            return 0.0;
        }

        let total_frames = self.total_source_frames() as f64;
        if total_frames == 0.0 || self.position_frames >= total_frames {
            self.transport = TransportState::Stopped;
            self.position_frames = total_frames;
            return 0.0;
        }

        let source_channel = output_channel.min(self.source_channels.saturating_sub(1) as usize);
        let value = interpolate_sample(
            &self.samples,
            self.source_channels as usize,
            self.position_frames,
            source_channel,
        );

        if output_channel + 1 == self.output_channels as usize {
            let ratio = self.source_sample_rate as f64 / self.output_sample_rate as f64;
            self.position_frames += ratio;
        }

        value
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<Mutex<PlaybackShared>>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let output_channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _| write_data(data, output_channels, &shared),
        move |error| eprintln!("audio stream error: {error}"),
        None,
    )
}

fn write_data<T>(output: &mut [T], output_channels: usize, shared: &Arc<Mutex<PlaybackShared>>)
where
    T: Sample + FromSample<f32>,
{
    let mut shared = shared.lock().expect("playback mutex poisoned");

    for frame in output.chunks_mut(output_channels) {
        for (channel, sample) in frame.iter_mut().enumerate() {
            let value = shared.next_value(channel);
            *sample = T::from_sample(value);
        }
    }
}

fn interpolate_sample(
    samples: &[f32],
    channels: usize,
    position_frames: f64,
    channel: usize,
) -> f32 {
    let base_frame = position_frames.floor() as usize;
    let next_frame = base_frame.saturating_add(1);
    let frac = (position_frames - base_frame as f64) as f32;

    let current = sample_at(samples, channels, base_frame, channel);
    let next = sample_at(samples, channels, next_frame, channel);

    current + (next - current) * frac
}

fn sample_at(samples: &[f32], channels: usize, frame: usize, channel: usize) -> f32 {
    let index = frame
        .saturating_mul(channels)
        .saturating_add(channel.min(channels.saturating_sub(1)));

    samples.get(index).copied().unwrap_or(0.0)
}
