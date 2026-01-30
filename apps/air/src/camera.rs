use std::io;
use tracing::{info, warn};

use crate::heartbeat;

pub async fn camera_task() -> io::Result<()> {
    info!("Camera task initialized");

    loop {
        let now = heartbeat::unix_time();
        let filename = format!("pics/{}.mp4", now.as_secs_f64());

        match tokio::process::Command::new("rpicam-vid")
            .args([
                "--width",
                "1920",
                "--height",
                "1080",
                "--timeout",
                "0",
                "--verbose",
                "0",
                "--output",
                &filename,
            ])
            .status()
            .await
        {
            Ok(exit_code) => {
                warn!("Camera process exited with status: {exit_code}. Will restart");
            }
            Err(e) => {
                warn!("Failed to spawn camera process. Will try again: {e}");
            }
        }
    }
}
