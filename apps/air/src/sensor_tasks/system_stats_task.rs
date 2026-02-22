use std::io;
use system_sensors::{
    CpuTemperature, FileSystemUsage, FilesystemUsageInfo, MemoryUsage, MemoryUsageInfo,
};
use tokio::{
    fs::File,
    sync::broadcast::{self, error::RecvError},
};
use tracing::{info, warn};
use uom::si::f64::ThermodynamicTemperature;

use crate::types::{Tick, Timestamped};

pub async fn system_stats_task(
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
