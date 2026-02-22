use crate::{db::models::NewAs7341Reading, sensor_tasks::utils::Backoff, types::Tick};
use as7341::{
    AS7341_ADDRESS, As7341, ChannelData, DeviceError, PixelConnections, PollingConfig,
    integration_time::TimingSettings,
};
use i2cdev::linux::LinuxI2CDevice;
use kanal::AsyncSender;
use std::time::Duration;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

pub async fn as7341_task(
    heartbeat: broadcast::Receiver<Tick>,
    readings: AsyncSender<NewAs7341Reading>,
) {
    info!("Starting AS7341 task");
    const TIMING_SETTINGS: TimingSettings =
        TimingSettings::new(Duration::from_millis(50), Duration::from_millis(500));

    let mut backoff = Backoff::new(Duration::from_millis(500), Duration::from_secs(5 * 60));

    loop {
        backoff.multiply(2);

        tokio::time::sleep(backoff.get()).await;

        info!("Starting AS7341");

        let Ok(i2c_dev) = LinuxI2CDevice::new("/dev/i2c-1", AS7341_ADDRESS as u16)
            .inspect_err(|e| warn!("Failed to create AS7341 i2c device: {e}"))
        else {
            continue;
        };

        let Ok(mut as7341) =
            as7341::As7341::new(i2c_dev).inspect_err(|e| warn!("Failed to setup AS7341: {e}"))
        else {
            continue;
        };

        if let Err(e) = as7341.set_gain(as7341::Gain::X512) {
            warn!("Failed to setup gain for AS7341: {e}");
            continue;
        }

        let Ok(()) = as7341
            .set_timing(TIMING_SETTINGS)
            .inspect_err(|e| warn!("Failed to set timing parameters for AS7341: {e}"))
        else {
            continue;
        };

        info!("AS7341 setup successfully");

        backoff.reset();
        as7341_run(as7341, heartbeat.resubscribe(), readings.clone()).await;
    }
}

async fn as7341_run(
    mut as7341: As7341<LinuxI2CDevice>,
    mut heartbeat: broadcast::Receiver<Tick>,
    readings: AsyncSender<NewAs7341Reading>,
) {
    const POLLING_CONFIG: PollingConfig = PollingConfig {
        polling_interval: Duration::from_millis(50),
        number_of_intervals: 3,
    };
    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let mut timeout = false;

                let first_batch = match as7341
                    .read_channels(&PixelConnections::f1_f5_nir(), POLLING_CONFIG)
                    .await
                {
                    Err(DeviceError::ChannelTimeout(channels)) => {
                        timeout = true;
                        channels
                    }
                    Ok(channels) => channels,

                    Err(e) => {
                        warn!("Failed to read first batch of channels: {e}");
                        return;
                    }
                };

                let second_batch = match as7341
                    .read_channels(&PixelConnections::f6_f8(), POLLING_CONFIG)
                    .await
                {
                    Err(DeviceError::ChannelTimeout(channels)) => {
                        timeout = true;
                        channels
                    }
                    Ok(channels) => channels,

                    Err(e) => {
                        warn!("Failed to read second batch of channels: {e}");
                        return;
                    }
                };

                let joined_readings =
                    create_reading_from_channels(tick, &first_batch, &second_batch, timeout);

                let _ = readings.send(joined_readings).await.ok();
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

fn create_reading_from_channels(
    timestamp: Tick,
    first_channels: &ChannelData,
    second_channels: &ChannelData,
    timeout: bool,
) -> NewAs7341Reading {
    NewAs7341Reading {
        timestamp: timestamp.as_secs(),
        timeout,
        nm415: first_channels.ch0_data as i32,
        nm445: first_channels.ch1_data as i32,
        nm480: first_channels.ch2_data as i32,
        nm515: first_channels.ch3_data as i32,
        nm555: first_channels.ch4_data as i32,
        nir: first_channels.ch5_data as i32,
        nm590: second_channels.ch0_data as i32,
        nm630: second_channels.ch1_data as i32,
        nm680: second_channels.ch2_data as i32,
    }
}
