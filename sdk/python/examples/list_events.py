"""Example: list recent events for a contract."""

import os

from soroban_pulse import SorobanPulseClient

client = SorobanPulseClient(api_key=os.environ["SOROBAN_PULSE_API_KEY"])

for event in client.iter_events(contract_id="CABC123", limit=50):
    print(event["id"], event["event_type"], event["ledger"])
