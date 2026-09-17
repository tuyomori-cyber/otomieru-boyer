use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, Sample, SampleFormat, SampleRate, SizedSample, Stream, StreamConfig,
    SupportedStreamConfig,
};

use crate::audio::comparison::{ComparisonAudioControl, ComparisonAudioSnapshot};
use crate::audio::dsp_engine::DspEngine;
use crate::audio::spectral_freeze::SpectralFreeze;
use crate::audio::timestretch::DspTransportEvent;
use crate::model::{ComparisonPhase, PlaybackDspSettings, Track};

pub struct AudioPlayer {
    runtime: Option<Arc<PlaybackRuntime>>,
    stream: Option<Stream>,
    comparison_control: Arc<ComparisonAudioControl>,
}

#[derive(Clone)]
pub struct PlayerSnapshot {
    pub transport: TransportState,
    pub position_seconds: f64,
    pub transport_generation: u64,
    pub comparison: ComparisonAudioSnapshot,
    pub the_world_active: bool,
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
            comparison_control: Arc::new(ComparisonAudioControl::default()),
        }
    }

    pub fn comparison_control(&self) -> Arc<ComparisonAudioControl> {
        Arc::clone(&self.comparison_control)
    }

    pub fn load_track(&mut self, track: &Track) -> Result<(), PlayerError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlayerError::NoOutputDevice)?;
        let default_config = device
            .default_output_config()
            .map_err(PlayerError::DefaultConfig)?;
        let config = select_output_config(&device, track, default_config)
            .map_err(PlayerError::SupportedConfigs)?;
        let stream_config = config.config();

        let runtime = Arc::new(PlaybackRuntime::from_track(
            track,
            &stream_config,
            Arc::clone(&self.comparison_control),
        ));
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
                runtime.transport_generation.fetch_add(1, Ordering::Relaxed);
            }
            runtime
                .dsp_engine
                .notify_transport_event(DspTransportEvent::Start);
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
            runtime.spectral_freeze.stop();
            runtime
                .transport
                .store(TransportState::Stopped.as_u8(), Ordering::Relaxed);
            runtime.set_position_frames(0.0);
            runtime.transport_generation.fetch_add(1, Ordering::Relaxed);
            runtime
                .dsp_engine
                .notify_transport_event(DspTransportEvent::Stop);
        }
    }

    pub fn seek_to_start(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime.set_position_frames(0.0);
            runtime.transport_generation.fetch_add(1, Ordering::Relaxed);
            runtime
                .dsp_engine
                .notify_transport_event(DspTransportEvent::Seek {
                    position_seconds: 0.0,
                });
        }
    }

    pub fn seek_to_seconds(&mut self, seconds: f64) {
        if let Some(runtime) = &self.runtime {
            runtime.seek_to_seconds(seconds);
            runtime.transport_generation.fetch_add(1, Ordering::Relaxed);
            runtime
                .dsp_engine
                .notify_transport_event(DspTransportEvent::Seek {
                    position_seconds: seconds,
                });
        }
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        let Some(runtime) = &self.runtime else {
            return PlayerSnapshot {
                transport: TransportState::Stopped,
                position_seconds: 0.0,
                transport_generation: 0,
                comparison: self.comparison_control.snapshot(),
                the_world_active: false,
            };
        };

        PlayerSnapshot {
            transport: TransportState::from_u8(runtime.transport.load(Ordering::Relaxed)),
            position_seconds: runtime.current_position_seconds(),
            transport_generation: runtime.transport_generation.load(Ordering::Relaxed),
            comparison: self.comparison_control.snapshot(),
            the_world_active: runtime.spectral_freeze.is_active(),
        }
    }

    pub fn toggle_the_world(&self) -> bool {
        let Some(runtime) = &self.runtime else {
            return false;
        };
        if runtime.spectral_freeze.is_active() {
            runtime.spectral_freeze.begin_deactivation();
            return false;
        }
        if runtime.transport.load(Ordering::Relaxed) != TransportState::Playing.as_u8() {
            return false;
        }
        let position = runtime.current_position_frames();
        runtime
            .spectral_freeze
            .activate(position, |frame, channel| {
                runtime
                    .dsp_engine
                    .render_source_sample(frame as f64, channel)
            });
        true
    }

    pub fn enable_comparison_from_start(&self) {
        self.comparison_control.enable_from_start();
    }

    pub fn disable_comparison(&self) {
        self.comparison_control.disable();
    }

    pub fn reset_comparison_to_start(&self) {
        self.comparison_control.reset_to_start_if_enabled();
    }

    pub fn set_comparison_sequence(&self, sequence: &[ComparisonPhase]) {
        self.comparison_control.set_sequence(sequence);
    }

    pub fn set_loop_enabled(&mut self, enabled: bool) {
        if let Some(runtime) = &self.runtime {
            runtime.loop_enabled.store(enabled, Ordering::Relaxed);
        }
    }

    pub fn set_loop_range(&mut self, loop_range: Option<(f64, f64)>) {
        if let Some(runtime) = &self.runtime {
            if let Some((start, end)) = loop_range {
                let start_frames = (start * runtime.dsp_engine.source_sample_rate() as f64)
                    .clamp(0.0, runtime.total_source_frames() as f64);
                let end_frames = (end * runtime.dsp_engine.source_sample_rate() as f64)
                    .clamp(0.0, runtime.total_source_frames() as f64);
                runtime
                    .loop_start_frames_bits
                    .store(start_frames.to_bits(), Ordering::Relaxed);
                runtime
                    .loop_end_frames_bits
                    .store(end_frames.to_bits(), Ordering::Relaxed);

                if runtime.loop_enabled.load(Ordering::Relaxed) {
                    let position = runtime.current_position_frames();
                    if position < start_frames || position >= end_frames {
                        runtime.set_position_frames(start_frames);
                        runtime.transport_generation.fetch_add(1, Ordering::Relaxed);
                        runtime
                            .dsp_engine
                            .notify_transport_event(DspTransportEvent::LoopJump {
                                start_seconds: start,
                                end_seconds: end,
                            });
                    }
                }
            } else {
                runtime
                    .loop_start_frames_bits
                    .store(0.0f64.to_bits(), Ordering::Relaxed);
                runtime
                    .loop_end_frames_bits
                    .store(0.0f64.to_bits(), Ordering::Relaxed);
            }
        }
    }

    pub fn set_dsp_settings(&mut self, settings: PlaybackDspSettings) {
        if let Some(runtime) = &self.runtime {
            runtime.set_dsp_settings(settings);
        }
    }

    pub fn set_source_volume(&self, volume: f32) {
        if let Some(runtime) = &self.runtime {
            runtime
                .source_volume_bits
                .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    pub fn set_source_muted(&self, muted: bool) {
        if let Some(runtime) = &self.runtime {
            runtime.source_muted.store(muted, Ordering::Relaxed);
        }
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

struct PlaybackRuntime {
    dsp_engine: DspEngine,
    position_frames_bits: AtomicU64,
    transport: AtomicU8,
    loop_enabled: AtomicBool,
    loop_start_frames_bits: AtomicU64,
    loop_end_frames_bits: AtomicU64,
    transport_generation: AtomicU64,
    source_volume_bits: AtomicU32,
    source_muted: AtomicBool,
    comparison_control: Arc<ComparisonAudioControl>,
    spectral_freeze: SpectralFreeze,
}

impl PlaybackRuntime {
    fn from_track(
        track: &Track,
        config: &StreamConfig,
        comparison_control: Arc<ComparisonAudioControl>,
    ) -> Self {
        Self {
            dsp_engine: DspEngine::new(
                Arc::from(track.samples.clone()),
                track.channels.max(1),
                track.sample_rate.max(1),
                config.sample_rate.0.max(1),
                config.channels.max(1),
            ),
            position_frames_bits: AtomicU64::new(0.0f64.to_bits()),
            transport: AtomicU8::new(TransportState::Stopped.as_u8()),
            loop_enabled: AtomicBool::new(false),
            loop_start_frames_bits: AtomicU64::new(0.0f64.to_bits()),
            loop_end_frames_bits: AtomicU64::new(0.0f64.to_bits()),
            transport_generation: AtomicU64::new(1),
            source_volume_bits: AtomicU32::new(1.0f32.to_bits()),
            source_muted: AtomicBool::new(false),
            comparison_control,
            spectral_freeze: SpectralFreeze::new(config.channels as usize, config.sample_rate.0),
        }
    }

    fn total_source_frames(&self) -> usize {
        self.dsp_engine.total_source_frames()
    }

    fn duration_seconds(&self) -> f64 {
        self.total_source_frames() as f64 / self.dsp_engine.source_sample_rate().max(1) as f64
    }

    fn current_position_frames(&self) -> f64 {
        f64::from_bits(self.position_frames_bits.load(Ordering::Relaxed))
    }

    fn set_position_frames(&self, frames: f64) {
        self.position_frames_bits
            .store(frames.to_bits(), Ordering::Relaxed);
    }

    fn current_position_seconds(&self) -> f64 {
        self.current_position_frames() / self.dsp_engine.source_sample_rate().max(1) as f64
    }

    fn seek_to_seconds(&self, seconds: f64) {
        let clamped = seconds.clamp(0.0, self.duration_seconds());
        self.set_position_frames(clamped * self.dsp_engine.source_sample_rate() as f64);
    }

    fn is_finished(&self) -> bool {
        self.current_position_frames() >= self.total_source_frames() as f64
    }

    fn step_ratio(&self) -> f64 {
        self.dsp_engine.step_ratio()
    }

    fn loop_range_frames(&self) -> Option<(f64, f64)> {
        if !self.loop_enabled.load(Ordering::Relaxed) {
            return None;
        }

        let start = f64::from_bits(self.loop_start_frames_bits.load(Ordering::Relaxed));
        let end = f64::from_bits(self.loop_end_frames_bits.load(Ordering::Relaxed));
        if end > start {
            Some((start, end))
        } else {
            None
        }
    }

    fn current_dsp_settings(&self) -> PlaybackDspSettings {
        self.dsp_engine.current_dsp_settings()
    }

    fn set_dsp_settings(&self, settings: PlaybackDspSettings) {
        self.dsp_engine.set_dsp_settings(settings);
        self.dsp_engine
            .reset_stream_to(self.current_position_frames());
    }
}

/// 再生位置の表示を滑らかに保つため、再生中はおよそ60 FPSでUIを更新する。
pub const UI_REPAINT_INTERVAL: Duration = Duration::from_micros(16_667);

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
    // 停止中も実行する。Seek 後の旧世代をここで捨てることで、worker が
    // 新しい位置の先読みを再開できる。
    runtime.dsp_engine.discard_stale_stream_frames();
    let transport = TransportState::from_u8(runtime.transport.load(Ordering::Relaxed));
    let mut position_frames = runtime.current_position_frames();
    let total_frames = runtime.total_source_frames() as f64;
    let loop_range = runtime.loop_range_frames();
    let dsp_settings = runtime.current_dsp_settings();
    let speed_ratio = dsp_settings.speed_ratio.max(0.25) as f64;
    let use_streaming = dsp_settings.preserve_pitch_on_speed_change;
    let source_volume =
        f32::from_bits(runtime.source_volume_bits.load(Ordering::Relaxed)).clamp(0.0, 1.0);
    let mut streamed_frame = [0.0f32; 32];

    for frame in output.chunks_mut(output_channels) {
        let freeze_gains = runtime.spectral_freeze.mix_gains();
        let frozen = freeze_gains.is_some();
        if !frozen
            && let Some((loop_start, loop_end)) = loop_range
            && position_frames >= loop_end
        {
            position_frames = loop_start;
            runtime.transport_generation.fetch_add(1, Ordering::Relaxed);
            runtime.dsp_engine.reset_stream_to(loop_start);
            if transport == TransportState::Playing {
                runtime.comparison_control.advance();
            }
        }

        if transport != TransportState::Playing
            || total_frames == 0.0
            || position_frames >= total_frames
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

        if let Some((freeze_gain, normal_gain)) = freeze_gains {
            let source_is_audible = runtime
                .comparison_control
                .snapshot()
                .source_is_audible(runtime.source_muted.load(Ordering::Relaxed));
            for (channel, sample) in frame.iter_mut().enumerate() {
                let frozen_value = runtime.spectral_freeze.sample(channel);
                let normal_value = runtime
                    .dsp_engine
                    .render_source_sample(position_frames, channel);
                *sample = T::from_sample(if source_is_audible {
                    (frozen_value * freeze_gain + normal_value * normal_gain) * source_volume
                } else {
                    0.0
                });
            }
            runtime.spectral_freeze.advance();
            position_frames = runtime.spectral_freeze.position_frames();
            continue;
        }

        let streamed = use_streaming
            && output_channels <= streamed_frame.len()
            && runtime
                .dsp_engine
                .stream_frame(&mut streamed_frame[..output_channels]);
        if use_streaming && !streamed {
            for sample in frame {
                *sample = T::from_sample(0.0);
            }
            continue;
        }
        let source_is_audible = runtime
            .comparison_control
            .snapshot()
            .source_is_audible(runtime.source_muted.load(Ordering::Relaxed));
        for (channel, sample) in frame.iter_mut().enumerate() {
            let value = if streamed {
                streamed_frame[channel]
            } else {
                runtime
                    .dsp_engine
                    .render_source_sample(position_frames, channel)
            };
            *sample = T::from_sample(if source_is_audible {
                value * source_volume
            } else {
                0.0
            });
        }

        position_frames += runtime.step_ratio() * speed_ratio;
    }

    runtime.set_position_frames(position_frames);
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
