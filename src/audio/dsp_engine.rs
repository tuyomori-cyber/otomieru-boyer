use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use crate::audio::streaming::{StreamingMetrics, StreamingPassthrough};
use crate::audio::timestretch::{
    DspTransportEvent, PlaybackDspChainSettings, ProcessorGraph, SourceAudioView,
};
use crate::model::{EqualizerSettings, PlaybackDspSettings, EQ_BAND_COUNT};

pub struct DspEngine {
    samples: Arc<[f32]>,
    source_channels: u16,
    source_sample_rate: u32,
    output_sample_rate: u32,
    dsp_speed_ratio_bits: AtomicU32,
    dsp_pitch_shift_semitones: AtomicI32,
    dsp_preserve_pitch_on_speed_change: AtomicBool,
    dsp_eq_gain_bits: [AtomicU32; EQ_BAND_COUNT],
    processor_graph: Mutex<ProcessorGraph>,
    streaming: StreamingPassthrough,
}

impl DspEngine {
    pub fn new(
        samples: Arc<[f32]>,
        source_channels: u16,
        source_sample_rate: u32,
        output_sample_rate: u32,
        output_channels: u16,
    ) -> Self {
        let source_channels = source_channels.max(1);
        let source_sample_rate = source_sample_rate.max(1);
        let output_sample_rate = output_sample_rate.max(1);
        Self {
            streaming: StreamingPassthrough::new(
                Arc::clone(&samples),
                source_channels as usize,
                source_sample_rate,
                output_sample_rate,
                output_channels as usize,
            ),
            samples,
            source_channels,
            source_sample_rate,
            output_sample_rate,
            dsp_speed_ratio_bits: AtomicU32::new(1.0f32.to_bits()),
            dsp_pitch_shift_semitones: AtomicI32::new(0),
            dsp_preserve_pitch_on_speed_change: AtomicBool::new(true),
            dsp_eq_gain_bits: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            processor_graph: Mutex::new(ProcessorGraph::default()),
        }
    }

    pub fn source_channels(&self) -> usize {
        self.source_channels as usize
    }

    pub fn total_source_frames(&self) -> usize {
        self.samples.len() / self.source_channels()
    }

    pub fn source_sample_rate(&self) -> u32 {
        self.source_sample_rate
    }

    pub fn source_channels_u16(&self) -> u16 {
        self.source_channels
    }

    pub fn step_ratio(&self) -> f64 {
        self.source_sample_rate as f64 / self.output_sample_rate.max(1) as f64
    }

    pub fn current_dsp_settings(&self) -> PlaybackDspSettings {
        PlaybackDspSettings {
            speed_ratio: f32::from_bits(self.dsp_speed_ratio_bits.load(Ordering::Relaxed)),
            pitch_shift_semitones: self.dsp_pitch_shift_semitones.load(Ordering::Relaxed),
            preserve_pitch_on_speed_change: self
                .dsp_preserve_pitch_on_speed_change
                .load(Ordering::Relaxed),
            equalizer: EqualizerSettings {
                gains_db: self
                    .dsp_eq_gain_bits
                    .each_ref()
                    .map(|gain| f32::from_bits(gain.load(Ordering::Relaxed))),
            },
        }
    }

    pub fn dsp_chain_settings(&self) -> PlaybackDspChainSettings {
        PlaybackDspChainSettings::from_playback_settings(self.current_dsp_settings())
    }

    pub fn set_dsp_settings(&self, settings: PlaybackDspSettings) {
        self.dsp_speed_ratio_bits
            .store(settings.speed_ratio.to_bits(), Ordering::Relaxed);
        self.dsp_pitch_shift_semitones
            .store(settings.pitch_shift_semitones, Ordering::Relaxed);
        self.dsp_preserve_pitch_on_speed_change
            .store(settings.preserve_pitch_on_speed_change, Ordering::Relaxed);
        for (storage, gain_db) in self
            .dsp_eq_gain_bits
            .iter()
            .zip(settings.equalizer.gains_db)
        {
            storage.store(gain_db.to_bits(), Ordering::Relaxed);
        }

        self.streaming.configure(
            settings.speed_ratio,
            settings.pitch_shift_semitones,
            settings.equalizer,
            settings.preserve_pitch_on_speed_change,
        );
        if let Ok(mut processor_graph) = self.processor_graph.lock() {
            processor_graph.configure(self.dsp_chain_settings());
            processor_graph.reset();
        }
    }

    pub fn reset_processors(&self) {
        if let Ok(mut processor_graph) = self.processor_graph.lock() {
            processor_graph.reset();
        }
    }

    pub fn notify_transport_event(&self, event: DspTransportEvent) {
        match event {
            DspTransportEvent::Start => {}
            DspTransportEvent::Stop => self.streaming.reset(0.0),
            DspTransportEvent::Seek { position_seconds } => self
                .streaming
                .reset(position_seconds * self.source_sample_rate as f64),
            DspTransportEvent::LoopJump { start_seconds, .. } => self
                .streaming
                .reset(start_seconds * self.source_sample_rate as f64),
        }
        if let Ok(mut processor_graph) = self.processor_graph.lock() {
            processor_graph.handle_transport_event(event);
        }
    }

    pub fn processor_chain_settings(&self) -> PlaybackDspChainSettings {
        if let Ok(processor_graph) = self.processor_graph.lock() {
            processor_graph.current_settings()
        } else {
            self.dsp_chain_settings()
        }
    }

    pub fn render_sample(&self, position_frames: f64, channel: usize) -> f32 {
        let source = SourceAudioView::new(&self.samples, self.source_channels());
        let sample = if let Ok(mut processor_graph) = self.processor_graph.lock() {
            processor_graph.render_time_stretched_sample(&source, position_frames, channel)
        } else {
            source.interpolate_sample(position_frames, channel)
        };

        if let Ok(mut processor_graph) = self.processor_graph.lock() {
            processor_graph.process_sample(sample, channel)
        } else {
            sample
        }
    }

    pub fn render_source_sample(&self, position_frames: f64, channel: usize) -> f32 {
        SourceAudioView::new(&self.samples, self.source_channels())
            .interpolate_sample(position_frames, channel)
    }

    /// 音声コールバックから呼べる、ロックを取らない世代切替です。
    pub fn reset_stream_to(&self, source_position_frames: f64) {
        self.streaming.reset(source_position_frames);
    }

    pub fn stream_frame(&self, destination: &mut [f32]) -> bool {
        self.streaming.pop_frame(destination)
    }

    /// 音声コールバックから旧 generation を破棄するための入口です。
    pub fn discard_stale_stream_frames(&self) {
        self.streaming.discard_stale_frames();
    }

    pub fn streaming_metrics(&self) -> StreamingMetrics {
        self.streaming.metrics()
    }
}
