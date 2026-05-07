#!/bin/bash

WIDTH=1280
HEIGHT=720
FPS=30
BITRATE=1500k

TARGET_IP="192.168.199.1"
TARGET_PORT="3900"

libcamera-vid -t0 -n --hdr \
    --mode 1920:1080:8 --width "$WIDTH" --height "$HEIGHT" --framerate "$FPS" \
    --codec yuv420 -o - |\
/usr/bin/ffmpeg -y -hide_banner \
    -f rawvideo -c:v rawvideo -s "$WIDTH"x"$HEIGHT" -r "$FPS" -i pipe: \
    -metadata Title="high" -metadata service_provider="arowss" \
    -c:v h264_v4l2m2m -b:v "$BITRATE" -an -f rtp_mpegts "udp://$TARGET_IP:$TARGET_PORT?ttl=1" \
