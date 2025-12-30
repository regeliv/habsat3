use libcamera::{
    camera::{ActiveCamera, CameraConfigurationStatus},
    camera_manager::CameraManager,
    framebuffer::{AsFrameBuffer, FrameMetadataRef, FrameMetadataStatus},
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    geometry::Size,
    pixel_format::PixelFormat,
    request::ReuseFlag,
    stream::{Stream, StreamRole},
    utils::Immutable,
};
use std::{io, str::FromStr, time::Duration};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{debug, error, info, warn};
use turbojpeg::YuvImage;

use crate::heartbeat::Tick;

struct PhotoCapture<'a> {
    #[expect(
        unused,
        reason = "White it's not used directly, we cannot drop it if we want the libary to function"
    )]
    camera_manager: CameraManager,
    camera: ActiveCamera<'a>,

    stream: libcamera::stream::Stream,

    available_request: Option<libcamera::request::Request>,
    request_complete_rx: kanal::AsyncReceiver<libcamera::request::Request>,

    picture_size: Size,
}

impl<'a> PhotoCapture<'a> {
    pub fn new() -> std::io::Result<Self> {
        let camera_manager = CameraManager::new()?;

        // There is a lot of bureacracy involed in configuring a camera and the Rust interface
        // libcamera is very awkward (https://github.com/lit-robotics/libcamera-rs/issues/53), so
        // the following code is, admittedly, quite ugly.
        //
        // One of the issues is that almost every field is behind an optional, likely due to the
        // underlying C++ library using pointers instead of references. While, quite possibly, in
        // our use case the pointers are always valid, having unwrap on every such optional is unwise,
        // given the unclear semantics of libcamera. This forces us to convert every optional into
        // io::Result for better debugging and comptability with return values.

        let mut active_camera = camera_manager
            .cameras()
            .get(0)
            .ok_or_else(|| {
                warn!("Camera not found");
                io::Error::from(io::ErrorKind::NotFound)
            })
            .and_then(|c| c.acquire())
            .inspect_err(|e| warn!("Failed to acquire camera: {e}"))?;

        let mut config = active_camera
            .generate_configuration(&[StreamRole::StillCapture])
            .ok_or_else(|| {
                warn!("Failed to generate camera configuration");
                io::Error::from(io::ErrorKind::Other)
            })?;

        config
            .get_mut(0)
            .expect("Stream was just generated. It must be there.")
            .set_pixel_format(PixelFormat::from_str("YUYV").expect("YUYV is valid fourcc format"));

        match config.validate() {
            CameraConfigurationStatus::Adjusted => {
                warn!("Camera configuration was adjusted")
            }
            CameraConfigurationStatus::Invalid => {
                warn!("Generated camera configuration was invalid. Configuration: {config:?}");
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            _ => {}
        }

        active_camera
            .configure(&mut config)
            .inspect_err(|e| warn!("Failed to configure camera: {e}. Config was: {config:?}"))?;

        let picture_size = {
            let config = config
                .get(0)
                .expect("Stream was just configured, it must be there");

            config.get_size()
        };

        info!("Camera configured successfully with config: {config:?}");

        let mut framebuffer_allocator = FrameBufferAllocator::new(&active_camera);
        let stream = config
            .get(0)
            .expect("Stream was just generated. It must be there.")
            .stream()
            .expect("Configuration was just applied, therefore the stream exists and is valid");

        let buffers = framebuffer_allocator
            .alloc(&stream)
            .inspect_err(|e| warn!("Failed to allocate framebuffers: {e}"))?;

        let mut buffers = buffers
            .into_iter()
            .map(MemoryMappedFrameBuffer::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                warn!("Failed to created mmaped framebuffers: {e}");
                io::Error::from(io::ErrorKind::Other)
            })?;

        let (request_complete_tx, request_complete_rx) = kanal::unbounded_async();
        let request_complete_tx = request_complete_tx.to_sync();

        active_camera.on_request_completed(move |req| {
            _ = request_complete_tx
                .send(req)
                .inspect_err(|e| warn!("Failed to send camera request completion: {e}"));
        });

        let mut capture_request = active_camera.create_request(None).ok_or_else(|| {
            warn!("Failed to create caputer request");
            io::Error::from(io::ErrorKind::Other)
        })?;

        capture_request
            .add_buffer(
                &stream,
                buffers
                    .pop()
                    .expect("There should be at least one buffer available"),
            )
            .inspect_err(|e| warn!("Failed to add buffer to a capture request: {e}"))?;

        active_camera
            .start(None)
            .inspect_err(|e| warn!("Failed to start camera: {e}"))?;

        Ok(Self {
            camera_manager,
            camera: active_camera,
            available_request: Some(capture_request),
            stream,
            request_complete_rx,
            picture_size,
        })
    }

    async fn do_request(&mut self) -> io::Result<()> {
        let mut request = self
            .available_request
            .take()
            .expect("The request is there. Only between this `take` and the following reassignments it's empty.");

        request.reuse(ReuseFlag::REUSE_BUFFERS);

        match self.camera.queue_request(request) {
            Err((returned_request, error)) => {
                self.available_request = Some(returned_request);

                Err(error)
            }
            Ok(()) => {
                let request = self
                    .request_complete_rx
                    .recv()
                    .await
                    .expect("Channel must not be closed while a request is live");

                self.available_request = Some(request);
                Ok(())
            }
        }
    }

    fn get_framebuffer<'r>(
        req: &'r libcamera::request::Request,
        stream: &Stream,
    ) -> io::Result<&'r MemoryMappedFrameBuffer<FrameBuffer>> {
        req.buffer(stream).ok_or_else(|| {
            warn!("Failed to access buffer of a capture request");
            io::Error::from(io::ErrorKind::InvalidData)
        })
    }

    fn get_frame_metadata<'r>(
        req: &'r libcamera::request::Request,
        stream: &Stream,
    ) -> io::Result<Immutable<FrameMetadataRef<'r>>> {
        let framebuffer = PhotoCapture::get_framebuffer(req, stream)?;

        let metadata = framebuffer.metadata().ok_or_else(|| {
            warn!("Failed to access metadata of a capture request");
            io::Error::from(io::ErrorKind::InvalidData)
        })?;

        Ok(metadata)
    }

    fn get_planes<'r>(
        req: &'r libcamera::request::Request,
        stream: &Stream,
    ) -> io::Result<Vec<&'r [u8]>> {
        PhotoCapture::get_framebuffer(req, stream).map(|fb| fb.data())
    }

    async fn setup_camera(&mut self) -> io::Result<()> {
        const FUEL: usize = 20;

        for _ in 0..FUEL {
            self.do_request().await?;
            let request = self.get_request();
            let metadata = PhotoCapture::get_frame_metadata(request, &self.stream)?;

            debug!("Camera setup frame status: {:?}", metadata.status());

            match metadata.status() {
                FrameMetadataStatus::Success => return Ok(()),
                // If we the status is cancelled, the camera likely has been disconnected, we abort
                // early in that case, there is no point in continuing
                FrameMetadataStatus::Cancelled => {
                    error!("Camera is likely disconnected, aborting setup");
                    return Err(io::Error::from(io::ErrorKind::NotConnected));
                }

                // `Error` status is normal during camera startup: first few frames return `Error`,
                // the next bunch returns `Startup` and only after these frames start to return `Success`.
                // So we keep taking frames, until we finish the startup process. To prevent
                // endless loops upon some hardware error, we use up fuel
                FrameMetadataStatus::Error | FrameMetadataStatus::Startup => continue,
            }
        }

        warn!("Failed to setup camera. Run out of fuel");
        Err(io::Error::from(io::ErrorKind::TimedOut))
    }

    async fn take_picture(&mut self) -> io::Result<&[u8]> {
        self.do_request().await?;

        let metadata = PhotoCapture::get_frame_metadata(self.get_request(), &self.stream)?;

        match metadata.status() {
            FrameMetadataStatus::Success => {}
            _ => self.setup_camera().await?,
        }

        let plane = PhotoCapture::get_planes(self.get_request(), &self.stream)?[0];
        let plane_meta = PhotoCapture::get_frame_metadata(self.get_request(), &self.stream)?
            .planes()
            .get(0)
            .expect("We've got reference to first plane, therefore metadata about it must exist");

        Ok(&plane[..plane_meta.bytes_used as usize])
    }

    fn get_request(&self) -> &libcamera::request::Request {
        self.available_request.as_ref().expect("Request is always returned in `do_request`, therefore it's always available from code outside of that function")
    }

    fn get_picture_size(&self) -> Size {
        self.picture_size
    }
}

fn yuyv_to_yuv422_planar(yuyv_data: &[u8], width: usize, height: usize, out: &mut Vec<u8>) {
    let total_pixels = width * height;

    let y_plane_size = total_pixels;
    let uv_plane_size = total_pixels / 2;

    let total_len = y_plane_size + 2 * uv_plane_size;

    out.resize(total_len, 0);

    let (y_plane, rest) = out.split_at_mut(y_plane_size);
    let (u_plane, v_plane) = rest.split_at_mut(uv_plane_size);

    for (i, chunk) in yuyv_data.chunks_exact(4).enumerate() {
        y_plane[i * 2] = chunk[0];
        y_plane[i * 2 + 1] = chunk[2];

        u_plane[i] = chunk[1];

        v_plane[i] = chunk[3];
    }
}

pub async fn camera_task(mut heartbeat: broadcast::Receiver<Tick>) -> io::Result<()> {
    let mut camera = PhotoCapture::new()?;

    let mut compressor = turbojpeg::Compressor::new()
        .inspect_err(|e| warn!("Failed to create jpeg compressor: {e}"))
        .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

    compressor
        .set_quality(80)
        .expect("80 is between 1 and 100, therefore it's valid quality value");

    let mut yuv422_planar_buf = Vec::new();

    let picture_size = camera.get_picture_size();

    let mut jpg_buf = {
        let buf_size = compressor
            .buf_len(picture_size.width as usize, picture_size.height as usize)
            .expect("We don't expect overflow here");

        vec![0; buf_size]
    };

    let pictures_directory = "pics";

    tokio::fs::create_dir_all(pictures_directory)
        .await
        .inspect_err(|e| warn!("Failed to create pics directory: {e}"))?;

    loop {
        match heartbeat.recv().await {
            Ok(tick) => {
                let pic = match camera.take_picture().await {
                    Ok(pic) => pic,

                    // Camera getting disconnected at this point is extremly bad for us: if we try
                    // to destroy libcamera state, it will segfault, if we keep probing for cameras,
                    // the Rust wrapper will `unwrap` and thus panic. Therefore, upon disconnection,
                    // we exit this loop and effectively hibernate this task by looping forever with
                    // very long sleeps
                    Err(e) if e.kind() == io::ErrorKind::NotConnected => break,
                    // Other errors are fine
                    Err(_) => continue,
                };

                yuyv_to_yuv422_planar(
                    pic,
                    picture_size.width as usize,
                    picture_size.height as usize,
                    &mut yuv422_planar_buf,
                );

                let image = YuvImage {
                    pixels: yuv422_planar_buf.as_slice(),
                    width: picture_size.width as usize,
                    align: 2,
                    height: picture_size.height as usize,
                    subsamp: turbojpeg::Subsamp::Sub2x1,
                };

                let compressed_size =
                    match compressor.compress_yuv_to_slice(image, jpg_buf.as_mut_slice()) {
                        Ok(size) => size,
                        Err(e) => {
                            warn!("Failed to compress YUV into JPG: {e}");
                            continue;
                        }
                    };

                let time = tick.unix_time.as_secs_f64();
                let file_path = format!("{pictures_directory}/{time}.jpg");

                match tokio::fs::write(&file_path, &jpg_buf[..compressed_size]).await {
                    Ok(()) => info!("Saved picture to {file_path}"),
                    Err(e) => warn!("Failed to save picture to {file_path}: {e}"),
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

    // If we get here, the camera task is dead. Thus there is no reason to take up any
    // heartbeat bandwidth
    drop(heartbeat);

    loop {
        tokio::time::sleep(Duration::from_hours(99999)).await;
    }
}
