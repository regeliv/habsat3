use crate::{
    db::models::{
        NewBmp280Reading, NewBnoReading, NewCpuTemperature, NewFromTimestamped as _, NewFsUsage,
        NewMemoryUsage, NewTel0157Reading,
    },
    types::{DataBatches, RxDataChannels, Timestamped},
};
use std::time::Duration;
use tracing::info;

mod utils;

pub mod as7341_task;
pub mod bmp280_task;
pub mod bno_task;
pub mod lora_task;
pub mod system_stats_task;
pub mod tel0157_task;

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
