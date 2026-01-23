use std::time::Duration;
use tape::Tape;
use tokio::{join, sync::Mutex};
use tokio_util::{future::FutureExt, sync::CancellationToken};
use tracing::{info, warn};

use crate::types::Timestamped;

pub async fn fall_detector(
    fall_data_rx: kanal::AsyncReceiver<Timestamped<bno_055::Bno055Reading>>,
    fall_cancellation_token: CancellationToken,
) {
    loop {
        if let Ok(reading) = fall_data_rx.recv().await
            && (reading.data.acc_y < 600 || 60000 < reading.data.acc_y)
        {
            warn!("Fall detected at {}", reading.timestamp.as_secs());

            fall_cancellation_token.cancel();
        }
    }
}

pub async fn tape_control(
    fall_cancellation_token: CancellationToken,
    extension_delay: Duration,
) -> tokio::io::Result<()> {
    let mut tape = tape::Tape::new()
        .await
        .inspect_err(|e| warn!("Failed to setup tape: {e}"))?;

    info!("Tape control initialized");

    tape.retract()
        .await
        .inspect_err(|e| warn!("Initial tape retraction failed: {e}"))?;

    let tape = tokio::sync::Mutex::new(tape);

    let extend_or_cancel =
        extend(&tape, extension_delay).with_cancellation_token(&fall_cancellation_token);

    let retract = retract(&tape, &fall_cancellation_token);

    _ = join!(extend_or_cancel, retract);

    Ok(())
}

async fn extend(tape: &Mutex<Tape>, extension_delay: Duration) -> tokio::io::Result<()> {
    tokio::time::sleep(extension_delay).await;

    tape.lock()
        .await
        .extend()
        .await
        .inspect_err(|e| warn!("Failed to extend tape: {e}"))?;
    info!("Tape extended");

    Ok(())
}

async fn retract(tape: &Mutex<Tape>, token: &CancellationToken) -> tokio::io::Result<()> {
    token.cancelled().await;
    tape.lock()
        .await
        .retract()
        .await
        .inspect_err(|e| warn!("Focred tape retraction failed: {e}"))?;

    info!("Tape retracted");
    Ok(())
}
