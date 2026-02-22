use std::time::Duration;

pub struct Backoff {
    base: Duration,
    current: Duration,
    max: Duration,
}

impl Backoff {
    pub fn new(base: Duration, max: Duration) -> Self {
        Self {
            base,
            current: base,
            max,
        }
    }

    pub fn multiply(&mut self, multiplier: u32) {
        self.current = (self.current * multiplier).min(self.max);
    }

    pub fn reset(&mut self) {
        self.current = self.base
    }

    pub fn get(&self) -> Duration {
        self.current
    }
}
