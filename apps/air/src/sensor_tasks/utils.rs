use std::{ops::ControlFlow, time::Duration};

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

pub struct BackoffReset<Dev, Reset, Loop>
where
    Reset: AsyncFn() -> ControlFlow<Dev, ()>,
    Loop: AsyncFnMut(&mut Dev) -> (),
{
    pub backoff: Backoff,
    pub reset: Reset,
    pub data_loop: Loop,
}

impl<Dev, Reset, Loop> BackoffReset<Dev, Reset, Loop>
where
    Reset: AsyncFn() -> ControlFlow<Dev, ()>,
    Loop: AsyncFnMut(&mut Dev) -> (),
{
    pub async fn run(&mut self) -> () {
        loop {
            let dev = (self.reset)().await;

            match dev {
                ControlFlow::Break(mut dev) => {
                    self.backoff.reset();
                    (self.data_loop)(&mut dev).await;
                }
                ControlFlow::Continue(()) => {
                    tokio::time::sleep(self.backoff.get()).await;
                    self.backoff.multiply(2);
                }
            }
        }
    }
}
