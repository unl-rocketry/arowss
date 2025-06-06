#!/bin/bash

WIDTH=1280
HEIGHT=720
FPS=30
BITRATE=1500k

TARGET_IP="192.168.199.1"

LOW_RES=240
LOW_RES_HEIGHT=$(qalc -t "round(($LOW_RES / 9) * 16)")
LOW_FPS=15

libcamera-vid -t0 -n --hdr \
    --mode 1920:1080:8 --width "$WIDTH" --height "$HEIGHT" --framerate "$FPS" \
    --codec yuv420 -o - |\
/usr/bin/ffmpeg -y -hide_banner \
    -f rawvideo -c:v rawvideo -s "$WIDTH"x"$HEIGHT" -r "$FPS" -i pipe: \
    -metadata Title="high" -metadata service_provider="arowss" \
    -c:v h264_v4l2m2m -b:v 1500k -an -f rtp_mpegts udp://192.168.199.1:3900?ttl=1 \
