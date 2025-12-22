use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use bno_055::{BNO_055_I2C_ADDR, SensorConfig, SensorData};
use chrono::{DateTime, Local};
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError};
use std::{num::NonZero, time::Duration};
use tokio::{
    net::TcpListener,
    sync::broadcast::{self, error::RecvError},
    time::Instant,
};
use tracing::{info, warn};

use crate::heartbeat::Tick;

mod heartbeat;

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

#[derive(Debug, serde::Serialize)]
#[allow(dead_code)]
enum ServerMessage {
    Uptime(u128),
    CurrentTime(String),
    Bno(SensorData),
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!("Started socket task");
    loop {
        tokio::select! {
            uptime_ms = state.uptime.recv() => {
                let uptime_ms = uptime_ms.unwrap();
                _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Uptime(uptime_ms.as_millis())).unwrap().into())).await;
            }
            time = state.time.recv() => {
                let time = time.unwrap();
                let current = time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::CurrentTime(current)).unwrap().into())).await;
            }
            bno_readings = state.bno_readings.recv() => {
                let bno_readings = bno_readings.unwrap();
                _ = socket.send(serde_json::to_string(&ServerMessage::Bno(bno_readings)).unwrap().into()).await;
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    uptime: kanal::AsyncReceiver<Duration>,
    time: kanal::AsyncReceiver<DateTime<Local>>,
    bno_readings: kanal::AsyncReceiver<bno_055::SensorData>,
}

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let (uptime_tx, uptime_rx) = kanal::unbounded_async();
    let (time_tx, time_rx) = kanal::unbounded_async();
    let (bno_tx, bno_rx) = kanal::unbounded_async();

    let state = AppState {
        uptime: uptime_rx,
        time: time_rx,
        bno_readings: bno_rx,
    };

    let mut ms100_heartbeat = heartbeat::Heartbeat::new(Duration::from_millis(100));
    let rx_every_100ms = ms100_heartbeat.rx_every_n_beats(NonZero::new(1).unwrap());

    tokio::spawn(async move { ms100_heartbeat.run().await });

    tokio::spawn(async move {
        info!("Started server time task");
        loop {
            _ = time_tx.send(Local::now()).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    tokio::spawn(async move {
        info!("Started uptime task");
        let start = Instant::now();
        loop {
            _ = uptime_tx.send(start.elapsed()).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    let bno_sensor_config = {
        let file = std::fs::read_to_string("bno_sensor_config.json").unwrap();
        serde_json::from_str::<SensorConfig>(&file).unwrap()
    };

    tokio::spawn(bno_task(bno_sensor_config, rx_every_100ms, bno_tx));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn bno_task(
    bno_sensor_config: SensorConfig,
    mut heartbeat: broadcast::Receiver<Tick>,
    bno_tx: kanal::AsyncSender<SensorData>,
) -> Result<(), LinuxI2CError> {
    info!("Started BNO-055 task");

    let dev = LinuxI2CDevice::new("/dev/i2c-1", BNO_055_I2C_ADDR as u16)?;

    let mut bno = bno_055::Bno055::new(dev)?;
    info!("BNO-055 created");

    bno.set_sensor_config(&bno_sensor_config)?;
    info!("BNO-055 config set to {:?}", bno_sensor_config);

    bno.set_operating_mode(bno_055::OperatingMode::NDOF_FMC_OFF)?;

    loop {
        match heartbeat.recv().await {
            Ok(_) => {
                let data = bno.get_sensor_data()?;
                info!("BNO-055 got sensor data {:?}", data);
                _ = bno_tx.send(data).await;
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
