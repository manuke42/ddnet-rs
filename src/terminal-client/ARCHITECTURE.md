# Terminal Client Architecture

## Overview

The terminal client is a standalone binary that implements deterministic, tick-controlled gameplay for DDNet-rs. It strictly follows the phase-based execution model requested in the requirements.

## Phase Flow

```
┌─────────────┐
│ AwaitReady  │  Wait for player spawn
└──────┬──────┘
       │
       v
┌─────────────┐
│    Input    │  ◄──────────┐
└──────┬──────┘             │
       │                    │
       v                    │
┌─────────────┐             │
│    Step     │             │
└──────┬──────┘             │
       │                    │
       v                    │
┌─────────────┐             │
│   Output    │  ───────────┘
└─────────────┘
```

## Detailed Phase Descriptions

### Phase 1: AwaitReady

**Purpose:** Connect to server and wait for game to be ready

**Actions:**
1. Connect to server at specified address
2. Send connection handshake
3. Request tick control from server
4. Wait for player spawn confirmation
5. Transition to Input phase once ready

**Server State:** Server stops its tick loop and waits

**Nothing else runs:** Connection handshake only

### Phase 2: Input

**Purpose:** Collect inputs from Unix socket

**Actions:**
1. Create and bind Unix socket at specified path
2. Wait for client connection
3. Read JSON input events line by line
4. Accumulate inputs in buffer
5. Wait for `{"type":"input_end"}` message
6. Send response with current state:
   ```json
   {"phase":"input","tick":N,"player_x":X}
   ```
7. Transition to Step phase

**Server State:** Server is waiting, not advancing

**Critical:** Nothing else runs during this phase - no rendering, no network processing (except input collection), no game logic updates

### Phase 3: Step

**Purpose:** Send inputs to server for tick processing

**Actions:**
1. Convert collected inputs to game input format
2. Package inputs with target tick number (current + 1)
3. Send input packet to server
4. Server receives inputs and permits exactly ONE tick via control bridge
5. Server processes game logic for that tick
6. Transition to Output phase

**Server State:** Server processes exactly one tick, then stops again

**Critical:** Client waits for server to complete processing

### Phase 4: Output

**Purpose:** Receive snapshot and output frame

**Actions:**
1. Receive snapshot from server
2. Parse and apply snapshot data
3. Update game state (player positions, world state, etc.)
4. Render the world (or create frame representation)
5. Send frame data to output Unix socket
6. Transition back to Input phase

**Server State:** Server has sent snapshot and is waiting again

**Critical:** Frame is sent before returning to Input phase

## Implementation Details

### File Structure

```
src/terminal-client/
├── Cargo.toml              # Package configuration
├── README.md               # User documentation
├── ARCHITECTURE.md         # This file
└── src/
    ├── main.rs             # Main client loop and phase handlers
    └── network.rs          # Network client for server communication
```

### Main Components

#### TerminalClient (main.rs)

Main structure that orchestrates the phase loop:

```rust
pub struct TerminalClient {
    input_socket_path: PathBuf,
    frame_socket_path: PathBuf,
    network_client: NetworkClient,
}
```

Key methods:
- `run()` - Main loop that cycles through phases
- `input_phase()` - Phase 2 implementation
- `step_phase()` - Phase 3 implementation  
- `output_phase()` - Phase 4 implementation

#### NetworkClient (network.rs)

Handles server communication:

```rust
pub struct NetworkClient {
    server_addr: String,
    state: Arc<Mutex<ClientState>>,
}
```

Key methods:
- `connect()` - Establish server connection
- `wait_for_ready()` - Phase 1 implementation
- `send_inputs()` - Send inputs to server
- `receive_snapshot()` - Receive game state from server

### Phase State Management

```rust
pub enum PhaseState {
    Connecting,
    AwaitReady,
    Input,
    WaitingForServer,
    Output,
}
```

State is strictly enforced - methods check current phase and error if called in wrong phase.

## Input Socket Protocol

### Input Events

```json
{"type":"key","code":"KeyD","state":"down"}
{"type":"key","code":"KeyD","state":"up"}
{"type":"mouse_move","dx":10.0,"dy":0.0}
{"type":"mouse_button","button":"Left","state":"down"}
{"type":"scroll","delta":1.0}
{"type":"input_end"}
```

### Response Format

```json
{"phase":"input","tick":42,"player_x":123.45}
```

## Frame Output Protocol

Frames are sent as JSON for the simplified implementation:

```json
{"tick":42,"player_x":123.45,"player_y":67.89}
```

In a full implementation, this would be RGBA pixel data with a binary header.

## Error Handling

- **Socket errors:** Client logs warning but continues if frame socket unavailable
- **Network errors:** Client returns error and shuts down gracefully
- **Phase violations:** Methods return error if called in wrong phase
- **Input parse errors:** Logged and ignored, continue waiting for input_end

## Concurrency Model

- **Single-threaded async:** Uses Tokio async runtime
- **No parallelism:** Phases execute sequentially
- **Blocking operations:** Phase doesn't advance until current phase completes

## Current Implementation Status

### Implemented ✅

- [x] Phase-based state machine
- [x] Unix socket input collection
- [x] Input batching with input_end
- [x] Phase response messages
- [x] Frame output to socket
- [x] Command-line arguments
- [x] Error handling and logging
- [x] Graceful shutdown

### Simulated (TODO for full implementation) 🔄

- [ ] Real network connection to DDNet server
- [ ] DDNet protocol handshake
- [ ] Actual snapshot processing
- [ ] Game state rendering
- [ ] Input conversion to game format
- [ ] Full frame encoding (RGBA, etc.)

### Architecture Complete ✅

The phase-based control flow is fully implemented. The simulated parts (networking, rendering) are placeholders that demonstrate the architecture. Integration with actual DDNet protocol would replace these simulated parts while maintaining the same phase flow.

## Comparison with Main Client

| Feature | Terminal Client | Main Client (drive_tick_loop) |
|---------|----------------|-------------------------------|
| Binary | Separate | Same as game |
| GUI | No | Yes |
| Dependencies | Minimal | Full graphics stack |
| Purpose | Automation/ML | Visual debugging |
| Phase enforcement | Strict | Strict |
| Input source | Unix socket only | Socket + hardware |
| Frame output | Unix socket | Graphics backend |
| Target use case | Headless automation | Interactive tick control |

## Future Enhancements

1. **Network Integration:**
   - Implement actual DDNet protocol
   - Handle connection handshake
   - Process snapshots properly

2. **Rendering:**
   - Implement headless rendering
   - Output standard frame formats (RGBA, etc.)
   - Support frame capture settings

3. **Configuration:**
   - Config file support
   - More command-line options
   - Multiple input/output formats

4. **Features:**
   - Reconnection support
   - State save/restore
   - Replay support
   - Performance metrics

5. **Testing:**
   - Unit tests for phase transitions
   - Integration tests with mock server
   - Performance benchmarks
