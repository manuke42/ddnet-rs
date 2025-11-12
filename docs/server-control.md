# Server Control WebSocket

The game server exposes a lightweight WebSocket control interface (see `game/game-server/src/control.rs`) that allows tooling to step the simulation manually and to inject player input. This is intended for automation, debugging, and replay tooling.

## Connection

- **Endpoint:** `ws://127.0.0.1:5000`
- **Transport:** clear-text WebSocket (no TLS)
- **Message format:** UTF-8 encoded JSON objects

Every JSON request receives an acknowledgement message (`step_ack`, `input_ack`, or `error`). When connected, clients also receive a stream of serialized tick snapshots that mirror `ControlTickReport`.

## Commands

### Step Simulation

```json
{
  "type": "step",
  "count": 10
}
```

- `count` is optional; defaults to `1` if omitted.
- Allows `count` ticks to run and returns a `step_ack` response.

### Queue Player Input

```json
{
  "type": "input",
  "player_id": 1,
  "for_tick": 42,
  "dir": 1,
  "jump": true,
  "fire": false,
  "hook": false,
  "cursor": [32.5, 64.0]
}
```

- `player_id`: numeric identifier of the local player slot.
- `for_tick`: optional monotonic tick the input should target.
- `dir`: movement direction (`-1`, `0`, `1`).
- `jump`, `fire`, `hook`: boolean state flags.
- `cursor`: optional `[x, y]` cursor position (double precision).

If valid, the server enqueues a `PlayerControlMessage` and responds with `input_ack`. Invalid requests yield an `error` response containing the failure reason.

## Usage Examples

### Using `websocat`

Run the server, then from another shell:

```sh
websocat ws://127.0.0.1:5000
```

Send JSON payloads (press `Enter` after each):

1. Allow a single tick:
   ```json
   {"type":"step"}
   ```
2. Enqueue movement for player `1`:
   ```json
   {"type":"input","player_id":1,"dir":1}
   ```

The terminal prints acknowledgement messages and streamed tick snapshots.

### Using Python

```python
import asyncio
import json
import websockets

async def main():
    async with websockets.connect("ws://127.0.0.1:5000") as ws:
        await ws.send(json.dumps({"type": "step", "count": 5}))
        print(await ws.recv())

        await ws.send(json.dumps({
            "type": "input",
            "player_id": 1,
            "dir": 1,
            "jump": True,
        }))
        print(await ws.recv())

        # Listen for streamed tick snapshots
        for _ in range(3):
            print(await ws.recv())

asyncio.run(main())
```

## Broadcast Snapshots

Clients automatically subscribe to the broadcast channel and receive the most recent tick snapshot immediately after connecting. Subsequent snapshots are pushed as ticks complete. Each snapshot is a JSON representation of `ControlTickReport` containing per-player and per-stage diagnostics.

## Graceful Shutdown

When the control bridge shuts down, the WebSocket writer stops transmitting, and in-flight `step`/`input` requests return `error` responses. Clients should close the connection when reads yield EOF or when they receive an error message indicating the control server is unavailable.
