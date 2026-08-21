#[derive(Debug, Clone, Copy)]
pub struct TimeStretchSettings {
    pub speed: f32,
    pub preserve_pitch: bool,
}

impl Default for TimeStretchSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            preserve_pitch: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PitchShiftSettings {
    pub semitones: i32,
}

impl Default for PitchShiftSettings {
    fn default() -> Self {
        Self { semitones: 0 }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlaybackDspChainSettings {
    pub time_stretch: TimeStretchSettings,
    pub pitch_shift: PitchShiftSettings,
}

impl PlaybackDspChainSettings {
    pub fn from_playback_settings(settings: PlaybackDspSettings) -> Self {
        Self {
            time_stretch: TimeStretchSettings {
                speed: settings.speed_ratio,
                preserve_pitch: settings.preserve_pitch_on_speed_change,
            },
            pitch_shift: PitchShiftSettings {
                semitones: settings.pitch_shift_semitones,
            },
        }
    }
}

pub struct SourceAudioView<'a> {
    samples: &'a [f32],
    channels: usize,
}

impl<'a> SourceAudioView<'a> {
    pub fn new(samples: &'a [f32], channels: usize) -> Self {
        Self {
            samples,
            channels: channels.max(1),
        }
    }

    pub fn interpolate_sample(&self, position_frames: f64, channel: usize) -> f32 {
        let base_frame = position_frames.floor() as usize;
        let next_frame = base_frame.saturating_add(1);
        let frac = (position_frames - base_frame as f64) as f32;

        let current = self.sample_at(base_frame, channel);
        let next = self.sample_at(next_frame, channel);
        current + (next - current) * frac
    }

    fn sample_at(&self, frame: usize, channel: usize) -> f32 {
        let index = frame
            .saturating_mul(self.channels)
            .saturating_add(channel.min(self.channels.saturating_sub(1)));

        self.samples.get(index).copied().unwrap_or(0.0)
    }
}

pub trait TimeStretchProcessor: Send {
    fn configure(&mut self, settings: TimeStretchSettings);
    fn reset(&mut self);
    fn handle_transport_event(&mut self, _event: DspTransportEvent) {}
    fn render_sample(
        &mut self,
        source: &SourceAudioView<'_>,
        position_frames: f64,
        channel: usize,
    ) -> f32;
    fn current_settings(&self) -> TimeStretchSettings;
}

pub trait PitchShiftProcessor: Send {
    fn configure(&mut self, settings: PitchShiftSettings);
    fn reset(&mut self);
    fn handle_transport_event(&mut self, _event: DspTransportEvent) {}
    fn process_sample(&mut self, sample: f32, _channel: usize) -> f32;
    fn current_settings(&self) -> PitchShiftSettings;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DspTransportEvent {
    Start,
    Stop,
    Seek {
        position_seconds: f64,
    },
    LoopJump {
        start_seconds: f64,
        end_seconds: f64,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PreparedTimeStretchProcessor {
    settings: TimeStretchSettings,
    last_event: Option<DspTransportEvent>,
}

impl TimeStretchProcessor for PreparedTimeStretchProcessor {
    fn configure(&mut self, settings: TimeStretchSettings) {
        self.settings = settings;
    }

    fn reset(&mut self) {}

    fn handle_transport_event(&mut self, event: DspTransportEvent) {
        self.last_event = Some(event);
    }

    fn render_sample(
        &mut self,
        source: &SourceAudioView<'_>,
        position_frames: f64,
        channel: usize,
    ) -> f32 {
        source.interpolate_sample(position_frames, channel)
    }

    fn current_settings(&self) -> TimeStretchSettings {
        self.settings
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BypassPitchShiftProcessor {
    settings: PitchShiftSettings,
}

impl PitchShiftProcessor for BypassPitchShiftProcessor {
    fn configure(&mut self, settings: PitchShiftSettings) {
        self.settings = settings;
    }

    fn reset(&mut self) {}

    fn process_sample(&mut self, sample: f32, _channel: usize) -> f32 {
        sample
    }

    fn current_settings(&self) -> PitchShiftSettings {
        self.settings
    }
}

pub struct ProcessorGraph {
    time_stretch: Box<dyn TimeStretchProcessor>,
    pitch_shift: Box<dyn PitchShiftProcessor>,
}

impl Default for ProcessorGraph {
    fn default() -> Self {
        Self {
            time_stretch: Box::new(PreparedTimeStretchProcessor::default()),
            pitch_shift: Box::new(BypassPitchShiftProcessor::default()),
        }
    }
}

impl ProcessorGraph {
    pub fn configure(&mut self, settings: PlaybackDspChainSettings) {
        self.time_stretch.configure(settings.time_stretch);
        self.pitch_shift.configure(settings.pitch_shift);
    }

    pub fn reset(&mut self) {
        self.time_stretch.reset();
        self.pitch_shift.reset();
    }

    pub fn handle_transport_event(&mut self, event: DspTransportEvent) {
        match event {
            DspTransportEvent::Start
            | DspTransportEvent::Stop
            | DspTransportEvent::Seek { .. }
            | DspTransportEvent::LoopJump { .. } => self.reset(),
        }
        self.time_stretch.handle_transport_event(event);
        self.pitch_shift.handle_transport_event(event);
    }

    pub fn current_settings(&self) -> PlaybackDspChainSettings {
        PlaybackDspChainSettings {
            time_stretch: self.time_stretch.current_settings(),
            pitch_shift: self.pitch_shift.current_settings(),
        }
    }

    pub fn render_time_stretched_sample(
        &mut self,
        source: &SourceAudioView<'_>,
        position_frames: f64,
        channel: usize,
    ) -> f32 {
        self.time_stretch
            .render_sample(source, position_frames, channel)
    }

    pub fn process_sample(&mut self, sample: f32, channel: usize) -> f32 {
        self.pitch_shift.process_sample(sample, channel)
    }
}
use crate::model::PlaybackDspSettings;
