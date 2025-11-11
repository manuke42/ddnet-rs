import asyncio
import json
import websockets

URI = "ws://localhost:5000"
PLAYER_ID = 1

async def run():
    async with websockets.connect(URI) as ws:
        print("connected")
        async def recv_loop():
            try:
                async for msg in ws:
                    print("RECV:", msg)
            except Exception as e:
                print("recv loop exited:", e)

        recv_task = asyncio.create_task(recv_loop())

        try:
            while True:
                # Tick 100 times in one step message
                await ws.send(json.dumps({"type": "step", "count": 1000}))
                print("sent: step x100")
                await asyncio.sleep(0.05)

                # Jump input
                # await ws.send(json.dumps({"type": "input", "player_id": PLAYER_ID, "jump": True}))
                # print("sent: jump")
                # await asyncio.sleep(0.05)

                # Tick every 100 ms for 5 seconds (50 ticks)
                for i in range(10):
                    await ws.send(json.dumps({"type": "step"}))
                    await asyncio.sleep(0.1)
                print("sent: 50 ticks at 100ms")

                # Wait 5 seconds
                await asyncio.sleep(1)
        finally:
            recv_task.cancel()

if __name__ == "__main__":
    asyncio.run(run())