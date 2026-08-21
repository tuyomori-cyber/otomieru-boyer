#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DspTransportEvent {
    Start,
    Stop,
    Seek {
        position_seconds: f64,
    },
    LoopJump {
        start_seconds: f64,
        end_seconds: f64,
    },
}
