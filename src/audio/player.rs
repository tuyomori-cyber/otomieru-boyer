use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, Sample, SampleFormat, SampleRate, SizedSample, Stream, StreamConfig,
    SupportedStreamConfig,
};

use crate::model::Track;

pub struct AudioPlayer {
    runtime: Option<Arc<PlaybackRuntime>>,
    stream: Option<Stream>,
    output_info: Option<OutputStreamInfo>,
}

#[derive(Clone)]
pub struct PlayerSnapshot {
    pub transport: TransportState,
    pub position_seconds: f64,
    pub debug_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl TransportState {
    fn as_u8(self) -> u8 {
        match self {
            Self::Stopped => 0,
            Self::Playing => 1,
            Self::Paused => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Playing,
            2 => Self::Paused,
            _ => Self::Stopped,
        }
    }
}

#[derive(Debug)]
pub enum PlayerError {
    NoOutputDevice,
    DefaultConfig(cpal::DefaultStreamConfigError),
    SupportedConfigs(cpal::SupportedStreamConfigsError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no default output device found"),
            Self::DefaultConfig(error) => write!(f, "failed to get default output config: {error}"),
            Self::SupportedConfigs(error) => {
                write!(f, "failed to query supported output configs: {error}")
            }
            Self::BuildStream(error) => write!(f, "failed to build output stream: {error}"),
            Self::PlayStream(error) => write!(f, "failed to start output stream: {error}"),
        }
    }
}

impl std::error::Error for PlayerError {}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            runtime: None,
            stream: None,
            output_info: None,
        }
    }

    pub fn load_track(&mut self, track: &Track) -> Result<(), PlayerError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlayerError::NoOutputDevice)?;
        let default_config = device
            .default_output_config()
            .map_err(PlayerError::DefaultConfig)?;
        let config =
            select_output_config(&device, track, default_config).map_err(PlayerError::SupportedConfigs)?;
        let stream_config = config.config();

        let runtime = Arc::new(PlaybackRuntime::from_track(track, &stream_config));
        let runtime_for_stream = Arc::clone(&runtime);

        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                build_output_stream::<f32>(&device, &stream_config, runtime_for_stream)
            }
            SampleFormat::I16 => {
                build_output_stream::<i16>(&device, &stream_config, runtime_for_stream)
            }
            SampleFormat::U16 => {
                build_output_stream::<u16>(&device, &stream_config, runtime_for_stream)
            }
            _ => build_output_stream::<f32>(&device, &stream_config, runtime_for_stream),
        }
        .map_err(PlayerError::BuildStream)?;

        stream.play().map_err(PlayerError::PlayStream)?;
        self.output_info = Some(OutputStreamInfo::from_config(config.sample_format(), &stream_config));
        self.runtime = Some(runtime);
        self.stream = Some(stream);
        Ok(())
    }

    pub fn play(&mut self) -> Result<(), PlayerError> {
        if let Some(stream) = &self.stream {
            stream.play().map_err(PlayerError::PlayStream)?;
        }

        if let Some(runtime) = &self.runtime {
            if runtime.is_finished() {
                runtime.set_position_frames(0.0);
            }
            runtime
                .transport
                .store(TransportState::Playing.as_u8(), Ordering::Relaxed);
        }

        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime
                .transport
                .store(TransportState::Paused.as_u8(), Ordering::Relaxed);
        }
    }

    pub fn stop(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime
                .transport
                .store(TransportState::Stopped.as_u8(), Ordering::Relaxed);
            runtime.set_position_frames(0.0);
        }
    }

    pub fn seek_to_start(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime.set_position_frames(0.0);
        }
    }

    pub fn seek_to_seconds(&mut self, seconds: f64) {
        if let Some(runtime) = &self.runtime {
            runtime.seek_to_seconds(seconds);
        }
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        let output = self
            .output_info
            .as_ref()
            .map(OutputStreamInfo::summary)
            .unwrap_or_else(|| "output: not initialized".to_owned());

        let Some(runtime) = &self.runtime else {
            return PlayerSnapshot {
                transport: TransportState::Stopped,
                position_seconds: 0.0,
                debug_summary: output,
            };
        };

        PlayerSnapshot {
            transport: TransportState::from_u8(runtime.transport.load(Ordering::Relaxed)),
            position_seconds: runtime.current_position_seconds(),
            debug_summary: format!(
                "source: {} Hz / {} ch / {:.5}x step | {}",
                runtime.source_sample_rate,
                runtime.source_channels,
                runtime.step_ratio(),
                output
            ),
        }
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

struct PlaybackRuntime {
    samples: Arc<[f32]>,
    source_channels: u16,
    source_sample_rate: u32,
    output_channels: u16,
    output_sample_rate: u32,
    position_frames_bits: AtomicU64,
    transport: AtomicU8,
}

impl PlaybackRuntime {
    fn from_track(track: &Track, config: &StreamConfig) -> Self {
        Self {
            samples: Arc::from(track.samples.clone()),
            source_channels: track.channels.max(1),
            source_sample_rate: track.sample_rate.max(1),
            output_channels: config.channels.max(1),
            output_sample_rate: config.sample_rate.0.max(1),
            position_frames_bits: AtomicU64::new(0.0f64.to_bits()),
            transport: AtomicU8::new(TransportState::Stopped.as_u8()),
        }
    }

    fn total_source_frames(&self) -> usize {
        self.samples.len() / self.source_channels.max(1) as usize
    }

    fn duration_seconds(&self) -> f64 {
        self.total_source_frames() as f64 / self.source_sample_rate.max(1) as f64
    }

    fn current_position_frames(&self) -> f64 {
        f64::from_bits(self.position_frames_bits.load(Ordering::Relaxed))
    }

    fn set_position_frames(&self, frames: f64) {
        self.position_frames_bits
            .store(frames.to_bits(), Ordering::Relaxed);
    }

    fn current_position_seconds(&self) -> f64 {
        self.current_position_frames() / self.source_sample_rate.max(1) as f64
    }

    fn seek_to_seconds(&self, seconds: f64) {
        let clamped = seconds.clamp(0.0, self.duration_seconds());
        self.set_position_frames(clamped * self.source_sample_rate as f64);
    }

    fn is_finished(&self) -> bool {
        self.current_position_frames() >= self.total_source_frames() as f64
    }

    fn step_ratio(&self) -> f64 {
        self.source_sample_rate as f64 / self.output_sample_rate.max(1) as f64
    }
}

pub const UI_REPAINT_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone)]
pub struct OutputStreamInfo {
    sample_rate: u32,
    channels: u16,
    sample_format: &'static str,
}

impl OutputStreamInfo {
    fn from_config(sample_format: SampleFormat, config: &StreamConfig) -> Self {
        Self {
            sample_rate: config.sample_rate.0,
            channels: config.channels,
            sample_format: sample_format_name(sample_format),
        }
    }

    fn summary(&self) -> String {
        format!(
            "output: {} Hz / {} ch / {}",
            self.sample_rate, self.channels, self.sample_format
        )
    }
}

fn sample_format_name(sample_format: SampleFormat) -> &'static str {
    match sample_format {
        SampleFormat::I8 => "i8",
        SampleFormat::I16 => "i16",
        SampleFormat::I24 => "i24",
        SampleFormat::I32 => "i32",
        SampleFormat::I64 => "i64",
        SampleFormat::U8 => "u8",
        SampleFormat::U16 => "u16",
        SampleFormat::U32 => "u32",
        SampleFormat::U64 => "u64",
        SampleFormat::F32 => "f32",
        SampleFormat::F64 => "f64",
        _ => "unknown",
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    runtime: Arc<PlaybackRuntime>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let output_channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _| write_data(data, output_channels, &runtime),
        move |error| eprintln!("audio stream error: {error}"),
        None,
    )
}

fn write_data<T>(output: &mut [T], output_channels: usize, runtime: &Arc<PlaybackRuntime>)
where
    T: Sample + FromSample<f32>,
{
    let transport = TransportState::from_u8(runtime.transport.load(Ordering::Relaxed));
    let mut position_frames = runtime.current_position_frames();
    let total_frames = runtime.total_source_frames() as f64;

    for frame in output.chunks_mut(output_channels) {
        if transport != TransportState::Playing || total_frames == 0.0 || position_frames >= total_frames
        {
            if position_frames >= total_frames && total_frames > 0.0 {
                runtime
                    .transport
                    .store(TransportState::Stopped.as_u8(), Ordering::Relaxed);
                position_frames = total_frames;
            }

            for sample in frame {
                *sample = T::from_sample(0.0);
            }
            continue;
        }

        for (channel, sample) in frame.iter_mut().enumerate() {
            let source_channel = channel.min(runtime.source_channels.saturating_sub(1) as usize);
            let value = interpolate_sample(
                &runtime.samples,
                runtime.source_channels as usize,
                position_frames,
                source_channel,
            );
            *sample = T::from_sample(value);
        }

        position_frames += runtime.step_ratio();
    }

    runtime.set_position_frames(position_frames);
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

fn select_output_config(
    device: &cpal::Device,
    track: &Track,
    default_config: SupportedStreamConfig,
) -> Result<SupportedStreamConfig, cpal::SupportedStreamConfigsError> {
    let default_channels = default_config.channels();
    let default_format = default_config.sample_format();
    let mut best_default_channel_match: Option<SupportedStreamConfig> = None;

    for range in device.supported_output_configs()? {
        if range.channels() != default_channels || range.sample_format() != default_format {
            continue;
        }

        let exact_rate_supported = range.min_sample_rate().0 <= track.sample_rate
            && track.sample_rate <= range.max_sample_rate().0;

        if !exact_rate_supported {
            continue;
        }

        best_default_channel_match = Some(range.with_sample_rate(SampleRate(track.sample_rate)));
        break;
    }

    Ok(best_default_channel_match.unwrap_or(default_config))
}
