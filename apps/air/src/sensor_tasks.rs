use crate::{
    db::models::{
        NewBmp280Reading, NewBnoReading, NewCpuTemperature, NewFromTimestamped as _, NewFsUsage,
        NewMemoryUsage, NewTel0157Reading,
    },
    types::{DataBatches, Labeled, RxDataChannels, Tick, Timestamped},
};
use bno_055::{BNO_055_I2C_ADDR, SensorConfig};
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError};
use std::time::Duration;
use system_sensors::{
    CpuTemperature, FileSystemUsage, FilesystemUsageInfo, MemoryUsage, MemoryUsageInfo,
};
use tel0157::TEL0157_I2C_ADDR;
use tokio::{
    fs::File,
    io,
    sync::broadcast::{self, error::RecvError},
};
use tracing::{info, warn};
use uom::si::f64::ThermodynamicTemperature;

pub async fn system_stats(
    mut heartbeat: broadcast::Receiver<Tick>,
    cpu_temp_tx: kanal::AsyncSender<Timestamped<ThermodynamicTemperature>>,
    fs_usage_tx: kanal::AsyncSender<Timestamped<FilesystemUsageInfo>>,
    mem_usage_tx: kanal::AsyncSender<Timestamped<MemoryUsageInfo>>,
) -> io::Result<()> {
    info!("Started system sensor task");

    let mut cpu_temperature_reader =
        CpuTemperature::new(File::open("/sys/class/thermal/thermal_zone0/temp").await?);
    info!("Initialized CPU temperature reader");

    let filesystem_usage = FileSystemUsage::new(File::open("/").await?);
    info!("Initialized file system usage reader");

    let mut memory_usage_reader = MemoryUsage::new(File::open("/proc/meminfo").await?);
    info!("Initialized memory usage reader");

    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                tokio::join!(
                    send_cpu_temp(&mut cpu_temperature_reader, &cpu_temp_tx, tick),
                    send_memory_usage(&mut memory_usage_reader, &mem_usage_tx, tick),
                    send_fs_usage(&filesystem_usage, &fs_usage_tx, tick),
                );
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

async fn send_cpu_temp(
    cpu_temperature_reader: &mut CpuTemperature,
    out: &kanal::AsyncSender<Timestamped<ThermodynamicTemperature>>,
    tick: Tick,
) {
    match cpu_temperature_reader.read().await {
        Err(e) => {
            warn!("Failed to read CPU temperature: {e} at {}", tick.as_secs());
        }
        Ok(cpu_temp) => {
            _ = out
                .send(Timestamped::new(tick, cpu_temp))
                .await
                .inspect_err(|e| warn!("Failed to send cpu temperature: {e}"))
        }
    }
}

async fn send_fs_usage(
    fs_usage: &FileSystemUsage,
    out: &kanal::AsyncSender<Timestamped<FilesystemUsageInfo>>,
    tick: Tick,
) {
    match fs_usage.get() {
        Err(e) => {
            warn!("Failed to get FS usage: {e} at {}", tick.as_secs());
        }
        Ok(cpu_temp) => {
            _ = out
                .send(Timestamped::new(tick, cpu_temp))
                .await
                .inspect_err(|e| warn!("Failed to send file system usage: {e}"))
        }
    }
}

async fn send_memory_usage(
    mem_usage: &mut MemoryUsage,
    out: &kanal::AsyncSender<Timestamped<MemoryUsageInfo>>,
    tick: Tick,
) {
    match mem_usage.read().await {
        Err(e) => {
            warn!(
                "Failed to get memory usage: {e} at {}",
                tick.unix_time.as_secs_f64()
            );
        }
        Ok(cpu_temp) => {
            _ = out
                .send(Timestamped::new(tick, cpu_temp))
                .await
                .inspect_err(|e| warn!("Failed to send memory usage: {e}"))
        }
    }
}

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

struct Backoff {
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
                batched.tel0157_readings.push(NewTel0157Reading::new_from_timestamped(&tel_reading))
            }
            Ok(bmp280_reading) = channels.bmp280_reading.recv() => {
                batched.bmp280_readings.push(NewBmp280Reading::new_from_timestamped(&bmp280_reading))
            }

        }
    }
}
