use common::RadioMsg;
use futures::StreamExt as _;
use lora::Sx1276;
use spidev::Spidev;
use tracing::warn;
use uom::si::{angle::degree, length::meter, velocity::meter_per_second};
use zerocopy::IntoBytes as _;

use crate::types::Timestamped;

pub async fn lora_task(
    gps_rx: kanal::AsyncReceiver<Timestamped<tel0157::Tel0157Reading>>,
    key: [u8; 32],
) {
    loop {
        const SPI_FILE: &str = "/dev/spidev0.0";

        let Ok(spi) =
            Spidev::open(SPI_FILE).inspect_err(|e| warn!(".Failed to open SPI file: {e}"))
        else {
            continue;
        };

        let Ok(radio) =
            lora::Sx1276::new(spi).inspect_err(|e| warn!("Failed to initialize radio: {e}"))
        else {
            continue;
        };

        lora_run(radio, gps_rx.clone(), &key).await
    }
}

pub async fn lora_run(
    mut radio: Sx1276,
    gps_rx: kanal::AsyncReceiver<Timestamped<tel0157::Tel0157Reading>>,
    key: &[u8; 32],
) {
    let mut stream = gps_rx.stream();

    while let Some(data) = stream.next().await {
        match radio.get_silicon_version() {
            Ok(0x12) => {}

            Err(e) => {
                warn!("SX1276 liveness check failed: {e}");
                return;
            }
            Ok(invalid_version) => {
                warn!("SX1276 liveness check failed. Invalid version: {invalid_version}");
                return;
            }
        }

        let timestamp = data.timestamp.as_secs();
        let msg = RadioMsg::from(data);

        if let Err(e) = radio.send(msg.encrypt(timestamp, key).as_bytes()) {
            warn!("Failed to send LoRa message: {e}");
            return;
        };
    }
}

impl From<Timestamped<tel0157::Tel0157Reading>> for RadioMsg {
    fn from(value: Timestamped<tel0157::Tel0157Reading>) -> Self {
        Self {
            latitude_degrees: value.data.latitude.get::<degree>(),
            longitude_degrees: value.data.longitude.get::<degree>(),
            course_over_ground_degrees: value.data.course_over_ground.get::<degree>(),
            speed_over_ground_meters_per_second: value
                .data
                .speed_over_ground
                .get::<meter_per_second>(),
            altitude_meters: value.data.altitude.get::<meter>(),
            satellites: value.data.satellites as u64,
        }
    }
}
