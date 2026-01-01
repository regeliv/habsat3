#!/usr/bin/env python3

from picamera2 import Picamera2
from picamera2.encoders import H264Encoder, Quality
import logging
import time
import zmq
import struct

ctx = zmq.Context()

sock = ctx.socket(zmq.PULL)

camera = Picamera2()

video_config = camera.create_video_configuration()
still_config = camera.create_still_configuration()

camera.configure(still_config)

logger = logging.getLogger(__name__)
logger.setLevel(logging.INFO)

ch = logging.StreamHandler()

formatter = logging.Formatter(
        "%(asctime)s   %(levelname)s: %(message)s",
    datefmt="%Y-%m-%dT%H:%M:%S"
)

ch.setFormatter(formatter)

logger.addHandler(ch)

def recv_and_process():
    request_type, tick = struct.unpack("<Qd", sock.recv())
    if request_type == 0:
        logger.info(f"Received video request for {tick}")
        camera.stop()

        camera.configure(video_config)

        camera.start_and_record_video(f"pics/{tick}.mp4", quality=Quality.HIGH, duration = 30, show_preview = False)

        camera.configure(still_config)
        logger.info(f"Finished video request for {tick}")
    else:
        logger.info(f"Received picture request for {tick}")

        camera.start_and_capture_file(f"pics/{tick}.jpg", show_preview = False);

        logger.info(f"Finished picture request for {tick}")


def main(): 
    sock.connect("ipc:///tmp/camera-events.ipc")
    while True:
        recv_and_process()

main()



