#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
}
