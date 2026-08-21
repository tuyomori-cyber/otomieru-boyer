use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::model::{EqualizerSettings, EQ_BAND_COUNT, EQ_BAND_FREQUENCIES_HZ};

const RING_FRAMES: usize = 32_768;
const LOW_WATER_FRAMES: usize = 9_600;
const HIGH_WATER_FRAMES: usize = 19_200;
const PROCESS_FRAMES: usize = 512;
const RB_OPTION_PROCESS_REALTIME: i32 = 0x0000_0001;
const RB_OPTION_THREADING_NEVER: i32 = 0x0001_0000;
const RB_OPTION_CHANNELS_TOGETHER: i32 = 0x1000_0000;

#[repr(C)]
struct RubberBandState(c_void);

#[link(name = "rubberband")]
unsafe extern "C" {
    fn rubberband_new(
        sample_rate: u32,
        channels: u32,
        options: i32,
        initial_time_ratio: f64,
        initial_pitch_scale: f64,
    ) -> *mut RubberBandState;
    fn rubberband_delete(state: *mut RubberBandState);
    fn rubberband_get_preferred_start_pad(state: *const RubberBandState) -> u32;
    fn rubberband_get_start_delay(state: *const RubberBandState) -> u32;
    fn rubberband_process(
        state: *mut RubberBandState,
        input: *const *const f32,
        frames: u32,
        final_block: i32,
    );
    fn rubberband_available(state: *const RubberBandState) -> i32;
    fn rubberband_retrieve(
        state: *mut RubberBandState,
        output: *const *mut f32,
        frames: u32,
    ) -> u32;
}

/// Rubber Band のリアルタイムAPIを worker だけで利用する backend です。
/// 入出力ワーク領域は生成時に確保し、`next_frame` 中に確保しません。
struct RubberBandBackend {
    state: NonNull<RubberBandState>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    input_ptrs: Vec<*const f32>,
    output_ptrs: Vec<*mut f32>,
    output_frames: usize,
    output_offset: usize,
    remaining_start_pad: usize,
    remaining_start_delay: usize,
    equalizer: Equalizer,
}

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn from_coefficients(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn low_shelf(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w = std::f32::consts::TAU * frequency / sample_rate;
        let cos = w.cos();
        let alpha = w.sin() * 0.5 * ((a + 1.0 / a) * 2.0).sqrt();
        let beta = 2.0 * a.sqrt() * alpha;
        Self::from_coefficients(
            a * ((a + 1.0) - (a - 1.0) * cos + beta),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - beta),
            (a + 1.0) + (a - 1.0) * cos + beta,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - beta,
        )
    }

    fn peaking(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w = std::f32::consts::TAU * frequency / sample_rate;
        let alpha = w.sin() * 0.5;
        Self::from_coefficients(
            1.0 + alpha * a,
            -2.0 * w.cos(),
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * w.cos(),
            1.0 - alpha / a,
        )
    }

    fn high_shelf(sample_rate: f32, frequency: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w = std::f32::consts::TAU * frequency / sample_rate;
        let cos = w.cos();
        let alpha = w.sin() * 0.5 * ((a + 1.0 / a) * 2.0).sqrt();
        let beta = 2.0 * a.sqrt() * alpha;
        Self::from_coefficients(
            a * ((a + 1.0) + (a - 1.0) * cos + beta),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - beta),
            (a + 1.0) - (a - 1.0) * cos + beta,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - beta,
        )
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

struct Equalizer {
    filters: Vec<Vec<Biquad>>,
}

impl Equalizer {
    fn new(sample_rate: u32, channels: usize, settings: EqualizerSettings) -> Self {
        let rate = sample_rate as f32;
        Self {
            filters: (0..channels)
                .map(|_| {
                    (0..EQ_BAND_COUNT)
                        .map(|index| match index {
                            0 => Biquad::low_shelf(
                                rate,
                                EQ_BAND_FREQUENCIES_HZ[index],
                                settings.gains_db[index],
                            ),
                            index if index == EQ_BAND_COUNT - 1 => Biquad::high_shelf(
                                rate,
                                EQ_BAND_FREQUENCIES_HZ[index],
                                settings.gains_db[index],
                            ),
                            _ => Biquad::peaking(
                                rate,
                                EQ_BAND_FREQUENCIES_HZ[index],
                                settings.gains_db[index],
                            ),
                        })
                        .collect()
                })
                .collect(),
        }
    }

    fn process(&mut self, channel: usize, sample: f32) -> f32 {
        self.filters[channel]
            .iter_mut()
            .fold(sample, |value, filter| filter.process(value))
    }
}

impl RubberBandBackend {
    fn new(
        sample_rate: u32,
        channels: usize,
        speed: f64,
        pitch_semitones: i32,
        equalizer: EqualizerSettings,
    ) -> Self {
        let channels = channels.max(1);
        let options =
            RB_OPTION_PROCESS_REALTIME | RB_OPTION_THREADING_NEVER | RB_OPTION_CHANNELS_TOGETHER;
        // SAFETY: arguments are valid for the library and the returned state is owned below.
        let state = unsafe {
            rubberband_new(
                sample_rate,
                channels as u32,
                options,
                1.0 / speed.clamp(0.25, 4.0),
                2.0f64.powf(pitch_semitones as f64 / 12.0),
            )
        };
        let state = NonNull::new(state).expect("failed to initialize Rubber Band stretcher");
        let input = (0..channels)
            .map(|_| vec![0.0; PROCESS_FRAMES])
            .collect::<Vec<_>>();
        let mut output = (0..channels)
            .map(|_| vec![0.0; PROCESS_FRAMES])
            .collect::<Vec<_>>();
        let input_ptrs = input.iter().map(|channel| channel.as_ptr()).collect();
        let output_ptrs = output
            .iter_mut()
            .map(|channel| channel.as_mut_ptr())
            .collect();
        // SAFETY: state remains valid for the lifetime of this backend.
        let remaining_start_pad =
            unsafe { rubberband_get_preferred_start_pad(state.as_ptr()) } as usize;
        // SAFETY: state remains valid for the lifetime of this backend.
        let remaining_start_delay = unsafe { rubberband_get_start_delay(state.as_ptr()) } as usize;

        Self {
            state,
            input,
            output,
            input_ptrs,
            output_ptrs,
            output_frames: 0,
            output_offset: 0,
            remaining_start_pad,
            remaining_start_delay,
            equalizer: Equalizer::new(sample_rate, channels, equalizer),
        }
    }

    fn next_frame(
        &mut self,
        samples: &[f32],
        source_channels: usize,
        total_frames: usize,
        source_step: f64,
        source_position: &mut f64,
        destination: &mut [f32],
    ) {
        loop {
            if self.output_offset < self.output_frames {
                let frame = self.output_offset;
                self.output_offset += 1;
                if self.remaining_start_delay > 0 {
                    self.remaining_start_delay -= 1;
                    continue;
                }
                for (channel, sample) in destination.iter_mut().enumerate() {
                    *sample = self.equalizer.process(channel, self.output[channel][frame]);
                }
                return;
            }

            let mut input_frames = 0;
            while input_frames < PROCESS_FRAMES && self.remaining_start_pad > 0 {
                self.remaining_start_pad -= 1;
                input_frames += 1;
            }
            while input_frames < PROCESS_FRAMES {
                for channel in 0..self.input.len() {
                    self.input[channel][input_frames] = interpolate(
                        samples,
                        source_channels,
                        total_frames,
                        *source_position,
                        channel,
                    );
                }
                *source_position += source_step;
                input_frames += 1;
            }
            // SAFETY: planar buffers and pointer arrays remain valid during this synchronous call.
            unsafe {
                rubberband_process(
                    self.state.as_ptr(),
                    self.input_ptrs.as_ptr(),
                    input_frames as u32,
                    0,
                );
            }
            // SAFETY: state remains valid and no other thread accesses this backend.
            let available = unsafe { rubberband_available(self.state.as_ptr()) }.max(0) as usize;
            if available == 0 {
                continue;
            }
            let requested = available.min(PROCESS_FRAMES);
            // SAFETY: output planes each have PROCESS_FRAMES writable samples.
            self.output_frames = unsafe {
                rubberband_retrieve(
                    self.state.as_ptr(),
                    self.output_ptrs.as_ptr(),
                    requested as u32,
                )
            } as usize;
            self.output_offset = 0;
        }
    }
}

impl Drop for RubberBandBackend {
    fn drop(&mut self) {
        // SAFETY: this backend owns the non-null state and drops it once.
        unsafe { rubberband_delete(self.state.as_ptr()) };
    }
}

/// 音声コールバック（consumer）と worker（producer）専用の固定長 SPSC リングです。
/// producer は frame を完全に書いた後で head を公開するため、consumer 側では
/// ロックも確保も行いません。
struct FrameRing {
    samples: Box<[UnsafeCell<f32>]>,
    generations: Box<[AtomicU64]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    channels: usize,
    capacity: usize,
}

unsafe impl Send for FrameRing {}
unsafe impl Sync for FrameRing {}

impl FrameRing {
    fn new(channels: usize) -> Self {
        let channels = channels.max(1);
        let capacity = RING_FRAMES;
        Self {
            samples: (0..capacity * channels)
                .map(|_| UnsafeCell::new(0.0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            generations: (0..capacity)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            channels,
            capacity,
        }
    }

    fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail).min(self.capacity)
    }

    fn try_push(&self, generation: u64, frame: &[f32]) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.capacity {
            return false;
        }

        let slot = head % self.capacity;
        let start = slot * self.channels;
        for channel in 0..self.channels {
            // SAFETY: producer is the sole writer, and this slot is not visible until head is stored.
            unsafe { *self.samples[start + channel].get() = frame[channel] };
        }
        self.generations[slot].store(generation, Ordering::Release);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    fn try_pop(&self, expected_generation: u64, destination: &mut [f32]) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return false;
        }

        let slot = tail % self.capacity;
        let generation = self.generations[slot].load(Ordering::Acquire);
        let start = slot * self.channels;
        if generation == expected_generation {
            for channel in 0..self.channels {
                // SAFETY: consumer is the sole reader after acquiring the published head.
                destination[channel] = unsafe { *self.samples[start + channel].get() };
            }
        }
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        generation == expected_generation
    }

    /// consumer 専用。世代切替後のフレームを一括で捨て、新しい generation の
    /// worker が直ちに空きスロットへ書けるようにする。
    fn discard_stale(&self, expected_generation: u64) {
        let mut tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        while tail != head {
            let slot = tail % self.capacity;
            if self.generations[slot].load(Ordering::Acquire) == expected_generation {
                break;
            }
            tail = tail.wrapping_add(1);
        }
        self.tail.store(tail, Ordering::Release);
    }
}

struct WorkerState {
    ring: FrameRing,
    generation: AtomicU64,
    requested_position_bits: AtomicU64,
    speed_bits: AtomicU32,
    pitch_shift_semitones: AtomicI32,
    eq_gain_bits: [AtomicU32; EQ_BAND_COUNT],
    enabled: AtomicBool,
    shutdown: AtomicBool,
    underruns: AtomicU64,
    generated_frames: AtomicU64,
    prepare_complete: AtomicBool,
}

pub struct StreamingPassthrough {
    state: Arc<WorkerState>,
    worker: Option<JoinHandle<()>>,
    channels: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StreamingMetrics {
    pub buffered_frames: usize,
    pub underruns: u64,
    pub generated_frames: u64,
    pub ready: bool,
    pub allocated_bytes: usize,
}

impl StreamingPassthrough {
    pub fn new(
        samples: Arc<[f32]>,
        source_channels: usize,
        source_rate: u32,
        output_rate: u32,
        output_channels: usize,
    ) -> Self {
        let channels = output_channels.max(1);
        let state = Arc::new(WorkerState {
            ring: FrameRing::new(channels),
            generation: AtomicU64::new(1),
            requested_position_bits: AtomicU64::new(0.0f64.to_bits()),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
            pitch_shift_semitones: AtomicI32::new(0),
            eq_gain_bits: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            enabled: AtomicBool::new(true),
            shutdown: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            generated_frames: AtomicU64::new(0),
            prepare_complete: AtomicBool::new(false),
        });
        let worker_state = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("time-stretch-worker".to_owned())
            .spawn(move || {
                run_worker(
                    samples,
                    source_channels.max(1),
                    source_rate.max(1),
                    output_rate.max(1),
                    worker_state,
                )
            })
            .expect("failed to create time-stretch worker");

        Self {
            state,
            worker: Some(worker),
            channels,
        }
    }

    pub fn configure(
        &self,
        speed: f32,
        pitch_shift_semitones: i32,
        equalizer: EqualizerSettings,
        enabled: bool,
    ) {
        self.state
            .speed_bits
            .store(speed.clamp(0.25, 4.0).to_bits(), Ordering::Release);
        self.state
            .pitch_shift_semitones
            .store(pitch_shift_semitones.clamp(-24, 24), Ordering::Release);
        for (storage, gain_db) in self.state.eq_gain_bits.iter().zip(equalizer.gains_db) {
            storage.store(gain_db.clamp(-12.0, 12.0).to_bits(), Ordering::Release);
        }
        self.state.enabled.store(enabled, Ordering::Release);
    }

    pub fn reset(&self, source_position_frames: f64) {
        self.state
            .requested_position_bits
            .store(source_position_frames.max(0.0).to_bits(), Ordering::Release);
        self.state.prepare_complete.store(false, Ordering::Release);
        self.state.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn pop_frame(&self, destination: &mut [f32]) -> bool {
        let generation = self.state.generation.load(Ordering::Acquire);
        let available = self.state.ring.try_pop(generation, destination);
        if !available {
            self.state.underruns.fetch_add(1, Ordering::Relaxed);
        }
        available
    }

    /// 音声コールバックからのみ呼びます。停止中にも実行することで、Seek 後に
    /// 旧世代のリングが満杯のまま worker を妨げる状態を防ぎます。
    pub fn discard_stale_frames(&self) {
        let generation = self.state.generation.load(Ordering::Acquire);
        self.state.ring.discard_stale(generation);
    }

    pub fn metrics(&self) -> StreamingMetrics {
        StreamingMetrics {
            buffered_frames: self.state.ring.len(),
            underruns: self.state.underruns.load(Ordering::Relaxed),
            generated_frames: self.state.generated_frames.load(Ordering::Relaxed),
            ready: self.state.prepare_complete.load(Ordering::Acquire),
            allocated_bytes: RING_FRAMES * self.channels * std::mem::size_of::<f32>()
                + RING_FRAMES * std::mem::size_of::<u64>(),
        }
    }
}

impl Drop for StreamingPassthrough {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    samples: Arc<[f32]>,
    source_channels: usize,
    source_rate: u32,
    output_rate: u32,
    state: Arc<WorkerState>,
) {
    let total_frames = samples.len() / source_channels;
    let mut active_generation = 0;
    let mut position = 0.0;
    let mut frame = vec![0.0; state.ring.channels];
    let mut backend = None;

    while !state.shutdown.load(Ordering::Acquire) {
        let generation = state.generation.load(Ordering::Acquire);
        if generation != active_generation {
            active_generation = generation;
            position = f64::from_bits(state.requested_position_bits.load(Ordering::Acquire));
            let speed = f32::from_bits(state.speed_bits.load(Ordering::Acquire)) as f64;
            let pitch_shift_semitones = state.pitch_shift_semitones.load(Ordering::Acquire);
            let equalizer = EqualizerSettings {
                gains_db: state
                    .eq_gain_bits
                    .each_ref()
                    .map(|gain| f32::from_bits(gain.load(Ordering::Acquire))),
            };
            backend = Some(RubberBandBackend::new(
                output_rate,
                state.ring.channels,
                speed,
                pitch_shift_semitones,
                equalizer,
            ));
        }

        if !state.enabled.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(2));
            continue;
        }

        let buffered_frames = state.ring.len();
        if buffered_frames >= LOW_WATER_FRAMES {
            thread::sleep(Duration::from_millis(2));
            continue;
        }

        // 一度起動したworkerは高水位までまとめて生成する。出力1フレームごとに
        // sleep/wakeしないため、CPUスケジューリングと原子操作の負荷を抑えられる。
        let frames_to_generate = HIGH_WATER_FRAMES - buffered_frames;
        let source_step = source_rate as f64 / output_rate as f64;
        for _ in 0..frames_to_generate {
            backend
                .as_mut()
                .expect("time stretch backend must be initialized")
                .next_frame(
                    &samples,
                    source_channels,
                    total_frames,
                    source_step,
                    &mut position,
                    &mut frame,
                );
            if state.generation.load(Ordering::Acquire) != active_generation {
                break;
            }
            if !state.ring.try_push(active_generation, &frame) {
                break;
            }
            state.generated_frames.fetch_add(1, Ordering::Relaxed);
        }
        if state.generation.load(Ordering::Acquire) == active_generation
            && state.ring.len() >= LOW_WATER_FRAMES
        {
            state.prepare_complete.store(true, Ordering::Release);
        }
    }
}

fn interpolate(
    samples: &[f32],
    channels: usize,
    total_frames: usize,
    position: f64,
    channel: usize,
) -> f32 {
    let base = position.floor().max(0.0) as usize;
    if base >= total_frames {
        return 0.0;
    }
    let next = (base + 1).min(total_frames.saturating_sub(1));
    let source_channel = channel.min(channels.saturating_sub(1));
    let current = samples[base * channels + source_channel];
    let following = samples[next * channels + source_channel];
    current + (following - current) * (position - base as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::{interpolate, EqualizerSettings, FrameRing, RubberBandBackend};

    #[test]
    fn ring_discards_a_frame_from_an_old_generation() {
        let ring = FrameRing::new(2);
        assert!(ring.try_push(4, &[0.25, -0.25]));

        let mut frame = [0.0; 2];
        assert!(!ring.try_pop(5, &mut frame));
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn interpolation_is_linear_and_silences_past_the_source() {
        let source = [0.0, 1.0, 2.0, 3.0];
        assert_eq!(interpolate(&source, 1, 4, 1.5, 0), 1.5);
        assert_eq!(interpolate(&source, 1, 4, 4.0, 0), 0.0);
    }

    #[test]
    fn discarding_stale_frames_frees_the_ring_for_a_new_generation() {
        let ring = FrameRing::new(1);
        assert!(ring.try_push(2, &[1.0]));
        assert!(ring.try_push(2, &[2.0]));

        ring.discard_stale(3);
        assert_eq!(ring.len(), 0);
        assert!(ring.try_push(3, &[3.0]));
    }

    #[test]
    fn rubber_band_backend_produces_finite_audio() {
        let source = (0..96_000)
            .map(|frame| (frame as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin())
            .collect::<Vec<_>>();
        let mut backend = RubberBandBackend::new(48_000, 1, 0.75, 12, EqualizerSettings::default());
        let mut position = 0.0;
        let mut frame = [0.0];
        let mut peak = 0.0f32;

        for _ in 0..8_000 {
            backend.next_frame(&source, 1, source.len(), 1.0, &mut position, &mut frame);
            assert!(frame[0].is_finite());
            peak = peak.max(frame[0].abs());
        }

        assert!(peak > 0.01);
    }
}
