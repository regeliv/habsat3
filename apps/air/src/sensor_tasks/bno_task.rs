use std::time::Duration;

use crate::{
    sensor_tasks::utils::Backoff,
    types::{Tick, Timestamped},
};
use bno_055::{BNO_055_I2C_ADDR, Bno055, SensorConfig};
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

pub async fn bno_task(
    bno_sensor_config: SensorConfig,
    heartbeat: broadcast::Receiver<Tick>,
    bno_tx: kanal::AsyncSender<Timestamped<bno_055::Bno055Reading>>,
) -> Result<(), LinuxI2CError> {
    info!("Started BNO-055 task");

    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(5 * 60));

    loop {
        backoff.multiply(2);

        tokio::time::sleep(backoff.get()).await;

        let Ok(dev) = LinuxI2CDevice::new("/dev/i2c-1", BNO_055_I2C_ADDR as u16)
            .inspect_err(|e| warn!("Failed to open I2C device: {e}"))
        else {
            continue;
        };

        let Ok(mut bno) =
            bno_055::Bno055::new(dev).inspect_err(|e| warn!("Failed to setup BNO-055: {e}"))
        else {
            continue;
        };
        info!("BNO-055 created");

        let Ok(()) = bno
            .set_sensor_config(&bno_sensor_config)
            .inspect_err(|e| warn!("Failed to set BNO-055 sensor config: {e}"))
        else {
            continue;
        };

        info!("BNO-055 config set to {:?}", bno_sensor_config);

        let Ok(()) = bno
            .set_operating_mode(bno_055::OperatingMode::NDOF_FMC_OFF)
            .inspect_err(|e| warn!("Failed to set BNO-055 operating mode: {e}"))
        else {
            continue;
        };

        backoff.reset();
        bno_run(bno, heartbeat.resubscribe(), bno_tx.clone()).await;
    }
}

async fn bno_run(
    mut bno_055: Bno055<LinuxI2CDevice>,
    mut heartbeat: broadcast::Receiver<Tick>,
    bno_tx: kanal::AsyncSender<Timestamped<bno_055::Bno055Reading>>,
) {
    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let Ok(data) = bno_055
                    .reading()
                    .inspect_err(|e| warn!("Failed to get BNO reading: {e}"))
                else {
                    return;
                };
                _ = bno_tx.send(Timestamped::new(tick, data)).await;
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
