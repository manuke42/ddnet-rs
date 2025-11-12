#!/usr/bin/env python3
"""Utility to send random input commands over the ddnet Unix input socket."""

import argparse
import json
import os
import random
import socket
import sys
import time
from typing import Iterable, Tuple
import errno

KEY_CODES = [
    "KeyW",
    "KeyA",
    "KeyS",
    "KeyD",
    "KeyK",
    "Space",
]
MOUSE_BUTTONS = ["Left", "Right"]
SCROLL_VALUES = [-1.0, 1.0]


def next_key_event() -> Iterable[dict]:
    code = random.choice(KEY_CODES)
    pressed = random.choice([True, False])
    state = "down" if pressed else "up"
    yield {"type": "key", "code": code, "state": state}

    # Optionally send the opposite state shortly after to mimic a tap
    if pressed and random.random() < 0.6:
        yield {"type": "key", "code": code, "state": "up"}


def next_mouse_button_event() -> Iterable[dict]:
    button = random.choice(MOUSE_BUTTONS)
    pressed = random.choice([True, False])
    state = "down" if pressed else "up"
    yield {"type": "mouse_button", "button": button, "state": state}

    if pressed and random.random() < 0.5:
        yield {"type": "mouse_button", "button": button, "state": "up"}


def next_mouse_move_event() -> Iterable[dict]:
    # Small jitter to keep movement plausible
    dx = random.uniform(-15.0, 15.0)
    dy = random.uniform(-15.0, 15.0)
    yield {"type": "mouse_move", "dx": dx, "dy": dy}


def next_scroll_event() -> Iterable[dict]:
    yield {"type": "scroll", "delta": random.choice(SCROLL_VALUES)}


EVENT_GENERATORS: Tuple = (
    next_key_event,
    next_mouse_button_event,
    next_mouse_move_event,
    next_scroll_event,
)


def send_events(socket_path: str, interval: float) -> None:
    reconnect_delay = 0.5

    while True:
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
                sock.connect(socket_path)
                print(f"Connected to {socket_path}. Press Ctrl+C to stop.")
                reconnect_delay = 0.5

                while True:
                    generator = random.choice(EVENT_GENERATORS)
                    for event in generator():
                        payload = json.dumps(event, separators=(",", ":")) + "\n"
                        sock.sendall(payload.encode("utf-8"))
                        print(f"Sent: {payload.strip()}")
                    time.sleep(max(0.0, random.gauss(interval, interval * 0.4)))

        except KeyboardInterrupt:
            print("\nInterrupted by user", file=sys.stderr)
            break
        except OSError as err:
            if err.errno in {errno.EPIPE, errno.ECONNRESET}:
                print(f"Connection dropped ({err.strerror}). Reconnecting...", file=sys.stderr)
            elif err.errno in {errno.ENOENT, errno.ECONNREFUSED}:
                print(
                    f"Socket {socket_path} unavailable ({err.strerror}). Retrying...",
                    file=sys.stderr,
                )
            else:
                print(f"Unexpected socket error: {err}", file=sys.stderr)

        time.sleep(reconnect_delay)
        reconnect_delay = min(reconnect_delay * 1.5, 5.0)


def parse_args(argv: Iterable[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Send random input events to the ddnet Unix input socket",
    )
    parser.add_argument(
        "socket",
        nargs="?",
        default=os.environ.get("DDNET_INPUT_SOCKET", "/tmp/ddnet-input.sock"),
        help="Path to the Unix domain socket (default: %(default)s or $DDNET_INPUT_SOCKET)",
    )
    parser.add_argument(
        "-i",
        "--interval",
        type=float,
        default=0.15,
        help="Average delay between groups of events in seconds (default: %(default)s)",
    )
    return parser.parse_args(argv)


def main(argv: Iterable[str]) -> int:
    args = parse_args(argv)

    send_events(args.socket, max(0.01, args.interval))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
