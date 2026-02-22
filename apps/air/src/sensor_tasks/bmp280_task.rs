use crate::{
    sensor_tasks::utils::Backoff,
    types::{Labeled, Tick, Timestamped},
};
use i2cdev::linux::LinuxI2CDevice;
use std::time::Duration;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

pub async fn bmp280_task(
    heartbeat: broadcast::Receiver<Tick>,
    bmp280_tx: kanal::AsyncSender<Timestamped<Labeled<bmp280::Bmp280Reading>>>,
    alt_address: bool,
) {
    let address = if alt_address { 0x77 } else { 0x76 };
    let label = if alt_address { 0x1 } else { 0x0 };

    info!("Started BMP280({label}) task");

    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(5 * 60));

    loop {
        backoff.multiply(2);

        tokio::time::sleep(backoff.get()).await;

        let Ok(device) = LinuxI2CDevice::new("/dev/i2c-1", address)
            .inspect_err(|e| warn!("Failed to create BMP280({label}) i2c device: {e}"))
        else {
            continue;
        };

        let Ok(bmp280) = bmp280::Bmp280::new(device)
            .inspect_err(|e| warn!("Failed to setup BMP280({label}): {e}"))
        else {
            continue;
        };

        info!("BMP280({label}) setup successfully");

        backoff.reset();

        bmp280_run(bmp280, heartbeat.resubscribe(), bmp280_tx.clone(), label).await;
    }
}

async fn bmp280_run(
    mut bmp280: bmp280::Bmp280<LinuxI2CDevice>,
    mut heartbeat: broadcast::Receiver<Tick>,
    bmp280_tx: kanal::AsyncSender<Timestamped<Labeled<bmp280::Bmp280Reading>>>,
    label: u8,
) {
    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let Ok(reading) = bmp280
                    .reading()
                    .await
                    .inspect_err(|e| warn!("Failed to get BMP280({label}) reading: {e}"))
                else {
                    return;
                };

                bmp280_tx
                    .send(Timestamped::new(tick, Labeled::new(label, reading)))
                    .await
                    .ok();
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
