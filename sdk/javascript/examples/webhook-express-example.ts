import express from "express";
import { verifyWebhookSignature, WebhookVerificationError } from "../src";

const app = express();
app.use(express.raw({ type: "application/json" }));

const WEBHOOK_SECRET = process.env.SOROBAN_PULSE_WEBHOOK_SECRET!;

app.post("/webhooks/soroban-pulse", (req, res) => {
  const signature = req.header("X-SorobanPulse-Signature") ?? "";

  try {
    verifyWebhookSignature(req.body, signature, WEBHOOK_SECRET);
  } catch (err) {
    if (err instanceof WebhookVerificationError) {
      return res.status(400).send("invalid signature");
    }
    throw err;
  }

  const event = JSON.parse(req.body.toString("utf-8"));
  console.log("received verified event:", event.eventType);
  res.status(204).end();
});

app.listen(3000);
