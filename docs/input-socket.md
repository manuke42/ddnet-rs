# Unix Input Socket

The client can accept simulated keyboard and mouse input through a Unix domain socket. When the client reaches an active game state it attempts to create the socket configured by `config.game.cl.input_socket_path`.

## Enabling the Socket

1. Set the desired path in your client configuration, for example:
   ```toml
   [game.cl]
   input_socket_path = "/tmp/ddnet-input.sock"
   ```
2. Start the client and join a server. Once the game is active the socket is created and ready for commands.

## Message Format

Send newline-delimited JSON objects. Each object must contain a `type` field and additional data:

| `type`        | Additional fields                                            | Notes                                    |
|---------------|--------------------------------------------------------------|------------------------------------------|
| `key`         | `code`: string (`KeyW`, `Space`, etc.), `state`: `down`/`up` | Uses winit `KeyCode` names.              |
| `mouse_button`| `button`: `left`/`right`/`middle`/`back`/`forward`, `state`  | State accepts `down`/`up`.               |
| `mouse_move`  | `dx`, `dy`: floating-point deltas                            | Values represent relative motion.        |
| `scroll`      | `delta`: positive or negative value                          | Sign controls wheel direction.           |

Example commands:

```json
{"type":"key","code":"KeyW","state":"down"}
{"type":"key","code":"KeyW","state":"up"}
{"type":"mouse_move","dx":12.5,"dy":-4.0}
{"type":"scroll","delta":1.0}
{"type":"mouse_button","button":"left","state":"down"}
{"type":"mouse_button","button":"left","state":"up"}
```

Each line **must** end with a newline character (`\n`). Commands with an empty `delta` (zero) are ignored.

## Test Sender Utility

The repository includes `input-socket-sender.py`, a small helper that emits random input commands:

```bash
python3 input-socket-sender.py /tmp/ddnet-input.sock
```

Options:

- `socket` (positional): Unix socket path. Defaults to `$DDNET_INPUT_SOCKET` or `/tmp/ddnet-input.sock`.
- `--interval` / `-i`: Average delay (seconds) between batches of commands (default `0.15`).

Stop the tool with `Ctrl+C`. Use it to validate integrations or to simulate rough player input via the socket.
