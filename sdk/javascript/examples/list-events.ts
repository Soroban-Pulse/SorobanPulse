import { SorobanPulseClient } from "../src";

async function main() {
  const client = new SorobanPulseClient({ apiKey: process.env.SOROBAN_PULSE_API_KEY! });

  for await (const event of client.iterEvents({ contractId: "CABC123", limit: 50 })) {
    console.log(event.id, event.eventType, event.ledger);
  }
}

main().catch(console.error);
