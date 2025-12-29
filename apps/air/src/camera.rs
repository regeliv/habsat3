use std::time::Duration;

use libcamera::{camera_manager::CameraManager, logging::LoggingLevel, stream::StreamRole};

pub async fn camera_task() {
    let mgr = CameraManager::new().unwrap();

    mgr.log_set_level("Camera", LoggingLevel::Warn);
    let cameras = mgr.cameras();

    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        for cam in cameras.iter() {
            println!();
            println!("ID: {}", cam.id());

            println!("Properties: {:#?}", cam.properties());

            println!("Controls: {:#?}", cam.controls());

            let config = cam
                .generate_configuration(&[StreamRole::ViewFinder])
                .unwrap();
            let view_finder_cfg = config.get(0).unwrap();
            println!("Available formats: {:#?}", view_finder_cfg.formats());
        }

        interval.tick().await;
    }
}
