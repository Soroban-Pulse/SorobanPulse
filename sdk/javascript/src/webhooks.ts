import { createHmac, timingSafeEqual } from "crypto";

export class WebhookVerificationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "WebhookVerificationError";
  }
}

function parseSignatureHeader(header: string): { timestamp: string; signature: string } {
  const parts = new Map(
    header
      .split(",")
      .map((part) => part.split("=") as [string, string])
      .filter(([key, value]) => key && value),
  );
  const timestamp = parts.get("t");
  const signature = parts.get("v1");
  if (!timestamp || !signature) {
    throw new WebhookVerificationError("malformed signature header");
  }
  return { timestamp, signature };
}

export interface VerifyWebhookOptions {
  toleranceSeconds?: number;
}

/**
 * Verifies a SorobanPulse webhook request body against its
 * `X-SorobanPulse-Signature` header (`t=<unix_ts>,v1=<hex_hmac>`), using
 * HMAC-SHA256 over `{timestamp}.{rawBody}`.
 */
export function verifyWebhookSignature(
  rawBody: string | Buffer,
  signatureHeader: string,
  secret: string,
  options: VerifyWebhookOptions = {},
): boolean {
  const toleranceSeconds = options.toleranceSeconds ?? 300;
  const { timestamp, signature } = parseSignatureHeader(signatureHeader);

  const timestampNum = Number(timestamp);
  if (!Number.isFinite(timestampNum)) {
    throw new WebhookVerificationError("invalid timestamp");
  }

  const nowSeconds = Math.floor(Date.now() / 1000);
  if (Math.abs(nowSeconds - timestampNum) > toleranceSeconds) {
    throw new WebhookVerificationError("timestamp outside tolerance window");
  }

  const bodyBuffer = typeof rawBody === "string" ? Buffer.from(rawBody) : rawBody;
  const signedPayload = Buffer.concat([Buffer.from(`${timestamp}.`), bodyBuffer]);
  const expected = createHmac("sha256", secret).update(signedPayload).digest("hex");

  const expectedBuffer = Buffer.from(expected, "hex");
  const signatureBuffer = Buffer.from(signature, "hex");

  if (
    expectedBuffer.length !== signatureBuffer.length ||
    !timingSafeEqual(expectedBuffer, signatureBuffer)
  ) {
    throw new WebhookVerificationError("signature mismatch");
  }

  return true;
}
