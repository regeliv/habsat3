use crate::{
    db::models::NewAs7341Reading,
    sensor_tasks::utils::{Backoff, BackoffReset},
    types::Tick,
};
use as7341::{
    AS7341_ADDRESS, As7341, ChannelData, DeviceError, PixelConnections, PollingConfig,
    integration_time::TimingSettings,
};
use i2cdev::linux::LinuxI2CDevice;
use kanal::AsyncSender;
use std::{ops::ControlFlow, time::Duration};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

pub async fn as7341_task(
    mut heartbeat: broadcast::Receiver<Tick>,
    readings: AsyncSender<NewAs7341Reading>,
) {
    const TIMING_SETTINGS: TimingSettings =
        TimingSettings::new(Duration::from_millis(50), Duration::from_millis(500));

    const POLLING_CONFIG: PollingConfig = PollingConfig {
        polling_interval: Duration::from_millis(50),
        number_of_intervals: 3,
    };

    let backoff = Backoff::new(Duration::from_secs(10), Duration::from_hours(1));

    let mut backoff_reset = BackoffReset {
        backoff,
        reset: async || {
            info!("Starting AS7341");
            let i2c_dev = match LinuxI2CDevice::new("/dev/i2c-1", AS7341_ADDRESS as u16) {
                Err(e) => {
                    warn!("Failed to create AS7341 i2c device: {e}");
                    return ControlFlow::Continue(());
                }

                Ok(dev) => dev,
            };

            let mut as7341 = match as7341::As7341::new(i2c_dev) {
                Err(e) => {
                    warn!("Failed to setup AS7341: {e}");
                    return ControlFlow::Continue(());
                }

                Ok(dev) => dev,
            };

            if let Err(e) = as7341.set_gain(as7341::Gain::X512) {
                warn!("Failed to setup gain for AS7341: {e}");
                return ControlFlow::Continue(());
            }

            match as7341.set_timing(TIMING_SETTINGS) {
                Err(e) => {
                    warn!("Failed to set timing parameters for AS7341: {e}");
                    ControlFlow::Continue(())
                }

                Ok(()) => {
                    info!("AS7341 successfully setup");
                    ControlFlow::Break(as7341)
                }
            }
        },

        data_loop: async |as7341: &mut As7341<LinuxI2CDevice>| loop {
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
        },
    };

    backoff_reset.run().await;
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
