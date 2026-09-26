import { SorobanPulseClient, EventSubscription } from "../src";

const client = new SorobanPulseClient({ apiKey: process.env.SOROBAN_PULSE_API_KEY! });

const subscription = new EventSubscription(client, {
  contractId: "CABC123",
  eventTypes: ["transfer", "mint"],
});

subscription.onEvent((event) => {
  console.log(`[${event.eventType}]`, event.data);
});

subscription.run();
