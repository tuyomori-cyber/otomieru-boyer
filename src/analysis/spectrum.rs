use crate::analysis::pitch_map::{frequency_to_midi, midi_to_frequency};
use crate::analysis::stft::StftResult;

pub const MIN_MIDI_NOTE: usize = 24;
pub const MAX_MIDI_NOTE: usize = 108;
const HARMONIC_WEIGHTS: [(usize, f32); 5] = [(2, 0.65), (3, 0.45), (4, 0.35), (5, 0.25), (6, 0.20)];

#[derive(Debug, Clone)]
pub struct SpectrogramData {
    pub frames: usize,
    pub pitches: usize,
    pub min_midi_note: usize,
    pub max_midi_note: usize,
    pub frame_duration_seconds: f64,
    pub intensities: Vec<f32>,
}

impl SpectrogramData {
    pub fn empty() -> Self {
        Self {
            frames: 0,
            pitches: 0,
            min_midi_note: MIN_MIDI_NOTE,
            max_midi_note: MAX_MIDI_NOTE,
            frame_duration_seconds: 0.0,
            intensities: Vec::new(),
        }
    }

    pub fn intensity_at(&self, frame: usize, pitch: usize) -> f32 {
        if frame >= self.frames || pitch >= self.pitches {
            return 0.0;
        }

        self.intensities[frame * self.pitches + pitch]
    }

    /// 倍音列を根拠にした基音候補の強度を返す。
    ///
    /// 元のセル強度に、2〜6倍音に対応するセルを重み付きで加える。これは
    /// 表示用の候補スコアであり、単一音の検出結果を保証するものではない。
    pub fn fundamental_strength_at(&self, frame: usize, pitch: usize) -> f32 {
        if frame >= self.frames || pitch >= self.pitches {
            return 0.0;
        }

        let target_midi = self.min_midi_note + pitch;
        let target_frequency = midi_to_frequency(target_midi as f32);
        let mut strength = self.intensity_at(frame, pitch);

        for (harmonic, weight) in HARMONIC_WEIGHTS {
            let harmonic_midi =
                frequency_to_midi(target_frequency * harmonic as f32).round() as isize;
            if harmonic_midi < self.min_midi_note as isize
                || harmonic_midi > self.max_midi_note as isize
            {
                continue;
            }

            let harmonic_pitch = harmonic_midi as usize - self.min_midi_note;
            strength += self.intensity_at(frame, harmonic_pitch) * weight;
        }

        strength.clamp(0.0, 1.0)
    }
}

impl Default for SpectrogramData {
    fn default() -> Self {
        Self::empty()
    }
}

pub fn build_spectrogram(stft: &StftResult) -> SpectrogramData {
    let pitches = MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1;
    let frames = stft.frames.len();
    if frames == 0 {
        return SpectrogramData::empty();
    }

    let mut intensities = vec![0.0_f32; frames * pitches];
    let bin_hz = stft.sample_rate as f32 / stft.window_size as f32;

    for (frame_index, frame) in stft.frames.iter().enumerate() {
        for (bin_index, magnitude) in frame.iter().enumerate().skip(1) {
            let frequency_hz = bin_index as f32 * bin_hz;
            if frequency_hz <= 0.0 {
                continue;
            }

            let midi = frequency_to_midi(frequency_hz).round() as isize;
            if midi < MIN_MIDI_NOTE as isize || midi > MAX_MIDI_NOTE as isize {
                continue;
            }

            let pitch_index = (midi as usize) - MIN_MIDI_NOTE;
            let slot = frame_index * pitches + pitch_index;
            intensities[slot] = intensities[slot].max(magnitude.log10().max(0.0));
        }
    }

    normalize(&mut intensities);

    SpectrogramData {
        frames,
        pitches,
        min_midi_note: MIN_MIDI_NOTE,
        max_midi_note: MAX_MIDI_NOTE,
        frame_duration_seconds: stft.hop_size as f64 / stft.sample_rate as f64,
        intensities,
    }
}

fn normalize(intensities: &mut [f32]) {
    let max = intensities
        .iter()
        .copied()
        .fold(0.0_f32, |acc, value| acc.max(value));
    if max <= f32::EPSILON {
        return;
    }

    for value in intensities {
        *value /= max;
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_MIDI_NOTE, MIN_MIDI_NOTE, SpectrogramData};

    fn data() -> SpectrogramData {
        let pitches = MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1;
        SpectrogramData {
            frames: 1,
            pitches,
            min_midi_note: MIN_MIDI_NOTE,
            max_midi_note: MAX_MIDI_NOTE,
            frame_duration_seconds: 0.01,
            intensities: vec![0.0; pitches],
        }
    }

    #[test]
    fn fundamental_strength_includes_an_octave_harmonic() {
        let mut spectrogram = data();
        let c2 = 36 - MIN_MIDI_NOTE;
        let c3 = 48 - MIN_MIDI_NOTE;
        spectrogram.intensities[c2] = 0.2;
        spectrogram.intensities[c3] = 0.8;

        assert!((spectrogram.fundamental_strength_at(0, c2) - 0.72).abs() < 0.001);
    }
}
