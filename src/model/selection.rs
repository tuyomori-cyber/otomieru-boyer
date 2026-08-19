#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
}

impl Selection {
    pub fn normalized(self) -> Option<(f64, f64)> {
        let start = self.start_seconds?;
        let end = self.end_seconds?;
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        if (end - start) <= f64::EPSILON {
            None
        } else {
            Some((start, end))
        }
    }

    pub fn set_range(&mut self, start_seconds: f64, end_seconds: f64) {
        self.start_seconds = Some(start_seconds);
        self.end_seconds = Some(end_seconds);
    }

    pub fn clear(&mut self) {
        self.start_seconds = None;
        self.end_seconds = None;
    }
}
