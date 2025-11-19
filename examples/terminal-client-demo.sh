#!/bin/bash
# Demo script for the terminal client

set -e

echo "==================================="
echo "Terminal Client Demo"
echo "==================================="
echo ""

# Check if terminal-client binary exists
if [ ! -f "target/release/terminal-client" ]; then
    echo "Building terminal-client..."
    cargo build --release -p terminal-client
fi

# Start the terminal client in background
echo "Starting terminal client..."
./target/release/terminal-client \
    --server 127.0.0.1:8303 \
    --input-socket /tmp/ddnet-terminal-input.sock \
    --frame-socket /tmp/ddnet-terminal-frames.sock &

CLIENT_PID=$!
echo "Terminal client started (PID: $CLIENT_PID)"

# Wait a bit for client to start
sleep 2

# Function to send input
send_input() {
    local socket="/tmp/ddnet-terminal-input.sock"
    echo "$1" | nc -U "$socket" -w 1 || true
}

# Send a sequence of inputs
echo ""
echo "Sending input sequence..."
echo "-----------------------------------"

echo "Step 1: Move right"
send_input '{"type":"key","code":"KeyD","state":"down"}'
send_input '{"type":"input_end"}'
sleep 0.5

echo "Step 2: Continue moving"
send_input '{"type":"input_end"}'
sleep 0.5

echo "Step 3: Jump"
send_input '{"type":"key","code":"Space","state":"down"}'
send_input '{"type":"input_end"}'
sleep 0.5

echo "Step 4: Stop moving"
send_input '{"type":"key","code":"KeyD","state":"up"}'
send_input '{"type":"key","code":"Space","state":"up"}'
send_input '{"type":"input_end"}'
sleep 0.5

echo "-----------------------------------"
echo "Demo complete!"
echo ""

# Cleanup
echo "Stopping terminal client..."
kill $CLIENT_PID 2>/dev/null || true

echo "Done!"
