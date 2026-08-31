import { createHmac } from "crypto";
import { verifyWebhookSignature, WebhookVerificationError } from "../src/webhooks";

const SECRET = "whsec_test_secret";

function sign(body: string, ts: number, secret = SECRET): string {
  const signed = `${ts}.${body}`;
  const sig = createHmac("sha256", secret).update(signed).digest("hex");
  return `t=${ts},v1=${sig}`;
}

describe("verifyWebhookSignature", () => {
  it("accepts a validly signed payload", () => {
    const body = JSON.stringify({ eventType: "transfer" });
    const ts = Math.floor(Date.now() / 1000);
    const header = sign(body, ts);
    expect(verifyWebhookSignature(body, header, SECRET)).toBe(true);
  });

  it("rejects a payload signed with the wrong secret", () => {
    const body = JSON.stringify({ eventType: "transfer" });
    const ts = Math.floor(Date.now() / 1000);
    const header = sign(body, ts, "wrong_secret");
    expect(() => verifyWebhookSignature(body, header, SECRET)).toThrow(WebhookVerificationError);
  });

  it("rejects an expired timestamp", () => {
    const body = JSON.stringify({ eventType: "transfer" });
    const ts = Math.floor(Date.now() / 1000) - 10_000;
    const header = sign(body, ts);
    expect(() => verifyWebhookSignature(body, header, SECRET)).toThrow(WebhookVerificationError);
  });

  it("rejects a malformed header", () => {
    expect(() => verifyWebhookSignature("{}", "not-valid", SECRET)).toThrow(
      WebhookVerificationError,
    );
  });

  it("rejects a tampered body", () => {
    const body = JSON.stringify({ eventType: "transfer" });
    const ts = Math.floor(Date.now() / 1000);
    const header = sign(body, ts);
    const tampered = JSON.stringify({ eventType: "mint" });
    expect(() => verifyWebhookSignature(tampered, header, SECRET)).toThrow(
      WebhookVerificationError,
    );
  });
});
