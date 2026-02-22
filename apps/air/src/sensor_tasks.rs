use crate::{
    db::models::{
        NewBmp280Reading, NewBnoReading, NewCpuTemperature, NewFromTimestamped as _, NewFsUsage,
        NewMemoryUsage, NewTel0157Reading,
    },
    sensor_tasks::utils::Backoff,
    types::{DataBatches, Labeled, RxDataChannels, Tick, Timestamped},
};
use bno_055::{BNO_055_I2C_ADDR, SensorConfig};
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError};
use std::time::Duration;
use tel0157::TEL0157_I2C_ADDR;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

mod utils;

pub mod as7341_task;
pub mod lora_task;
pub mod system_stats_task;

pub async fn bno_task(
    bno_sensor_config: SensorConfig,
    mut heartbeat: broadcast::Receiver<Tick>,
    bno_tx: kanal::AsyncSender<Timestamped<bno_055::Bno055Reading>>,
) -> Result<(), LinuxI2CError> {
    info!("Started BNO-055 task");

    let dev = LinuxI2CDevice::new("/dev/i2c-1", BNO_055_I2C_ADDR as u16)
        .inspect_err(|e| warn!("Failed to open I2C device: {e}"))?;

    let mut bno = bno_055::Bno055::new(dev)?;
    info!("BNO-055 created");

    bno.set_sensor_config(&bno_sensor_config)?;
    info!("BNO-055 config set to {:?}", bno_sensor_config);

    bno.set_operating_mode(bno_055::OperatingMode::NDOF_FMC_OFF)?;

    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let data = bno.reading()?;
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

pub async fn tel0157_task(
    mut heartbeat: broadcast::Receiver<Tick>,
    tel0157_tx: kanal::AsyncSender<Timestamped<tel0157::Tel0157Reading>>,
) -> Result<(), LinuxI2CError> {
    info!("Started TEL0157 task");

    let dev = LinuxI2CDevice::new("/dev/i2c-1", TEL0157_I2C_ADDR as u16)
        .inspect_err(|e| warn!("Failed to open i2c device: {e}"))?;

    let mut tel0157 = tel0157::Tel0157::new(dev)?;

    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let reading = tel0157.reading()?;
                _ = tel0157_tx.send(Timestamped::new(tick, reading)).await;
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

pub async fn bmp280_task(
    mut heartbeat: broadcast::Receiver<Tick>,
    bmp280_tx: kanal::AsyncSender<Timestamped<Labeled<bmp280::Bmp280Reading>>>,
    alt_address: bool,
) {
    let address = if alt_address { 0x77 } else { 0x76 };
    let label = if alt_address { 0x1 } else { 0x0 };

    let mut backoff = Backoff::new(Duration::from_secs(10), Duration::from_hours(1));

    info!("Started BMP280@{address:x} task");

    loop {
        let Ok(device) = LinuxI2CDevice::new("/dev/i2c-1", address)
            .inspect_err(|e| warn!("Failed to create BMP280@{address:x} i2c device: {e}"))
        else {
            tokio::time::sleep(backoff.get()).await;
            backoff.multiply(2);
            continue;
        };

        let Ok(mut bmp280) = bmp280::Bmp280::new(device)
            .inspect_err(|e| warn!("Failed to setup BMP280@{address:x}: {e}"))
        else {
            tokio::time::sleep(backoff.get()).await;
            backoff.multiply(2);
            continue;
        };

        backoff.reset();

        info!("BMP280@{address:x} setup successfully");

        loop {
            match heartbeat.recv().await {
                Ok(tick) => {
                    let Ok(reading) = bmp280
                        .reading()
                        .await
                        .inspect_err(|e| warn!("Failed to get BMP280@{address:x} reading: {e}"))
                    else {
                        break;
                    };
                    _ = bmp280_tx
                        .send(Timestamped::new(tick, Labeled::new(label, reading)))
                        .await;
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
}

pub async fn data_collector(
    channels: RxDataChannels,
    batch_tx: kanal::AsyncSender<DataBatches>,
    fall_tx: kanal::AsyncSender<Timestamped<bno_055::Bno055Reading>>,
    gps_tx: kanal::AsyncSender<Timestamped<tel0157::Tel0157Reading>>,
) {
    info!("Started data collector");

    let mut batched = DataBatches::new();

    let mut send_interval = tokio::time::interval(Duration::from_secs(10));
    send_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = send_interval.tick() => {
                if  0 < batched.total_len() {
                    batch_tx.send(batched.clone()).await.ok();
                    info!("Sent batched data of size: {}", batched.total_len());
                }
                batched.clear();
            }

            Ok(fs_usage) = channels.fs_usage.recv() => {
                batched.fs_usages.push(NewFsUsage::new_from_timestamped(&fs_usage));
            }
            Ok(mem_usage) = channels.mem_usage.recv() => {
                batched.mem_usages.push(NewMemoryUsage::new_from_timestamped(&mem_usage));
            }
            Ok(cpu_temp) = channels.cpu_temp.recv() => {
                batched.cpu_temps.push(NewCpuTemperature::new_from_timestamped(&cpu_temp));
            }
            Ok(bno_reading) = channels.bno_reading.recv() => {
                fall_tx.send(bno_reading.clone()).await.ok();
                batched.bno_readings.push(NewBnoReading::new_from_timestamped(&bno_reading));
            }
            Ok(tel_reading) = channels.tel0157_reading.recv() => {
                gps_tx.send(tel_reading.clone()).await.ok();
                batched.tel0157_readings.push(NewTel0157Reading::new_from_timestamped(&tel_reading))
            }
            Ok(bmp280_reading) = channels.bmp280_reading.recv() => {
                batched.bmp280_readings.push(NewBmp280Reading::new_from_timestamped(&bmp280_reading))
            }
            Ok(as7341_reading) = channels.as7341_reading.recv() => {
                batched.as7341_readings.push(as7341_reading)
            }

        }
    }
}
