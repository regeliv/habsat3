use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use chrono::{DateTime, Local};
use serde_json::json;
use std::time::Duration;
use tokio::{net::TcpListener, time::Instant};

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
    <script>
        const ws = new WebSocket(`ws://${location.host}/ws`);
        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            if (data.current !== undefined) document.getElementById('current').textContent = data.current;
            if (data.uptime_ms !== undefined) document.getElementById('uptime').textContent = data.uptime_ms;
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
    loop {
        tokio::select! {
            uptime_ms = state.uptime.recv() => {
                let uptime_ms = uptime_ms.unwrap();
                _ = socket.send(Message::Text(json!({"uptime_ms": uptime_ms.as_millis()}).to_string().into())).await;
            }
            time = state.time.recv() => {
                let time = time.unwrap();
                let current = time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                _ = socket.send(Message::Text(json!({"current": current}).to_string().into())).await;
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    uptime: kanal::AsyncReceiver<Duration>,
    time: kanal::AsyncReceiver<DateTime<Local>>,
}

#[tokio::main]
async fn main() {
    let (uptime_tx, uptime_rx) = kanal::unbounded_async();
    let (time_tx, time_rx) = kanal::unbounded_async();

    let state = AppState {
        uptime: uptime_rx,
        time: time_rx,
    };

    tokio::spawn(async move {
        loop {
            _ = time_tx.send(Local::now()).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    tokio::spawn(async move {
        let start = Instant::now();
        loop {
            _ = uptime_tx.send(start.elapsed()).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
