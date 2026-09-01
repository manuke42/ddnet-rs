# DDNet AI Bridge

The `raw-ai-mode` server exposes a local-only Unix-domain AI socket:

```text
/tmp/ddnet-ai.sock
```

The socket is created with mode `0600`. It is an actor/training interface, not a public game protocol and not a replacement for normal DDNet clients.

## Frame Format

Every message is little-endian and length-prefixed:

```text
u32 payload_length
[u8; 4] magic = "DAI1"
u8 version = 1
u8 message_type
payload
```

Message types are `HELLO=1`, `STEP=2`, `INPUT=3`, `RUN_MODE=4`, `RESET=5`, `RESPAWN=6`, `SPAWN=7`, `DESPAWN=8`, `ACK=128`, `MAP=129`, `STATE=130`, and `ERROR=255`.

All successful control commands receive an `ACK` payload:

```text
u8 acknowledged_message_type
u64 acknowledged_tick
```

If a new `HELLO` arrives while the server is parked in manual mode, the bridge
releases one tick and acknowledges that target. This guarantees a newly attached
actor can obtain a state at least as recent as its handshake after a prior actor
crashed or disconnected.

For `STEP`, `acknowledged_tick` is the minimum server tick that an actor must wait for before treating an incoming state as the action result. This prevents stale queued states from being used as an RL transition.

## Input

`INPUT` has a 29-byte payload:

```text
u64 player_id
u64 for_tick                 # 0 applies to the next authoritative tick
i8 direction                 # -1, 0, 1
u8 buttons                   # bit 0 jump, bit 1 fire, bit 2 hook
u8 weapon                    # 0 unchanged; 1 hammer, 2 gun, 3 shotgun, 4 grenade, 5 laser
u8 reserved[2]
f32 cursor_x
f32 cursor_y
```

The cursor is a vector relative to the character, exactly like the real client
target coordinates. Its direction controls hook launch and weapon fire; its
magnitude does not affect those mechanics. `(0, 0)` is invalid and is sanitized
by the game to a tiny right-facing vector.

The server turns rising button edges into DDNet consumable jump/fire/hook events
at the supplied cursor while preserving button hold state. A held button must be
released before another rising-edge event can occur. Input is applied through
the existing authoritative `PlayerInput` path rather than emulating keyboard
events in a graphical client.

## State

The server sends a `MAP` frame when it starts and after every map load. It contains the non-empty physics tiles from game, front, tele, speedup, switch, and tune layers. Static geometry is deliberately sent once per map; the Python framework caches and preprocesses it into local/regional observation geometry.

After each game tick, while an AI socket is connected, the server sends `STATE` with fixed-size records for characters, projectiles, lasers, pickups, and flags. The bridge does not send rendered frames, JSON snapshots, or map assets on this hot path.

## Tick Modes

- `0` manual: only `STEP` releases ticks. Use this for reproducible action/state rollouts.
- `1` realtime: default server behavior for normal client connection.
- `2` unbounded: ticks continuously with a scheduler yield, retaining the map's normal physics timestep.

Do not raise the simulation tick rate for training because it changes gameplay physics. Use manual batched `STEP` calls or unbounded mode to increase throughput.

`RESET` uses the server's normal map-reload lifecycle. It is appropriate for map-level curriculum changes, but a graphical client must reload and rejoin afterwards. Fast spawn-level resets require an explicit game-mode scenario hook and are intentionally not fabricated in this generic bridge.

`RESPAWN` has an eight-byte little-endian player ID payload. It uses the normal in-game kill and respawn lifecycle for that connected player, advances the six required manual ticks, and returns an acknowledgement for the spawned tick. It does not reload the map or disconnect the graphical client. It is intended for local, single-player AI episodes.

`SPAWN` has no payload. It creates an in-process server-owned player with no
network peer, switches to manual mode, advances one tick, and acknowledges that
target tick. The actor discovers the new player ID by comparing character IDs in
the state before and after the acknowledged tick. This is the preferred training
path because it requires no graphical client and retains authoritative stepping.

`DESPAWN` contains the native player's eight-byte little-endian ID. It removes
that player through the normal disconnect lifecycle and advances one manual tick.
Actors must only despawn players they created themselves.
