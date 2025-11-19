#!/usr/bin/env python3
"""
Demo script for deterministic tick control via input socket.

This script demonstrates how to control the game in a deterministic way:
1. Connect to the input socket
2. Send a batch of inputs
3. Mark the end of the batch with input_end
4. Receive the state response
5. Repeat

Usage:
    python3 tick-control-demo.py [socket_path]

Prerequisites:
    - Start the game/server with drive_tick_loop = true in config
    - Set input_socket_path in config (default: /tmp/ddnet-input.sock)
    - Optionally set frame_socket_path to capture frames
"""

import argparse
import json
import socket
import sys
import time
from pathlib import Path


def connect_to_socket(socket_path: Path, max_retries: int = 10) -> socket.socket:
    """Connect to the input socket with retries."""
    for attempt in range(max_retries):
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect(str(socket_path))
            print(f"✓ Connected to {socket_path}")
            return sock
        except (FileNotFoundError, ConnectionRefusedError) as e:
            if attempt < max_retries - 1:
                print(f"  Waiting for socket... ({attempt + 1}/{max_retries})")
                time.sleep(1)
            else:
                raise RuntimeError(
                    f"Could not connect to {socket_path} after {max_retries} attempts. "
                    "Make sure the game is running with drive_tick_loop=true"
                ) from e


def send_input_batch(sock: socket.socket, inputs: list[dict]) -> dict:
    """Send a batch of inputs and wait for response."""
    # Send all inputs
    for inp in inputs:
        message = json.dumps(inp) + "\n"
        sock.sendall(message.encode('utf-8'))
        print(f"  → {json.dumps(inp)}")
    
    # Mark end of batch
    end_message = json.dumps({"type": "input_end"}) + "\n"
    sock.sendall(end_message.encode('utf-8'))
    print(f"  → {json.dumps({'type': 'input_end'})}")
    
    # Wait for response
    response_data = b""
    while True:
        chunk = sock.recv(1024)
        if not chunk:
            raise ConnectionError("Socket closed while waiting for response")
        response_data += chunk
        if b"\n" in response_data:
            break
    
    response_str = response_data.decode('utf-8').strip()
    response = json.loads(response_str)
    print(f"  ← {response_str}")
    return response


def run_demo(socket_path: Path):
    """Run the demo with a sequence of inputs."""
    print("=" * 60)
    print("Deterministic Tick Control Demo")
    print("=" * 60)
    
    sock = connect_to_socket(socket_path)
    
    try:
        # Demo sequence: move right for a few ticks
        sequences = [
            {
                "name": "Wait for ready",
                "inputs": [],
                "description": "Waiting for player to spawn..."
            },
            {
                "name": "Move right",
                "inputs": [
                    {"type": "key", "code": "KeyD", "state": "down"},
                ],
                "description": "Moving right"
            },
            {
                "name": "Continue right",
                "inputs": [],
                "description": "Continuing movement"
            },
            {
                "name": "Stop moving",
                "inputs": [
                    {"type": "key", "code": "KeyD", "state": "up"},
                ],
                "description": "Releasing movement key"
            },
            {
                "name": "Jump",
                "inputs": [
                    {"type": "key", "code": "Space", "state": "down"},
                ],
                "description": "Jumping"
            },
        ]
        
        for i, seq in enumerate(sequences, 1):
            print(f"\n[Step {i}] {seq['name']}")
            print(f"  {seq['description']}")
            
            response = send_input_batch(sock, seq['inputs'])
            
            phase = response.get('phase', 'unknown')
            tick = response.get('tick', '?')
            player_x = response.get('player_x', '?')
            
            print(f"  State: phase={phase}, tick={tick}, x={player_x}")
            
            # Wait a bit between steps for visibility
            time.sleep(0.5)
        
        print("\n" + "=" * 60)
        print("Demo complete!")
        print("The game executed {} deterministic ticks.".format(len(sequences)))
        print("=" * 60)
        
    finally:
        sock.close()
        print("\nSocket closed.")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Demo deterministic tick control",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument(
        "socket",
        nargs="?",
        default="/tmp/ddnet-input.sock",
        type=Path,
        help="Path to input socket (default: %(default)s)"
    )
    
    args = parser.parse_args(argv)
    
    try:
        run_demo(args.socket)
        return 0
    except Exception as e:
        print(f"\n✗ Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
