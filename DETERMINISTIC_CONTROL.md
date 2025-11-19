# Deterministic Tick Control

This document explains the deterministic, client-driven tick control system implemented in DDNet-rs.

## Overview

The deterministic tick control system allows external tools to control the game simulation in a step-by-step, time-independent manner. This is useful for:
- Training machine learning models
- Automated testing and replay
- Debugging and analysis
- Deterministic simulation for research

## Architecture

### Phase-Based State Machine

The system uses a 4-phase state machine:

```
AwaitReady → Input → WaitingForServer → Output → (back to Input)
```

1. **AwaitReady**: Wait for player to spawn and world to be ready
2. **Input**: Collect inputs (nothing else runs during this phase)
3. **WaitingForServer**: Client sent inputs, waiting for server to process
4. **Output**: Server sent snapshot, client renders and outputs frame

### Key Components

#### Client Side (`src/client/game/active.rs`)
- `TickLoopPhase` enum defines the state machine
- `drive_tick_loop_step()` manages phase transitions
- `allows_input_handling()` gates input processing
- `allows_rendering()` gates rendering
- `complete_output_phase()` transitions back to Input phase

#### Server Side (`game/game-server/src/server.rs`)
- `control_bridge` provides tick gating
- `tick_controller` tracks which client has control
- `try_acquire_tick()` / `wait_for_tick()` pause simulation
- Server advances only when client sends inputs

#### Communication
- **Input Socket** (`src/client/input/socket.rs`): Unix domain socket for input
- **Frame Socket** (`lib/frame-sender`): Unix domain socket for frame output
- **Network Messages**: Client sends inputs, server sends snapshots

## Configuration

Add to your config file:

```toml
[game.cl]
# Enable client-driven tick control
drive_tick_loop = true

# Path to Unix socket for input (empty to disable)
input_socket_path = "/tmp/ddnet-input.sock"

[game.cl.recorder]
# Path to Unix socket for frame output (empty to disable)
frame_socket_path = "/tmp/ddnet-frames.sock"

# Frame capture settings
fps = 60
sample_rate = 48000
crf = 23
hw_accel = ""
```

## Usage

### 1. Start the Game

Start the game with `drive_tick_loop = true` in your config. The game will:
- Connect to the server (can be local integrated server)
- Wait for player to spawn (AwaitReady phase)
- Enter Input phase and wait for input

### 2. Send Inputs

Connect to the input socket and send JSON messages:

```python
import socket
import json

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect("/tmp/ddnet-input.sock")

# Send some inputs
inputs = [
    {"type": "key", "code": "KeyD", "state": "down"},
    {"type": "mouse_move", "dx": 10.0, "dy": 0.0},
    {"type": "input_end"}  # Mark end of batch
]

for inp in inputs:
    message = json.dumps(inp) + "\n"
    sock.sendall(message.encode('utf-8'))

# Read response
response = sock.recv(1024).decode('utf-8')
state = json.loads(response)
print(f"Phase: {state['phase']}, Tick: {state['tick']}")
```

### 3. Capture Frames (Optional)

Connect to the frame socket to receive rendered frames:

```python
import socket
import struct

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect("/tmp/ddnet-frames.sock")

# Read handshake
handshake = sock.recv(1024).decode('utf-8').strip()
info = json.loads(handshake)
print(f"Frame size: {info['width']}x{info['height']}, FPS: {info['fps']}")

# Read frame
# Format: 32-byte header + RGBA pixel data
header = sock.recv(32)
# ... parse header and read pixel data
```

See `examples/tick-control-demo.py` for a complete example.

## Input Socket Protocol

### Input Messages

| Type | Fields | Description |
|------|--------|-------------|
| `key` | `code` (string), `state` (down/up) | Keyboard input |
| `mouse_button` | `button` (string), `state` (down/up) | Mouse button |
| `mouse_move` | `dx` (float), `dy` (float) | Mouse movement |
| `scroll` | `delta` (float) | Mouse wheel |
| `input_end` | (none) | Marks end of input batch |

### Response Format

After each `input_end`, the client responds with JSON:

```json
{
  "phase": "input",
  "tick": 42,
  "player_x": 123.45
}
```

Fields:
- `phase`: Current phase (await_ready, input, waiting_for_server, output, inactive)
- `tick`: Predicted monotonic tick number
- `player_x`: X position of active player

## Implementation Details

### Input Phase

During the Input phase:
- Input collection is active (`allows_input_handling()` returns true)
- Rendering is blocked (`allows_rendering()` returns false)
- Client waits for either:
  - Hardware input (keyboard/mouse) → automatic batch completion
  - Socket input batch ending with `input_end` message

### Server Control

When `drive_tick_loop` is enabled:
1. Client sends `TickControllerReady` message after connecting
2. Server assigns tick control to the client
3. Server's main loop calls `control_bridge.try_acquire_tick()`
4. Server waits until client sends inputs with future tick number
5. Client's input message includes target tick: `tick_of_inp + 1`
6. Server permits one tick via `control_bridge.allow_ticks(1)`
7. Server processes exactly one tick
8. Server sends snapshot to client

### Output Phase

During the Output phase:
- Rendering is active (`allows_rendering()` returns true)
- Input collection is blocked (`allows_input_handling()` returns false)
- Client renders the world
- Frame sender automatically captures and sends frame
- Client calls `complete_output_phase()` to return to Input phase

## Logging

All phase transitions are logged with target "client":

```
INFO [client] Entering Input phase at tick 10
INFO [client] Dispatching input for tick 11, transitioning to WaitingForServer phase
INFO [client] Snapshot received for tick 11, transitioning to Output phase
INFO [client] Completing Output phase, returning to Input phase
```

Enable logging:
```bash
RUST_LOG=ddnet_rs=info cargo run
```

## Examples

See `examples/tick-control-demo.py` for a complete working example that:
- Connects to the input socket
- Sends a sequence of inputs
- Receives state responses
- Demonstrates deterministic control

## Comparison with Control WebSocket

DDNet-rs has two ways to control the simulation:

### Client-Driven Control (This Feature)
- Enabled via `drive_tick_loop = true`
- Client requests control from server
- Server pauses until client sends inputs
- One client can control the simulation
- Input via Unix socket
- Frame output via Unix socket
- Integrated with existing client

### Control WebSocket (Alternative)
- Enabled via `sv.control_websocket_enabled = true`
- External tool connects to WebSocket on port 5000
- Send `{"type":"step","count":1}` to advance ticks
- Send `{"type":"input",...}` to inject player input
- Receive tick state broadcasts
- Independent of client
- See `docs/server-control.md`

**Use client-driven control when:**
- You want the client to control the server
- You need frame capture integrated
- You want deterministic input-to-frame loop

**Use control WebSocket when:**
- You want external tool to control server
- You don't need the client running
- You want network-based control

## Troubleshooting

### Socket not created
- Check `input_socket_path` is set in config
- Ensure game is in Active state (player spawned)
- Check permissions on socket directory

### Client not entering Input phase
- Verify `drive_tick_loop = true` in config
- Wait for player to spawn (AwaitReady phase)
- Check server logs for tick control assignment

### Server not responding
- Ensure server has granted tick control to client
- Check client sent `TickControllerReady` message
- Verify inputs include future tick number

### Frames not captured
- Set `frame_socket_path` in config
- Connect frame socket before client starts rendering
- Check frame sender logs for errors

## References

- Input Socket Documentation: `docs/input-socket.md`
- Server Control Documentation: `docs/server-control.md`
- Example Script: `examples/tick-control-demo.py`
- Input Sender Utility: `input-socket-sender.py`
