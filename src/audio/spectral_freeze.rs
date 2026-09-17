use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use rustfft::{FftPlanner, num_complex::Complex32};

pub const FREEZE_FFT_SIZE: usize = 4096;
const FREEZE_HOP_SIZE: usize = FREEZE_FFT_SIZE / 4;
const INACTIVE: u8 = 0;
const FADING_IN: u8 = 1;
const ACTIVE: u8 = 2;
const FADING_OUT: u8 = 3;

/// 音声コールバックがロックも確保もせず読む、二重バッファのFreezeフレーム。
pub struct SpectralFreeze {
    mode: AtomicU8,
    position_frames_bits: AtomicU64,
    active_buffer: AtomicUsize,
    buffers: [Vec<AtomicU32>; 2],
    channels: usize,
    fade_frames: usize,
    transition_frame: AtomicUsize,
    output_frame: AtomicUsize,
}

impl SpectralFreeze {
    pub fn new(channels: usize, output_sample_rate: u32) -> Self {
        let channels = channels.clamp(1, 32);
        let make_buffer = || {
            (0..FREEZE_HOP_SIZE * channels)
                .map(|_| AtomicU32::new(0.0f32.to_bits()))
                .collect()
        };
        Self {
            mode: AtomicU8::new(INACTIVE),
            position_frames_bits: AtomicU64::new(0.0f64.to_bits()),
            active_buffer: AtomicUsize::new(0),
            buffers: [make_buffer(), make_buffer()],
            channels,
            fade_frames: ((output_sample_rate as f64 * 0.020).round() as usize).max(1),
            transition_frame: AtomicUsize::new(0),
            output_frame: AtomicUsize::new(0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.mode.load(Ordering::Acquire) != INACTIVE
    }

    pub fn position_frames(&self) -> f64 {
        f64::from_bits(self.position_frames_bits.load(Ordering::Acquire))
    }

    pub fn activate(&self, position_frames: f64, mut sample_at: impl FnMut(usize, usize) -> f32) {
        // activeではない側へだけ書くため、callbackと同じ領域を同時に触らない。
        let target = 1 - self.active_buffer.load(Ordering::Acquire);
        let buffer = &self.buffers[target];
        let mut planner = FftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(FREEZE_FFT_SIZE);
        let inverse = planner.plan_fft_inverse(FREEZE_FFT_SIZE);
        let start = position_frames.floor().max(0.0) as usize;
        for channel in 0..self.channels {
            let mut spectrum = (0..FREEZE_FFT_SIZE)
                .map(|offset| {
                    let window = 0.5
                        - 0.5
                            * (std::f32::consts::TAU * offset as f32 / FREEZE_FFT_SIZE as f32)
                                .cos();
                    Complex32::new(sample_at(start + offset, channel) * window, 0.0)
                })
                .collect::<Vec<_>>();
            forward.process(&mut spectrum);
            inverse.process(&mut spectrum);
            // 同じスペクトルフレームを1/4 hopで4重に重ね、窓二乗和で正規化する。
            for offset in 0..FREEZE_HOP_SIZE {
                let (mixed, weight) = (0..4).fold((0.0, 0.0), |(mixed, weight), block| {
                    let index = offset + block * FREEZE_HOP_SIZE;
                    let window = 0.5
                        - 0.5
                            * (std::f32::consts::TAU * index as f32 / FREEZE_FFT_SIZE as f32).cos();
                    (
                        mixed + spectrum[index].re / FREEZE_FFT_SIZE as f32 * window,
                        weight + window * window,
                    )
                });
                let sample = (mixed / weight.max(f32::EPSILON)).clamp(-1.0, 1.0);
                buffer[channel * FREEZE_HOP_SIZE + offset]
                    .store(sample.to_bits(), Ordering::Relaxed);
            }
        }
        self.position_frames_bits
            .store(position_frames.to_bits(), Ordering::Release);
        self.active_buffer.store(target, Ordering::Release);
        self.output_frame.store(0, Ordering::Release);
        self.transition_frame.store(0, Ordering::Release);
        self.mode.store(FADING_IN, Ordering::Release);
    }

    pub fn begin_deactivation(&self) {
        if self.mode.load(Ordering::Acquire) != INACTIVE {
            self.transition_frame.store(0, Ordering::Release);
            self.mode.store(FADING_OUT, Ordering::Release);
        }
    }

    pub fn stop(&self) {
        self.mode.store(INACTIVE, Ordering::Release);
    }

    /// (Freeze音の係数, 通常音の係数)。equal-powerで20msだけ切り替える。
    pub fn mix_gains(&self) -> Option<(f32, f32)> {
        let mode = self.mode.load(Ordering::Acquire);
        if mode == INACTIVE {
            return None;
        }
        if mode == ACTIVE {
            return Some((1.0, 0.0));
        }
        let frame = self.transition_frame.fetch_add(1, Ordering::Relaxed);
        let t = (frame as f32 / self.fade_frames as f32).clamp(0.0, 1.0);
        let angle = t * std::f32::consts::FRAC_PI_2;
        if frame + 1 >= self.fade_frames {
            self.mode.store(
                if mode == FADING_IN { ACTIVE } else { INACTIVE },
                Ordering::Release,
            );
        }
        Some(if mode == FADING_IN {
            (angle.sin(), angle.cos())
        } else {
            (angle.cos(), angle.sin())
        })
    }

    pub fn sample(&self, channel: usize) -> f32 {
        let buffer = self.active_buffer.load(Ordering::Acquire);
        let channel = channel.min(self.channels - 1);
        let output_frame = self.output_frame.load(Ordering::Relaxed);
        f32::from_bits(
            self.buffers[buffer][channel * FREEZE_HOP_SIZE + output_frame % FREEZE_HOP_SIZE]
                .load(Ordering::Relaxed),
        )
    }

    pub fn advance(&self) {
        self.output_frame.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{FREEZE_HOP_SIZE, SpectralFreeze};

    #[test]
    fn frozen_frame_is_finite_and_repeats() {
        let freeze = SpectralFreeze::new(1, 48_000);
        freeze.activate(20.0, |frame, _| (frame as f32 * 0.1).sin());
        assert!(freeze.is_active());
        assert!(freeze.sample(0).is_finite());
        for _ in 0..3 {
            freeze.advance();
        }
        let sample = freeze.sample(0);
        for _ in 0..FREEZE_HOP_SIZE {
            freeze.advance();
        }
        assert_eq!(sample, freeze.sample(0));
    }

    #[test]
    fn boundary_short_and_silent_inputs_produce_only_finite_samples() {
        for (samples, positions) in [
            (vec![0.25_f32, -0.5, 0.75], vec![0.0, 2.0, 3.0]),
            (vec![0.0_f32; 8], vec![0.0, 7.0, 8.0]),
        ] {
            for position in positions {
                let freeze = SpectralFreeze::new(1, 48_000);
                freeze.activate(position, |frame, _| {
                    samples.get(frame).copied().unwrap_or(0.0)
                });

                for _ in 0..FREEZE_HOP_SIZE * 2 {
                    assert!(freeze.sample(0).is_finite());
                    freeze.advance();
                }

                freeze.begin_deactivation();
                freeze.stop();
                assert!(!freeze.is_active());
            }
        }
    }
}
