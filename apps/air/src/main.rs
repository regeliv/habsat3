use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use bno_055::SensorConfig;
use kanal::{AsyncReceiver, AsyncSender};
use mimalloc::MiMalloc;
use std::{num::NonZero, time::Duration};
use system_sensors::{FilesystemUsageInfo, MemoryUsageInfo};
use tokio::net::TcpListener;
use tokio_util::task::LocalPoolHandle;
use tracing::{Level, info};
use uom::si::f64::ThermodynamicTemperature;

use crate::{
    camera::camera_task,
    heartbeat::Tick,
    sensor_tasks::{Data, RxDataChannels, bno_task, data_collector, system_stats},
};

mod camera;
mod heartbeat;
mod sensor_tasks;

#[global_allocator]
static GLOBAL: MiMalloc = mimalloc::MiMalloc;

async fn index() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html>
<body>
    <h1>Current Server Time:</h1>
    <div id="current" style="font-size: 2em; font-family: monospace;">--:--:--</div>
    <h1>Uptime (ms):</h1>
    <div id="uptime" style="font-size: 2em; font-family: monospace;">{}</div>
    <h1>Acc X</h1>
    <div id="acc_x" style="font-size: 2em; font-family: monospace;"></div>
    <script>
        const ws = new WebSocket(`ws://${location.host}/ws`);
        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            console.log(data);
            if (data.CurrentTime !== undefined) document.getElementById('current').textContent = data.CurrentTime;
            if (data.Uptime !== undefined) document.getElementById('uptime').textContent = data.Uptime;
            if (data.Bno) document.getElementById('acc_x').textContent = data.Bno.acc_x;
        };
    </script>
</body>
</html>
    "#,
    )
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!("Started socket task");
    loop {
        tokio::select! {
            data = state.data.recv() => {
                dbg!(data.unwrap().into_iter().filter(|d| !matches!(d.ty, sensor_tasks::DataType::BnoReadings(_))).collect::<Vec<_>>());
                _ = socket.send(Message::text("foo")).await;
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    data: kanal::AsyncReceiver<Vec<Data>>,
}

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
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let bno_channel = AsyncChannel::<(Tick, bno_055::SensorData)>::new_unbounded();
    let cpu_temp_channel = AsyncChannel::<(Tick, ThermodynamicTemperature)>::new_unbounded();
    let mem_usage_channel = AsyncChannel::<(Tick, MemoryUsageInfo)>::new_unbounded();
    let fs_usage_channel = AsyncChannel::<(Tick, FilesystemUsageInfo)>::new_unbounded();

    let data_channel = AsyncChannel::<Vec<Data>>::new_unbounded();

    let rx_channels = RxDataChannels {
        mem_usage: mem_usage_channel.rx,
        cpu_temp: cpu_temp_channel.rx,
        fs_usage: fs_usage_channel.rx,
        bno_readings: bno_channel.rx,
    };

    let state = AppState {
        data: data_channel.rx,
    };

    let mut ms100_heartbeat = heartbeat::Heartbeat::new(Duration::from_millis(100));

    let rx_every_100ms = ms100_heartbeat.rx_every_n_beats(NonZero::new(1).unwrap());
    let rx_every_10s = ms100_heartbeat.rx_every_n_beats(NonZero::new(100).unwrap());

    let bno_sensor_config = {
        let file = std::fs::read_to_string("bno_sensor_config.json").unwrap();
        serde_json::from_str::<SensorConfig>(&file).unwrap()
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    let pool = LocalPoolHandle::new(1);

    _ = tokio::join!(
        ms100_heartbeat.run(),
        bno_task(bno_sensor_config, rx_every_100ms, bno_channel.tx),
        data_collector(rx_channels, data_channel.tx),
        system_stats(
            rx_every_10s,
            cpu_temp_channel.tx,
            fs_usage_channel.tx,
            mem_usage_channel.tx
        ),
        pool.spawn_pinned(camera_task),
        axum::serve(listener, app)
    );
}
