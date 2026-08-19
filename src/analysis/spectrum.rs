use crate::analysis::pitch_map::frequency_to_midi;
use crate::analysis::stft::StftResult;

pub const MIN_MIDI_NOTE: usize = 24;
pub const MAX_MIDI_NOTE: usize = 108;

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
