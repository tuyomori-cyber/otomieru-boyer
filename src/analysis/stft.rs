#[derive(Debug, Clone, Copy)]
pub struct StftSettings {
    pub window_size: usize,
    pub hop_size: usize,
}

impl Default for StftSettings {
    fn default() -> Self {
        Self {
            window_size: 4096,
            hop_size: 512,
        }
    }
}
