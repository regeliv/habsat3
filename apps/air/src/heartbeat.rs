use std::{
    collections::HashMap,
    num::NonZero,
    time::{Duration, SystemTime},
};

use tokio::sync::broadcast;

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("System time must not be changed to before the UNIX epoch.")
}

#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub unix_time: Duration,
}

pub struct Heartbeat {
    interval: Duration,
    beat_broadcast: HashMap<NonZero<usize>, (broadcast::Sender<Tick>, broadcast::Receiver<Tick>)>,
}

impl Heartbeat {
    pub fn new(interval: Duration) -> Self {
        Heartbeat {
            interval,
            beat_broadcast: HashMap::new(),
        }
    }

    pub fn rx_every_n_beats(&mut self, n: NonZero<usize>) -> broadcast::Receiver<Tick> {
        self.beat_broadcast
            .entry(n)
            .or_insert_with(|| broadcast::channel(1))
            .1
            .resubscribe()
    }

    pub async fn run(&self) {
        let mut ticks = 0usize;

        let mut beat =
            tokio::time::interval_at(tokio::time::Instant::now() + self.interval, self.interval);

        loop {
            beat.tick().await;

            let unix_time = unix_time();

            for (multiple, (sender, _)) in self.beat_broadcast.iter() {
                if ticks.is_multiple_of(multiple.get()) {
                    // It's fine if we have no receivers, so we ignore the error
                    let _ = sender.send(Tick { unix_time });
                }
            }
            ticks += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    async fn validate_tick_interval(interval: Duration, mut ticks: broadcast::Receiver<Tick>) {
        let valid_range = ((interval * 98) / 100)..=((interval * 102) / 100);

        let mut last_tick = ticks.recv().await.unwrap();
        let mut last_beat_time = Instant::now();

        for _ in 0..10 {
            let new_tick = ticks.recv().await.unwrap();
            let new_beat_time = Instant::now();

            assert!(valid_range.contains(&(new_tick.unix_time - last_tick.unix_time)));
            assert!(valid_range.contains(&(new_beat_time - last_beat_time)));

            last_tick = new_tick;
            last_beat_time = new_beat_time;
        }
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let interval = Duration::from_millis(50);

        let mut beat = Heartbeat::new(interval);

        let ticks_every_50ms = beat.rx_every_n_beats(NonZero::new(1).unwrap());
        let ticks_every_150ms = beat.rx_every_n_beats(NonZero::new(3).unwrap());
        let ticks_every_550ms = beat.rx_every_n_beats(NonZero::new(11).unwrap());
        let ticks_every_1000ms = beat.rx_every_n_beats(NonZero::new(20).unwrap());

        tokio::spawn(async move { beat.run().await });

        tokio::join!(
            validate_tick_interval(Duration::from_millis(50), ticks_every_50ms),
            validate_tick_interval(Duration::from_millis(150), ticks_every_150ms),
            validate_tick_interval(Duration::from_millis(550), ticks_every_550ms),
            validate_tick_interval(Duration::from_millis(1000), ticks_every_1000ms),
        );
    }
}
