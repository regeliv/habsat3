use futures::StreamExt;
use std::io;
use tokio::select;
use tokio::{
    process::Command,
    sync::broadcast::{self, error::RecvError},
};
use tracing::{info, warn};
use zerocopy::IntoBytes;
use zeromq::SocketEvent;
use zeromq::{Socket as _, SocketSend};

#[repr(u64)]
#[derive(Debug, zerocopy::IntoBytes, zerocopy::Immutable)]
enum RequestType {
    Video = 0,
    Picture = 1,
}

#[derive(Debug, zerocopy::IntoBytes, zerocopy::Immutable)]
#[expect(unused, reason = "They are read by the client")]
struct CameraRequest {
    request_type: RequestType,
    tick: f64,
}

use crate::types::Tick;

pub async fn camera_task(mut heartbeat: broadcast::Receiver<Tick>) -> io::Result<()> {
    let pictures_directory = "pics";

    tokio::fs::create_dir_all(pictures_directory)
        .await
        .inspect_err(|e| warn!("Failed to create pics directory: {e}"))?;

    let mut sender = zeromq::PushSocket::new();
    _ = tokio::fs::remove_file("/tmp/camera-events.ipc").await;

    let mut monitor = sender.monitor();

    sender
        .bind("ipc:///tmp/camera-events.ipc")
        .await
        .inspect_err(|e| warn!("Failed to create zeromq IPC file: {e}"))
        .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

    let mut i = 0usize;

    loop {
        select! {
            Some(socket_event) = monitor.next() => {
                match socket_event {
                    SocketEvent::Connected(_, _) => {
                        info!("Camera client connected");
                    },
                    SocketEvent::Disconnected(_) => {
                        warn!("Python task is dead, respawning");
                        Command::new("./camera.py").spawn().inspect_err(|e| warn!("Failed to respawn python task: {e}")).ok();
                    },
                    SocketEvent::Listening(_) => {
                        info!("Zmq socket started, spawning python task");
                        Command::new("./camera.py").spawn().inspect_err(|e| warn!("Failed to spawn python task: {e}")).ok();
                    }
                    _ => {}
                }
            }

            beat = heartbeat.recv() => {
                match beat {
                    Ok(tick) => {
                        let req = CameraRequest {
                            request_type: if i.is_multiple_of(2) {
                                RequestType::Video
                            } else {
                                RequestType::Picture
                            },
                            tick: tick.as_secs(),
                        };

                        i += 1;
                        match sender.send(req.as_bytes().to_owned().into()).await {
                            Ok(_) => info!("Queued camera action"),
                            Err(e) => warn!("Failed to queue camera action: {e:?}"),
                        }
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
    }
}
