use bno_055::SensorConfig;
use kanal::{AsyncReceiver, AsyncSender};
use std::{io::stdout, sync::Mutex, time::Duration};
use system_sensors::{FilesystemUsageInfo, MemoryUsageInfo};
use tokio_util::{sync::CancellationToken, task::LocalPoolHandle};
use tracing::{Level, info};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use uom::si::f64::ThermodynamicTemperature;

use crate::{
    camera::camera_task,
    db::models::NewAs7341Reading,
    sensor_tasks::{
        as7341_task::as7341_task, bmp280_task::bmp280_task, bno_task::bno_task, data_collector,
        lora_task::lora_task, system_stats_task::system_stats_task, tel0157_task::tel0157_task,
    },
    tape_control::{fall_detector, tape_control},
    types::{DataBatches, Labeled, RxDataChannels, Timestamped},
};

mod camera;
mod db;
mod heartbeat;
mod sensor_tasks;
mod tape_control;
mod types;

struct AsyncChannel<T> {
    pub tx: AsyncSender<T>,
    pub rx: AsyncReceiver<T>,
}

impl<T> AsyncChannel<T> {
    pub fn new_unbounded() -> Self {
        let (tx, rx) = kanal::unbounded_async();
        Self { tx, rx }
    }
}

#[tokio::main]
async fn main() {
    let log_file =
        std::fs::File::create_new(format!("logs/{}.log", heartbeat::unix_time().as_secs_f64()))
            .expect("Log file creation must succeed");

    let file_and_stdout = Mutex::new(log_file).and(|| Box::new(stdout()));

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_writer(file_and_stdout)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    info!("Application started");

    let bno_channel = AsyncChannel::<Timestamped<bno_055::Bno055Reading>>::new_unbounded();
    let fall_data_channel = AsyncChannel::<Timestamped<bno_055::Bno055Reading>>::new_unbounded();
    let cpu_temp_channel = AsyncChannel::<Timestamped<ThermodynamicTemperature>>::new_unbounded();
    let mem_usage_channel = AsyncChannel::<Timestamped<MemoryUsageInfo>>::new_unbounded();
    let fs_usage_channel = AsyncChannel::<Timestamped<FilesystemUsageInfo>>::new_unbounded();
    let tel0157_reading_channel =
        AsyncChannel::<Timestamped<tel0157::Tel0157Reading>>::new_unbounded();
    let gps_channel = AsyncChannel::<Timestamped<tel0157::Tel0157Reading>>::new_unbounded();
    let bmp280_reading_channel =
        AsyncChannel::<Timestamped<Labeled<bmp280::Bmp280Reading>>>::new_unbounded();
    let as7341_reading_channel = AsyncChannel::<NewAs7341Reading>::new_unbounded();

    let batch_channel = AsyncChannel::<DataBatches>::new_unbounded();

    let rx_channels = RxDataChannels {
        mem_usage: mem_usage_channel.rx,
        cpu_temp: cpu_temp_channel.rx,
        fs_usage: fs_usage_channel.rx,
        bno_reading: bno_channel.rx,
        tel0157_reading: tel0157_reading_channel.rx,
        bmp280_reading: bmp280_reading_channel.rx,
        as7341_reading: as7341_reading_channel.rx,
    };

    let fall_cancellation_token = CancellationToken::new();
    let fall_cancellation_child = fall_cancellation_token.child_token();

    let mut ms100_heartbeat = heartbeat::Heartbeat::new(Duration::from_millis(100));

    let rx_every_100ms = ms100_heartbeat.rx_every(Duration::from_millis(100));
    let rx_every_2s = ms100_heartbeat.rx_every(Duration::from_secs(2));
    let rx_every_10s = ms100_heartbeat.rx_every(Duration::from_secs(10));

    let bno_sensor_config = {
        let file = std::fs::read_to_string("bno_sensor_config.json").unwrap();
        serde_json::from_str::<SensorConfig>(&file).unwrap()
    };

    let key: [u8; 32] = {
        let key = tokio::fs::read("lora_encryption_key")
            .await
            .expect("Failed to read `lora_encryption_key` file");

        key.try_into()
            .expect("LoRa key must contain exactly 32 bytes")
    };

    let tape_extension_delay = {
        const TAPE_DELAY_FILE: &str = "tape_extension_delay_seconds";
        let delay = tokio::fs::read_to_string(&TAPE_DELAY_FILE)
            .await
            .unwrap_or_else(|_| panic!("Failed to read `{TAPE_DELAY_FILE}`"));

        let delay = delay.parse::<u64>().expect("Failed to parse tape extension delay. The file cannot contain trailing whitespace and must contain a non-negative integer");

        Duration::from_secs(delay)
    };

    // I2C failures seem to interact badly with async filesystem operations on the same thread,
    // namely they seem to cause file `open` and file `read` to never be polled, thus blocking
    // tasks depending on these operations. Interestingly, async `interval` continues to run
    // suggesting that it is not just a simple runtime block causing this lock of polling.
    // More interestingly, the issue seems to disappear on musl.
    //
    // Therefore, we move i2c tasks to a different thread using `LocalPoolHandle`. We don't use
    // `tokio::spawn`, because it does not guarantee that a task gets spawned (and polled) on a
    // different thread due to tokio's work-stealing nature
    let i2c_pool = LocalPoolHandle::new(1);

    let db_pool = LocalPoolHandle::new(1);

    _ = tokio::join!(
        db_pool.spawn_pinned(|| db::db_task(batch_channel.rx)),
        ms100_heartbeat.run(),
        i2c_pool.spawn_pinned(|| bno_task(bno_sensor_config, rx_every_100ms, bno_channel.tx)),
        i2c_pool.spawn_pinned({
            let rx_every_2s = rx_every_2s.resubscribe();
            || as7341_task(rx_every_2s, as7341_reading_channel.tx)
        }),
        i2c_pool.spawn_pinned({
            let rx_every_2s = rx_every_2s.resubscribe();
            || tel0157_task(rx_every_2s, tel0157_reading_channel.tx)
        }),
        i2c_pool.spawn_pinned({
            let rx_every_2s = rx_every_2s.resubscribe();
            let bmp280_tx = bmp280_reading_channel.tx.clone();
            || bmp280_task(rx_every_2s, bmp280_tx, false)
        }),
        i2c_pool.spawn_pinned(|| bmp280_task(rx_every_2s, bmp280_reading_channel.tx, true)),
        i2c_pool.spawn_pinned(move || lora_task(gps_channel.rx, key)),
        fall_detector(fall_data_channel.rx, fall_cancellation_token),
        tape_control(fall_cancellation_child, tape_extension_delay),
        data_collector(
            rx_channels,
            batch_channel.tx,
            fall_data_channel.tx,
            gps_channel.tx
        ),
        system_stats_task(
            rx_every_10s.resubscribe(),
            cpu_temp_channel.tx,
            fs_usage_channel.tx,
            mem_usage_channel.tx
        ),
        camera_task(),
    );
}
