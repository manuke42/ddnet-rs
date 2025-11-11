#!/bin/bash
set -euo pipefail

FIFO=/tmp/ddnet_ddnet_ws_fifo
WSCAT_LOG=/tmp/ddnet_ddnet_ws_out.log

cleanup() {
  echo "cleaning up..."
  kill "${WSCAT_PID:-0}" 2>/dev/null || true
  exec 3>&- || true
  rm -f "$FIFO"
}
trap cleanup INT TERM EXIT

rm -f "$FIFO" "$WSCAT_LOG"
mkfifo "$FIFO"

# keep fd 3 open so the reader (wscat) doesn't see EOF when we write single messages
exec 3>"$FIFO"

# start wscat reading from the FIFO (stdin) and printing server messages to stdout
# keep it in background and capture its pid
echo "starting wscat..."
wscat -c ws://localhost:5000 --no-color < "$FIFO" | tee "$WSCAT_LOG" &
WSCAT_PID=$!

# give it a moment to connect
sleep 0.5

echo "starting test loop..."

while true; do
  # Tick 100 times in one command
  echo "Stepping 100 ticks..."
  printf '%s\n' '{"type":"step","count":100}' >&3
  sleep 0.05

  # Jump (single input)
    echo "Jumping..."
  printf '%s\n' '{"type":"input","player_id":1,"jump":true}' >&3
  sleep 0.05

  # Tick every 100 ms for 5 seconds (50 ticks)
    echo "Stepping 50 ticks with 100ms delay..."
  for ((i=1; i<=50; i++)); do
    printf '%s\n' '{"type":"step"}' >&3
    sleep 0.1
  done

  # Wait 5 seconds
  sleep 5
done