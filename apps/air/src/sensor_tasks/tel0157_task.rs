use std::time::Duration;

use crate::{
    sensor_tasks::utils::Backoff,
    types::{Tick, Timestamped},
};
use i2cdev::linux::LinuxI2CDevice;
use tel0157::TEL0157_I2C_ADDR;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

pub async fn tel0157_task(
    heartbeat: broadcast::Receiver<Tick>,
    tel0157_tx: kanal::AsyncSender<Timestamped<tel0157::Tel0157Reading>>,
) {
    info!("Started TEL0157 task");

    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(5 * 60));

    loop {
        backoff.multiply(2);

        tokio::time::sleep(backoff.get()).await;

        let Ok(dev) = LinuxI2CDevice::new("/dev/i2c-1", TEL0157_I2C_ADDR as u16)
            .inspect_err(|e| warn!("Failed to open TEL0157 i2c device: {e}"))
        else {
            continue;
        };

        let Ok(tel0157) =
            tel0157::Tel0157::new(dev).inspect_err(|e| warn!("Failed to setup TEL0157: {e}"))
        else {
            continue;
        };

        backoff.reset();

        tel0157_run(tel0157, heartbeat.resubscribe(), tel0157_tx.clone()).await;
    }
}

async fn tel0157_run(
    mut tel0157: tel0157::Tel0157<LinuxI2CDevice>,
    mut heartbeat: broadcast::Receiver<Tick>,
    tel0157_tx: kanal::AsyncSender<Timestamped<tel0157::Tel0157Reading>>,
) {
    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let Ok(reading) = tel0157
                    .reading()
                    .inspect_err(|e| warn!("Failed to get TEL0157 reading: {e}"))
                else {
                    continue;
                };

                tel0157_tx.send(Timestamped::new(tick, reading)).await.ok();
            }

            Err(RecvError::Lagged(_)) => {
                warn!("Skipped a beat");
            }

            Err(RecvError::Closed) => {
                unreachable!("Heartbeat should never stop ticking while a task is running");
            }
        }
    }
}
