#[derive(Debug, Clone, Copy)]
pub struct TimeStretchSettings {
    pub speed: f32,
    pub preserve_pitch: bool,
}

impl Default for TimeStretchSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            preserve_pitch: true,
        }
    }
}
