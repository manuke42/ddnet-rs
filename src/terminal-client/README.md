# DDNet Terminal Client

A standalone terminal client for DDNet-rs that provides deterministic, tick-controlled gameplay.

## Features

- **Deterministic Tick Control**: Client controls when the server advances simulation
- **Phase-Based Execution**: Clear separation of Input, Step, and Output phases
- **Unix Socket Interface**: Receives inputs via Unix socket
- **Frame Output**: Sends rendered frames to output socket

## How It Works

The terminal client follows a strict phase-based execution model:

### 1. AwaitReady Phase
- Client connects to the server
- Waits for player to spawn
- Server stops ticking and waits

### 2. Input Phase
- Client opens Unix socket and waits for input
- Collects input events (keyboard, mouse, etc.)
- Waits for `input_end` message to complete batch
- **Nothing else runs during this phase**
- Sends JSON response back with current state

### 3. Step Phase
- Client sends collected inputs to server
- Server receives inputs and processes **exactly one tick**
- Server stops and waits again

### 4. Output Phase
- Client receives snapshot from server
- Renders the world state
- Sends rendered frame to output socket
- Returns to Input phase

## Building

```bash
cargo build --release -p terminal-client
```

## Usage

```bash
./target/release/terminal-client [OPTIONS]

Options:
  -s, --server <SERVER>          Server address [default: 127.0.0.1:8303]
  -i, --input-socket <PATH>      Input socket path [default: /tmp/ddnet-input.sock]
  -f, --frame-socket <PATH>      Frame socket path [default: /tmp/ddnet-frames.sock]
```

## Input Socket Protocol

### Input Messages

Send newline-delimited JSON to the input socket:

```json
{"type":"key","code":"KeyD","state":"down"}
{"type":"mouse_move","dx":10.0,"dy":0.0}
{"type":"input_end"}
```

### Response Format

After `input_end`, the client responds with:

```json
{"phase":"input","tick":42,"player_x":123.45}
```

## Example

1. Start the server:
```bash
cargo run --release --bin ddnet-rs
```

2. Start the terminal client:
```bash
cargo run --release -p terminal-client
```

3. Send inputs via socket:
```bash
echo '{"type":"key","code":"KeyD","state":"down"}' | nc -U /tmp/ddnet-input.sock
echo '{"type":"input_end"}' | nc -U /tmp/ddnet-input.sock
```

4. The client will:
   - Collect the inputs
   - Send them to the server
   - Wait for server to process one tick
   - Receive snapshot
   - Render and output frame
   - Wait for next input batch

## Differences from Main Client

The terminal client is a **separate binary** that:
- Runs in terminal without GUI
- Controls the server tick loop
- Strictly enforces phase-based execution
- Only advances on explicit input batches
- Minimal rendering (just state output)

The main `ddnet-rs` client can also operate in this mode with `drive_tick_loop = true`, but the terminal client is designed specifically for:
- Automated testing
- ML training
- Deterministic replay
- Headless operation

## Implementation Status

This is a simplified implementation that demonstrates the architecture. A full implementation would:

- [ ] Implement real network connection to DDNet server
- [ ] Handle full game protocol (snapshots, inputs, etc.)
- [ ] Implement proper rendering (or headless rendering)
- [ ] Support all input types
- [ ] Handle reconnection and error recovery
- [ ] Add configuration file support
- [ ] Implement frame capture in standard formats

The current implementation uses simulated network and rendering to demonstrate the phase-based control flow.
