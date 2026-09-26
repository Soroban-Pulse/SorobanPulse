"""Example: async event listing and subscription creation."""

import asyncio
import os

from soroban_pulse import AsyncSorobanPulseClient


async def main() -> None:
    async with AsyncSorobanPulseClient(api_key=os.environ["SOROBAN_PULSE_API_KEY"]) as client:
        async for event in client.iter_events(contract_id="CABC123", limit=25):
            print(event["id"], event["event_type"])

        sub = await client.create_subscription(
            contract_id="CABC123",
            webhook_url="https://example.com/webhooks/soroban-pulse",
            event_types=["transfer", "mint"],
        )
        print("created subscription", sub["id"])


if __name__ == "__main__":
    asyncio.run(main())
