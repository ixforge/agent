#!/bin/sh
bird -f -c /etc/bird/bird.conf -s /run/bird/bird.ctl &
BIRD_PID=$!
sleep 1
chmod 666 /run/bird/bird.ctl
wait $BIRD_PID
