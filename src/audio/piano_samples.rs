use crate::audio::decoder::{DecodedAudio, DecoderError, decode_wav_bytes};

struct PianoSampleAsset {
    root_midi: f32,
    bytes: &'static [u8],
}

pub struct PianoSample {
    pub root_midi: f32,
    pub audio: DecodedAudio,
    pub loop_range_frames: Option<(usize, usize)>,
}

pub struct PianoSampleBank {
    samples: Vec<PianoSample>,
}

impl PianoSampleBank {
    pub fn load() -> Result<Self, DecoderError> {
        PIANO_SAMPLE_ASSETS
            .iter()
            .map(|asset| {
                Ok(PianoSample {
                    root_midi: asset.root_midi,
                    audio: decode_wav_bytes(asset.bytes)?,
                    loop_range_frames: None,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|samples| Self { samples })
    }

    pub fn sample_index_for_midi(&self, midi_note: u8) -> usize {
        nearest_sample_index(&self.samples, midi_note)
    }

    pub fn sample_at(&self, index: usize) -> &PianoSample {
        &self.samples[index]
    }
}

pub struct SynthStringsSampleBank {
    samples: Vec<PianoSample>,
}

impl SynthStringsSampleBank {
    pub fn load() -> Result<Self, DecoderError> {
        SYNTH_STRINGS_SAMPLE_ASSETS
            .iter()
            .map(|asset| {
                Ok(PianoSample {
                    root_midi: asset.root_midi,
                    audio: decode_wav_bytes(asset.bytes)?,
                    loop_range_frames: Some((asset.loop_start_frame, asset.loop_end_frame)),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|samples| Self { samples })
    }

    pub fn sample_index_for_midi(&self, midi_note: u8) -> usize {
        nearest_sample_index(&self.samples, midi_note)
    }

    pub fn sample_at(&self, index: usize) -> &PianoSample {
        &self.samples[index]
    }
}

fn nearest_sample_index(samples: &[PianoSample], midi_note: u8) -> usize {
    samples
        .iter()
        .enumerate()
        .min_by(|left, right| {
            (left.1.root_midi - midi_note as f32)
                .abs()
                .total_cmp(&(right.1.root_midi - midi_note as f32).abs())
        })
        .map(|(index, _)| index)
        .expect("the embedded sample bank contains samples")
}

macro_rules! piano_sample {
    ($root:expr, $file:literal) => {
        PianoSampleAsset {
            root_midi: $root,
            bytes: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/piano-fb/samples/",
                $file
            )),
        }
    };
}

// FreePats Piano FB small, mapped from its bundled SFZ file. The tuning offsets
// in that mapping are included in each root MIDI value.
const PIANO_SAMPLE_ASSETS: &[PianoSampleAsset] = &[
    piano_sample!(21.0, "A0.wav"),
    piano_sample!(23.9, "C1.wav"),
    piano_sample!(30.0, "F#1.wav"),
    piano_sample!(32.96, "A1.wav"),
    piano_sample!(37.95, "D2.wav"),
    piano_sample!(40.92, "F2.wav"),
    piano_sample!(46.75, "B2.wav"),
    piano_sample!(53.92, "F#3.wav"),
    piano_sample!(56.82, "A3.wav"),
    piano_sample!(59.91, "C4.wav"),
    piano_sample!(63.9, "E4.wav"),
    piano_sample!(71.0, "B4.wav"),
    piano_sample!(73.0, "C#5.wav"),
    piano_sample!(77.05, "F5.wav"),
    piano_sample!(82.0, "A#5.wav"),
    piano_sample!(84.0, "C6.wav"),
    piano_sample!(88.1, "E6.wav"),
    piano_sample!(91.04, "G6.wav"),
    piano_sample!(93.04, "A6.wav"),
    piano_sample!(96.0, "C7.wav"),
    piano_sample!(98.0, "D7.wav"),
    piano_sample!(101.0, "F7.wav"),
    piano_sample!(105.0, "A7.wav"),
];

struct SynthStringsSampleAsset {
    root_midi: f32,
    loop_start_frame: usize,
    loop_end_frame: usize,
    bytes: &'static [u8],
}

macro_rules! synth_strings_sample {
    ($root:expr, $start:expr, $end:expr, $file:literal) => {
        SynthStringsSampleAsset {
            root_midi: $root,
            loop_start_frame: $start,
            loop_end_frame: $end,
            bytes: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/synth-strings-1/samples/",
                $file
            )),
        }
    };
}

const SYNTH_STRINGS_SAMPLE_ASSETS: &[SynthStringsSampleAsset] = &[
    synth_strings_sample!(30.0, 304, 620_744, "F#1.wav"),
    synth_strings_sample!(36.0, 239, 218_136, "C2.wav"),
    synth_strings_sample!(42.0, 147, 387_489, "F#2.wav"),
    synth_strings_sample!(48.0, 118, 218_916, "C3.wav"),
    synth_strings_sample!(54.0, 75, 216_802, "F#3.wav"),
    synth_strings_sample!(60.0, 35, 354_941, "C4.wav"),
    synth_strings_sample!(66.0, 20, 250_855, "F#4.wav"),
    synth_strings_sample!(72.0, 40, 177_603, "C5.wav"),
    synth_strings_sample!(78.0, 20, 125_022, "F#5.wav"),
    synth_strings_sample!(84.0, 22, 75_130, "C6.wav"),
    synth_strings_sample!(90.0, 46, 182_387, "F#6.wav"),
    synth_strings_sample!(96.0, 30, 125_918, "C7.wav"),
];

#[cfg(test)]
mod tests {
    use super::{PianoSample, PianoSampleBank, SynthStringsSampleBank};
    use crate::audio::decoder::DecodedAudio;

    #[test]
    fn selects_the_nearest_recorded_note() {
        let bank = PianoSampleBank {
            samples: vec![
                PianoSample {
                    root_midi: 60.0,
                    audio: DecodedAudio::default(),
                    loop_range_frames: None,
                },
                PianoSample {
                    root_midi: 72.0,
                    audio: DecodedAudio::default(),
                    loop_range_frames: None,
                },
            ],
        };
        assert_eq!(
            bank.sample_at(bank.sample_index_for_midi(65)).root_midi,
            60.0
        );
        assert_eq!(
            bank.sample_at(bank.sample_index_for_midi(68)).root_midi,
            72.0
        );
    }

    #[test]
    fn embedded_piano_samples_decode_successfully() {
        let bank = PianoSampleBank::load().expect("embedded piano samples should decode");
        assert_eq!(bank.samples.len(), 23);
        assert!(
            bank.samples
                .iter()
                .all(|sample| !sample.audio.samples.is_empty())
        );
    }

    #[test]
    fn embedded_synth_strings_samples_decode_successfully() {
        let bank = SynthStringsSampleBank::load().expect("embedded strings should decode");
        assert_eq!(bank.samples.len(), 12);
        assert!(
            bank.samples
                .iter()
                .all(|sample| sample.loop_range_frames.is_some())
        );
    }
}
