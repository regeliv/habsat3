use crate::heartbeat::Tick;
use bno_055::{BNO_055_I2C_ADDR, SensorConfig};
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError};
use std::time::Duration;
use system_sensors::{
    CpuTemperature, FileSystemUsage, FilesystemUsageInfo, MemoryUsage, MemoryUsageInfo,
};
use tokio::{
    fs::File,
    io,
    sync::broadcast::{self, error::RecvError},
};
use tracing::{info, warn};
use uom::si::f64::ThermodynamicTemperature;

pub async fn system_stats(
    mut heartbeat: broadcast::Receiver<Tick>,
    cpu_temp_tx: kanal::AsyncSender<(Tick, ThermodynamicTemperature)>,
    fs_usage_tx: kanal::AsyncSender<(Tick, FilesystemUsageInfo)>,
    mem_usage_tx: kanal::AsyncSender<(Tick, MemoryUsageInfo)>,
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
    out: &kanal::AsyncSender<(Tick, ThermodynamicTemperature)>,
    tick: Tick,
) {
    match cpu_temperature_reader.read().await {
        Err(e) => {
            warn!(
                "Failed to read CPU temperature: {e} at {}",
                tick.unix_time.as_secs_f64()
            );
        }
        Ok(cpu_temp) => {
            _ = out
                .send((tick, cpu_temp))
                .await
                .inspect_err(|e| warn!("Failed to send cpu temperature: {e}"))
        }
    }
}

async fn send_fs_usage(
    fs_usage: &FileSystemUsage,
    out: &kanal::AsyncSender<(Tick, FilesystemUsageInfo)>,
    tick: Tick,
) {
    match fs_usage.get() {
        Err(e) => {
            warn!(
                "Failed to get FS usage: {e} at {}",
                tick.unix_time.as_secs_f64()
            );
        }
        Ok(cpu_temp) => {
            _ = out
                .send((tick, cpu_temp))
                .await
                .inspect_err(|e| warn!("Failed to send file system usage: {e}"))
        }
    }
}

async fn send_memory_usage(
    mem_usage: &mut MemoryUsage,
    out: &kanal::AsyncSender<(Tick, MemoryUsageInfo)>,
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
                .send((tick, cpu_temp))
                .await
                .inspect_err(|e| warn!("Failed to send memory usage: {e}"))
        }
    }
}

pub async fn bno_task(
    bno_sensor_config: SensorConfig,
    mut heartbeat: broadcast::Receiver<Tick>,
    bno_tx: kanal::AsyncSender<(Tick, bno_055::SensorData)>,
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
                let data = bno.get_sensor_data()?;
                //info!("BNO-055 got sensor data {:?}", data);
                _ = bno_tx.send((tick, data)).await;
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

pub struct RxDataChannels {
    pub mem_usage: kanal::AsyncReceiver<(Tick, MemoryUsageInfo)>,
    pub cpu_temp: kanal::AsyncReceiver<(Tick, ThermodynamicTemperature)>,
    pub fs_usage: kanal::AsyncReceiver<(Tick, FilesystemUsageInfo)>,
    pub bno_readings: kanal::AsyncReceiver<(Tick, bno_055::SensorData)>,
}

#[derive(Debug, Clone)]
pub struct Data {
    #[expect(dead_code)]
    pub tick: Tick,
    pub ty: DataType,
}

#[derive(Debug, Clone)]
pub enum DataType {
    #[expect(dead_code)]
    FsUsage(FilesystemUsageInfo),
    #[expect(dead_code)]
    CpuTemp(ThermodynamicTemperature),
    #[expect(dead_code)]
    MemUsage(MemoryUsageInfo),
    #[expect(dead_code)]
    BnoReadings(bno_055::SensorData),
}

pub async fn data_collector(channels: RxDataChannels, batch_tx: kanal::AsyncSender<Vec<Data>>) {
    info!("Started data collector");
    let mut batch = Vec::<Data>::with_capacity(200);

    let mut send_interval = tokio::time::interval(Duration::from_secs(5));
    send_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = send_interval.tick() => {
                _ = batch_tx.send(batch.clone()).await;
                warn!("Sent batch of size {}", batch.len());
                batch.clear();
            }

            fs_usage = channels.fs_usage.recv() => {
                if let Ok(fs_usage) = fs_usage { batch.push(Data { tick: fs_usage.0, ty: DataType::FsUsage(fs_usage.1) }) }
            }
            mem_usage = channels.mem_usage.recv() => {
                if let Ok(mem_usage) = mem_usage { batch.push(Data { tick: mem_usage.0, ty: DataType::MemUsage(mem_usage.1) }) }
            }
            cpu_temp = channels.cpu_temp.recv() => {
                if let Ok(cpu_temp) = cpu_temp { batch.push(Data { tick: cpu_temp.0, ty: DataType::CpuTemp(cpu_temp.1) }) }
            }

            bno_readings = channels.bno_readings.recv() => {
                if let Ok(bno_readings) = bno_readings {
                    batch.push(Data { tick: bno_readings.0, ty: DataType::BnoReadings(bno_readings.1.clone()) });
                }
            }

        }
    }
}
