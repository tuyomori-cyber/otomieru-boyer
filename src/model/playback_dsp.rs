#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackDspSettings {
    pub speed_ratio: f32,
    pub pitch_shift_semitones: i32,
    pub preserve_pitch_on_speed_change: bool,
    pub equalizer: EqualizerSettings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqualizerSettings {
    pub gains_db: [f32; EQ_BAND_COUNT],
}

pub const EQ_BAND_COUNT: usize = 5;
pub const EQ_BAND_FREQUENCIES_HZ: [f32; EQ_BAND_COUNT] = [100.0, 250.0, 1_000.0, 4_000.0, 10_000.0];

impl EqualizerSettings {
    pub fn gain_for_frequency_hz(self, frequency_hz: f32) -> f32 {
        let frequency_hz = frequency_hz.max(1.0);
        let gain_db = if frequency_hz <= EQ_BAND_FREQUENCIES_HZ[0] {
            self.gains_db[0]
        } else if frequency_hz >= EQ_BAND_FREQUENCIES_HZ[EQ_BAND_COUNT - 1] {
            self.gains_db[EQ_BAND_COUNT - 1]
        } else {
            let index = EQ_BAND_FREQUENCIES_HZ
                .windows(2)
                .position(|bands| frequency_hz <= bands[1])
                .unwrap_or(EQ_BAND_COUNT - 2);
            let low = EQ_BAND_FREQUENCIES_HZ[index].ln();
            let high = EQ_BAND_FREQUENCIES_HZ[index + 1].ln();
            let ratio = ((frequency_hz.ln() - low) / (high - low)).clamp(0.0, 1.0);
            self.gains_db[index] + (self.gains_db[index + 1] - self.gains_db[index]) * ratio
        };
        10.0_f32.powf(gain_db / 20.0)
    }
}

impl Default for EqualizerSettings {
    fn default() -> Self {
        Self {
            gains_db: [0.0; EQ_BAND_COUNT],
        }
    }
}

impl PlaybackDspSettings {
    pub fn octave_up(&mut self) {
        self.pitch_shift_semitones = 12;
    }

    pub fn octave_down(&mut self) {
        self.pitch_shift_semitones = -12;
    }

    pub fn reset_pitch(&mut self) {
        self.pitch_shift_semitones = 0;
    }
}

impl Default for PlaybackDspSettings {
    fn default() -> Self {
        Self {
            speed_ratio: 1.0,
            pitch_shift_semitones: 0,
            preserve_pitch_on_speed_change: true,
            equalizer: EqualizerSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EqualizerSettings, EQ_BAND_FREQUENCIES_HZ};

    #[test]
    fn equalizer_interpolates_gain_between_bands() {
        let settings = EqualizerSettings {
            gains_db: [0.0, 6.0, 0.0, -6.0, 0.0],
        };
        assert!((settings.gain_for_frequency_hz(EQ_BAND_FREQUENCIES_HZ[1]) - 2.0).abs() < 0.01);
        assert!(settings.gain_for_frequency_hz(2_000.0) < 1.0);
    }
}
