#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    Playing,
    Paused,
    #[default]
    Stopped,
}
