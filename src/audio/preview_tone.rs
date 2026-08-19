#[derive(Debug, Clone, Copy)]
pub struct PreviewToneRequest {
    pub midi_note: u8,
    pub duration_ms: u32,
}
