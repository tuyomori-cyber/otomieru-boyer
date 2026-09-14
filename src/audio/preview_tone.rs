use std::f32::consts::TAU;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::analysis::pitch_map::midi_to_frequency;
use crate::audio::comparison::ComparisonAudioControl;
use crate::audio::decoder::DecoderError;
use crate::audio::piano_samples::{PianoSample, PianoSampleBank, SynthStringsSampleBank};

// 矩形波は一定振幅で高調波も多いため、録音サンプルと同じ振幅では大きく聞こえる。
const SQUARE_WAVE_OUTPUT_GAIN: f32 = 0.25;
const DEFAULT_REFERENCE_A4_HZ: f32 = 440.0;
const MIN_REFERENCE_A4_HZ: f32 = 430.0;
const MAX_REFERENCE_A4_HZ: f32 = 450.0;
pub const MAX_MEMO_VOICES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PreviewTimbre {
    #[default]
    Piano = 0,
    Square = 1,
    SynthStrings = 2,
}

impl PreviewTimbre {
    pub const ALL: [Self; 3] = [Self::Piano, Self::SynthStrings, Self::Square];

    pub fn label(self) -> &'static str {
        match self {
            Self::Piano => "ピアノ",
            Self::SynthStrings => "シンセストリングス",
            Self::Square => "矩形波",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Square,
            2 => Self::SynthStrings,
            _ => Self::Piano,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PreviewToneRequest {
    pub midi_note: u8,
    pub timbre: PreviewTimbre,
    /// 0.0 から 0.5 までの出力振幅。大きすぎる試聴音を避けるため上限を設ける。
    pub amplitude: f32,
    /// 試聴音のA4基準周波数。元音源に合わせるための値で、430〜450 Hzに制限する。
    pub reference_a4_hz: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoToneRequest {
    pub memo_key: u64,
    pub midi_note: u8,
    pub amplitude: f32,
}

pub struct PreviewTonePlayer {
    state: Arc<PreviewToneState>,
    memo_slots: [Option<MemoToneRequest>; MAX_MEMO_VOICES],
    last_transport_generation: Option<u64>,
    _stream: Stream,
}

struct PreviewToneState {
    active: AtomicBool,
    midi_note: AtomicU8,
    timbre: AtomicU8,
    amplitude_bits: AtomicU32,
    reference_a4_hz_bits: AtomicU32,
    memo_voices: [ToneVoiceControl; MAX_MEMO_VOICES],
    comparison_control: Arc<ComparisonAudioControl>,
}

struct ToneVoiceControl {
    active: AtomicBool,
    key: AtomicU64,
    midi_note: AtomicU8,
    amplitude_bits: AtomicU32,
}

impl ToneVoiceControl {
    fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            key: AtomicU64::new(0),
            midi_note: AtomicU8::new(69),
            amplitude_bits: AtomicU32::new(0.0f32.to_bits()),
        }
    }

    fn update(&self, request: MemoToneRequest) {
        self.active.store(false, Ordering::Release);
        // 同じメモをシークやループで鳴らし直す場合も、音声スレッドへ
        // 必ず新しい発音として伝わるようスロット固有の世代を進める。
        self.key.fetch_add(1, Ordering::Relaxed);
        self.midi_note.store(request.midi_note, Ordering::Relaxed);
        self.amplitude_bits.store(
            request.amplitude.clamp(0.0, 0.5).to_bits(),
            Ordering::Relaxed,
        );
        self.active.store(true, Ordering::Release);
    }

    fn stop(&self) {
        self.active.store(false, Ordering::Release);
    }
}

#[derive(Debug)]
pub enum PreviewToneError {
    NoOutputDevice,
    DefaultConfig(cpal::DefaultStreamConfigError),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    SampleDecode(DecoderError),
}

impl fmt::Display for PreviewToneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no default output device found"),
            Self::DefaultConfig(error) => write!(f, "failed to get default output config: {error}"),
            Self::BuildStream(error) => write!(f, "failed to build preview stream: {error}"),
            Self::PlayStream(error) => write!(f, "failed to start preview stream: {error}"),
            Self::SampleDecode(error) => {
                write!(f, "failed to load embedded piano samples: {error}")
            }
        }
    }
}

impl std::error::Error for PreviewToneError {}

impl PreviewTonePlayer {
    pub fn new(comparison_control: Arc<ComparisonAudioControl>) -> Result<Self, PreviewToneError> {
        let piano_samples =
            Arc::new(PianoSampleBank::load().map_err(PreviewToneError::SampleDecode)?);
        let synth_strings_samples =
            Arc::new(SynthStringsSampleBank::load().map_err(PreviewToneError::SampleDecode)?);
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
            midi_note: AtomicU8::new(69),
            timbre: AtomicU8::new(PreviewTimbre::Piano as u8),
            amplitude_bits: AtomicU32::new(0.16f32.to_bits()),
            reference_a4_hz_bits: AtomicU32::new(DEFAULT_REFERENCE_A4_HZ.to_bits()),
            memo_voices: std::array::from_fn(|_| ToneVoiceControl::new()),
            comparison_control,
        });
        let stream_state = Arc::clone(&state);
        let stream_samples = Arc::clone(&piano_samples);
        let stream_strings = Arc::clone(&synth_strings_samples);

        let stream = match config.sample_format() {
            SampleFormat::F32 => build_preview_stream::<f32>(
                &device,
                &stream_config,
                stream_state,
                stream_samples,
                stream_strings,
            ),
            SampleFormat::I16 => build_preview_stream::<i16>(
                &device,
                &stream_config,
                stream_state,
                stream_samples,
                stream_strings,
            ),
            SampleFormat::U16 => build_preview_stream::<u16>(
                &device,
                &stream_config,
                stream_state,
                stream_samples,
                stream_strings,
            ),
            _ => build_preview_stream::<f32>(
                &device,
                &stream_config,
                stream_state,
                stream_samples,
                stream_strings,
            ),
        }
        .map_err(PreviewToneError::BuildStream)?;

        stream.play().map_err(PreviewToneError::PlayStream)?;

        Ok(Self {
            state,
            memo_slots: [None; MAX_MEMO_VOICES],
            last_transport_generation: None,
            _stream: stream,
        })
    }

    pub fn update_preview(&self, request: PreviewToneRequest) {
        self.state
            .midi_note
            .store(request.midi_note, Ordering::Relaxed);
        self.state
            .timbre
            .store(request.timbre as u8, Ordering::Relaxed);
        self.state.amplitude_bits.store(
            request.amplitude.clamp(0.0, 0.5).to_bits(),
            Ordering::Relaxed,
        );
        self.state.reference_a4_hz_bits.store(
            request
                .reference_a4_hz
                .clamp(MIN_REFERENCE_A4_HZ, MAX_REFERENCE_A4_HZ)
                .to_bits(),
            Ordering::Relaxed,
        );
        self.state.active.store(true, Ordering::Relaxed);
    }

    pub fn stop_preview(&self) {
        self.state.active.store(false, Ordering::Relaxed);
    }

    pub fn sync_memo_voices(
        &mut self,
        requests: &[MemoToneRequest],
        timbre: PreviewTimbre,
        reference_a4_hz: f32,
        transport_generation: u64,
    ) {
        let requests = &requests[..requests.len().min(MAX_MEMO_VOICES)];
        self.state.timbre.store(timbre as u8, Ordering::Relaxed);
        self.state.reference_a4_hz_bits.store(
            reference_a4_hz
                .clamp(MIN_REFERENCE_A4_HZ, MAX_REFERENCE_A4_HZ)
                .to_bits(),
            Ordering::Relaxed,
        );

        if self.last_transport_generation != Some(transport_generation) {
            self.stop_all_memo_voices();
            self.last_transport_generation = Some(transport_generation);
        }

        for slot in 0..MAX_MEMO_VOICES {
            if self.memo_slots[slot].is_some_and(|current| {
                !requests
                    .iter()
                    .any(|request| request.memo_key == current.memo_key)
            }) {
                self.state.memo_voices[slot].stop();
                self.memo_slots[slot] = None;
            }
        }

        for request in requests.iter().copied() {
            let slot = self
                .memo_slots
                .iter()
                .position(|current| {
                    current.is_some_and(|current| current.memo_key == request.memo_key)
                })
                .or_else(|| self.memo_slots.iter().position(Option::is_none));
            let Some(slot) = slot else {
                break;
            };
            if self.memo_slots[slot] != Some(request) {
                self.state.memo_voices[slot].update(request);
                self.memo_slots[slot] = Some(request);
            }
        }
    }

    pub fn stop_all_memo_voices(&mut self) {
        for (slot, control) in self
            .memo_slots
            .iter_mut()
            .zip(self.state.memo_voices.iter())
        {
            control.stop();
            *slot = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ToneVoiceRuntime {
    active: bool,
    key: u64,
    midi_note: u8,
    timbre: PreviewTimbre,
    amplitude: f32,
    reference_a4_hz: f32,
    sample_index: usize,
    strings_sample_index: usize,
    sample_position: f64,
    square_phase: f32,
}

impl Default for ToneVoiceRuntime {
    fn default() -> Self {
        Self {
            active: false,
            key: 0,
            midi_note: 69,
            timbre: PreviewTimbre::Piano,
            amplitude: 0.0,
            reference_a4_hz: DEFAULT_REFERENCE_A4_HZ,
            sample_index: 0,
            strings_sample_index: 0,
            sample_position: 0.0,
            square_phase: 0.0,
        }
    }
}

impl ToneVoiceRuntime {
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        &mut self,
        active: bool,
        key: u64,
        midi_note: u8,
        timbre: PreviewTimbre,
        amplitude: f32,
        reference_a4_hz: f32,
        piano_samples: &PianoSampleBank,
        synth_strings_samples: &SynthStringsSampleBank,
    ) {
        if !active {
            self.active = false;
            return;
        }
        if !self.active || self.key != key || self.midi_note != midi_note || self.timbre != timbre {
            self.key = key;
            self.midi_note = midi_note;
            self.timbre = timbre;
            self.sample_index = piano_samples.sample_index_for_midi(midi_note);
            self.strings_sample_index = synth_strings_samples.sample_index_for_midi(midi_note);
            self.sample_position = 0.0;
            self.square_phase = 0.0;
        }
        self.amplitude = amplitude;
        self.reference_a4_hz = reference_a4_hz;
        self.active = true;
    }

    fn sample(
        self,
        channel: usize,
        piano_samples: &PianoSampleBank,
        synth_strings_samples: &SynthStringsSampleBank,
    ) -> f32 {
        if !self.active {
            return 0.0;
        }
        match self.timbre {
            PreviewTimbre::Piano => {
                interpolated_sample(
                    piano_samples.sample_at(self.sample_index),
                    self.sample_position,
                    channel,
                )
                .unwrap_or(0.0)
                    * self.amplitude
            }
            PreviewTimbre::SynthStrings => {
                interpolated_sample(
                    synth_strings_samples.sample_at(self.strings_sample_index),
                    self.sample_position,
                    channel,
                )
                .unwrap_or(0.0)
                    * self.amplitude
            }
            PreviewTimbre::Square => {
                square_wave_sample(self.square_phase, self.amplitude * SQUARE_WAVE_OUTPUT_GAIN)
            }
        }
    }

    fn advance(
        &mut self,
        output_sample_rate: f64,
        piano_samples: &PianoSampleBank,
        synth_strings_samples: &SynthStringsSampleBank,
    ) {
        if !self.active {
            return;
        }
        match self.timbre {
            PreviewTimbre::Piano => {
                let sample = piano_samples.sample_at(self.sample_index);
                self.sample_position = advance_sample_position(
                    sample,
                    self.sample_position,
                    self.midi_note,
                    output_sample_rate,
                    self.reference_a4_hz,
                );
            }
            PreviewTimbre::SynthStrings => {
                let sample = synth_strings_samples.sample_at(self.strings_sample_index);
                self.sample_position = advance_sample_position(
                    sample,
                    self.sample_position,
                    self.midi_note,
                    output_sample_rate,
                    self.reference_a4_hz,
                );
            }
            PreviewTimbre::Square => {
                self.square_phase = (self.square_phase
                    + TAU * midi_to_frequency(self.midi_note as f32) * self.reference_a4_hz
                        / DEFAULT_REFERENCE_A4_HZ
                        / output_sample_rate as f32)
                    % TAU;
            }
        }
    }
}

fn build_preview_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    state: Arc<PreviewToneState>,
    piano_samples: Arc<PianoSampleBank>,
    synth_strings_samples: Arc<SynthStringsSampleBank>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let output_sample_rate = config.sample_rate.0 as f64;
    let channels = config.channels as usize;
    let mut preview_voice = ToneVoiceRuntime::default();
    let mut memo_voices = [ToneVoiceRuntime::default(); MAX_MEMO_VOICES];

    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            for frame in data.chunks_mut(channels) {
                let timbre = PreviewTimbre::from_u8(state.timbre.load(Ordering::Relaxed));
                let reference_a4_hz =
                    f32::from_bits(state.reference_a4_hz_bits.load(Ordering::Relaxed));
                let memos_are_audible = state.comparison_control.snapshot().memos_are_audible();
                preview_voice.prepare(
                    state.active.load(Ordering::Relaxed),
                    0,
                    state.midi_note.load(Ordering::Relaxed),
                    timbre,
                    f32::from_bits(state.amplitude_bits.load(Ordering::Relaxed)),
                    reference_a4_hz,
                    &piano_samples,
                    &synth_strings_samples,
                );
                for (voice, control) in memo_voices.iter_mut().zip(state.memo_voices.iter()) {
                    voice.prepare(
                        control.active.load(Ordering::Acquire),
                        control.key.load(Ordering::Relaxed),
                        control.midi_note.load(Ordering::Relaxed),
                        timbre,
                        f32::from_bits(control.amplitude_bits.load(Ordering::Relaxed)),
                        reference_a4_hz,
                        &piano_samples,
                        &synth_strings_samples,
                    );
                }

                for (channel, out) in frame.iter_mut().enumerate() {
                    let memo_mix = if memos_are_audible {
                        memo_voices.iter().fold(0.0, |mix, voice| {
                            mix + voice.sample(channel, &piano_samples, &synth_strings_samples)
                        })
                    } else {
                        0.0
                    };
                    let preview =
                        preview_voice.sample(channel, &piano_samples, &synth_strings_samples);
                    *out = T::from_sample((preview + memo_mix).clamp(-1.0, 1.0));
                }
                preview_voice.advance(output_sample_rate, &piano_samples, &synth_strings_samples);
                for voice in &mut memo_voices {
                    voice.advance(output_sample_rate, &piano_samples, &synth_strings_samples);
                }
            }
        },
        move |error| eprintln!("preview stream error: {error}"),
        None,
    )
}

fn interpolated_sample(sample: &PianoSample, position: f64, channel: usize) -> Option<f32> {
    let channels = sample.audio.channels.max(1) as usize;
    let frame_count = sample.audio.samples.len() / channels;
    let frame = position.floor() as usize;
    let next_frame = frame.checked_add(1)?;
    if next_frame >= frame_count {
        return None;
    }
    let fraction = (position - frame as f64) as f32;
    let source_channel = channel % channels;
    let first = sample.audio.samples[frame * channels + source_channel];
    let next = sample.audio.samples[next_frame * channels + source_channel];
    Some(first + (next - first) * fraction)
}

fn advance_sample_position(
    sample: &PianoSample,
    position: f64,
    midi_note: u8,
    output_sample_rate: f64,
    reference_a4_hz: f32,
) -> f64 {
    let pitch_ratio = 2.0_f64.powf((midi_note as f64 - sample.root_midi as f64) / 12.0)
        * (reference_a4_hz / DEFAULT_REFERENCE_A4_HZ) as f64;
    let next_position =
        position + sample.audio.sample_rate as f64 / output_sample_rate * pitch_ratio;
    let Some((loop_start, loop_end)) = sample.loop_range_frames else {
        return next_position;
    };
    if next_position < loop_end as f64 {
        return next_position;
    }
    let loop_length = (loop_end - loop_start).max(1) as f64;
    loop_start as f64 + (next_position - loop_end as f64) % loop_length
}

fn square_wave_sample(phase: f32, amplitude: f32) -> f32 {
    if phase.sin() >= 0.0 {
        amplitude
    } else {
        -amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_REFERENCE_A4_HZ, MemoToneRequest, PreviewTimbre, SQUARE_WAVE_OUTPUT_GAIN,
        ToneVoiceControl, advance_sample_position, interpolated_sample, square_wave_sample,
    };
    use crate::audio::decoder::DecodedAudio;
    use crate::audio::piano_samples::PianoSample;
    use std::sync::atomic::Ordering;

    #[test]
    fn memo_voice_update_always_advances_its_trigger_generation() {
        let control = ToneVoiceControl::new();
        let request = MemoToneRequest {
            memo_key: 42,
            midi_note: 60,
            amplitude: 0.2,
        };

        control.update(request);
        let first_generation = control.key.load(Ordering::Relaxed);
        control.update(request);

        assert_eq!(first_generation, 1);
        assert_eq!(control.key.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn interpolates_stereo_sample_frames() {
        let sample = PianoSample {
            root_midi: 60.0,
            audio: DecodedAudio {
                samples: vec![0.0, 0.2, 1.0, 0.6, 0.0, 0.0],
                sample_rate: 44_100,
                channels: 2,
            },
            loop_range_frames: None,
        };
        assert_eq!(interpolated_sample(&sample, 0.5, 0), Some(0.5));
        assert!((interpolated_sample(&sample, 0.5, 1).unwrap() - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn square_wave_has_the_requested_amplitude() {
        assert_eq!(square_wave_sample(0.0, 0.16), 0.16);
        assert_eq!(square_wave_sample(std::f32::consts::PI, 0.16), -0.16);
        assert_eq!(PreviewTimbre::from_u8(1), PreviewTimbre::Square);
        assert_eq!(PreviewTimbre::from_u8(2), PreviewTimbre::SynthStrings);
        assert_eq!(0.16 * SQUARE_WAVE_OUTPUT_GAIN, 0.04);
    }

    #[test]
    fn looped_samples_wrap_to_the_declared_loop_range() {
        let sample = PianoSample {
            root_midi: 60.0,
            audio: DecodedAudio {
                samples: vec![0.0; 2_000],
                sample_rate: 1_000,
                channels: 1,
            },
            loop_range_frames: Some((100, 200)),
        };
        assert_eq!(
            advance_sample_position(&sample, 199.5, 60, 1_000.0, DEFAULT_REFERENCE_A4_HZ),
            100.5
        );
    }

    #[test]
    fn sample_playback_follows_the_reference_a4_pitch() {
        let sample = PianoSample {
            root_midi: 69.0,
            audio: DecodedAudio {
                samples: vec![0.0; 2_000],
                sample_rate: 1_000,
                channels: 1,
            },
            loop_range_frames: None,
        };
        assert!(
            (advance_sample_position(&sample, 0.0, 69, 1_000.0, 430.0) - 430.0 / 440.0).abs()
                < 1e-6
        );
        assert!(
            (advance_sample_position(&sample, 0.0, 69, 1_000.0, 450.0) - 450.0 / 440.0).abs()
                < 1e-6
        );
    }
}
