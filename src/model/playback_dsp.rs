#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackDspSettings {
    pub speed_ratio: f32,
    pub pitch_shift_semitones: i32,
    pub preserve_pitch_on_speed_change: bool,
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
        }
    }
}
