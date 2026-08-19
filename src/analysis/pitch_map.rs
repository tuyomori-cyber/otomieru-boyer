pub const A4_FREQUENCY_HZ: f32 = 440.0;
pub const A4_MIDI_NOTE: f32 = 69.0;

pub fn midi_to_frequency(midi_note: f32) -> f32 {
    A4_FREQUENCY_HZ * 2.0_f32.powf((midi_note - A4_MIDI_NOTE) / 12.0)
}

pub fn frequency_to_midi(frequency_hz: f32) -> f32 {
    A4_MIDI_NOTE + 12.0 * (frequency_hz / A4_FREQUENCY_HZ).log2()
}
