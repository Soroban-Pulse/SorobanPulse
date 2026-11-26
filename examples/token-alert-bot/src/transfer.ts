// Decode SEP-41 `transfer` events from SorobanPulse webhook payloads.
//
// SorobanPulse forwards the RPC event as-is, so topics and value usually
// arrive as base64 XDR ScVals (either a bare string or `{ "xdr": "..." }`).
// Already-decoded JSON shapes are accepted too.

import { createHmac, timingSafeEqual } from "node:crypto";
import { scValToNative, xdr } from "@stellar/stellar-sdk";

/** The fields of a SorobanPulse webhook payload this bot uses. */
export interface WebhookEvent {
  contractId: string;
  type: string;
  txHash: string;
  ledger: number;
  ledgerClosedAt: string;
  inSuccessfulContractCall?: boolean;
  topic?: unknown[] | null;
  value: unknown;
}

export interface Transfer {
  from: string;
  to: string;
  /** Raw i128 amount in the token's smallest unit. */
  amount: bigint;
  /** Trailing asset topic of Stellar Asset Contract transfers (e.g. "native"). */
  asset?: string;
}

function decodeScVal(raw: unknown): unknown {
  const b64 =
    typeof raw === "string"
      ? raw
      : raw && typeof raw === "object" && typeof (raw as { xdr?: unknown }).xdr === "string"
        ? (raw as { xdr: string }).xdr
        : undefined;
  if (b64 !== undefined) {
    try {
      return scValToNative(xdr.ScVal.fromXDR(b64, "base64"));
    } catch {
      // Not XDR — fall through and treat it as already-decoded JSON.
    }
  }
  return raw;
}

function asAmount(v: unknown): bigint | undefined {
  if (typeof v === "bigint") return v;
  if (typeof v === "number" && Number.isInteger(v)) return BigInt(v);
  if (typeof v === "string" && /^-?\d+$/.test(v)) return BigInt(v);
  if (v && typeof v === "object") {
    // CAP-67 muxed transfers carry `{ amount, to_muxed_id }`; JSON i128 as `{ i128 }`.
    const o = v as Record<string, unknown>;
    return asAmount(o.amount ?? o.i128);
  }
  return undefined;
}

function asText(v: unknown): string | undefined {
  if (typeof v === "string") return v;
  if (v && typeof v === "object") {
    const o = v as Record<string, unknown>;
    const s = o.symbol ?? o.address ?? o.string;
    return typeof s === "string" ? s : undefined;
  }
  return undefined;
}

/** Returns the transfer, or `undefined` when the event is not a SEP-41 transfer. */
export function decodeTransfer(event: WebhookEvent): Transfer | undefined {
  const topics = (event.topic ?? []).map(decodeScVal);
  if (topics.length < 3 || asText(topics[0]) !== "transfer") return undefined;

  const from = asText(topics[1]);
  const to = asText(topics[2]);
  const amount = asAmount(decodeScVal(event.value));
  if (!from || !to || amount === undefined) return undefined;

  return { from, to, amount, asset: asText(topics[3]) };
}

/** Converts a whole-token threshold such as "100" or "0.5" into base units. */
export function toBaseUnits(amount: string, decimals: number): bigint {
  const match = /^(\d+)(?:\.(\d+))?$/.exec(amount.trim());
  if (!match) throw new Error(`invalid amount: ${amount}`);
  const [, whole, frac = ""] = match;
  if (frac.length > decimals) throw new Error(`${amount} has more than ${decimals} decimals`);
  return BigInt(whole + frac.padEnd(decimals, "0"));
}

/** Formats a base-unit amount for display, e.g. 1234500000n with 7 decimals → "123.45". */
export function formatAmount(amount: bigint, decimals: number): string {
  const negative = amount < 0n;
  const digits = (negative ? -amount : amount).toString().padStart(decimals + 1, "0");
  const whole = digits.slice(0, digits.length - decimals);
  const frac = digits.slice(digits.length - decimals).replace(/0+$/, "");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${negative ? "-" : ""}${grouped}${frac ? `.${frac}` : ""}`;
}

/** Default replay tolerance for signature v2, in seconds (5 minutes). */
export const DEFAULT_TOLERANCE_SECONDS = 300;

/** Header carrying the v2 signature: `t=<unix>,v1=<hex>`. */
export const SIGNATURE_HEADER = "x-sorobanpulse-signature";
/** Header carrying the unique delivery ID. */
export const DELIVERY_ID_HEADER = "x-sorobanpulse-delivery-id";
/** Legacy (v1) header: HMAC over the raw body only, no timestamp. */
export const LEGACY_SIGNATURE_HEADER = "x-sorobanpulse-signature-v1";

export interface VerifyOptions {
  /** Shared webhook secret. */
  secret: string;
  /** Raw request body bytes exactly as received. */
  body: string | Uint8Array;
  /** Request headers (lower-cased lookup is performed internally). */
  headers: Record<string, string | string[] | undefined>;
  /** Max allowed age of the signed timestamp, in seconds. Defaults to 5 minutes. */
  toleranceSeconds?: number;
  /** Current unix time in seconds; injectable for tests. Defaults to `Date.now()`. */
  now?: () => number;
  /** Set of already-seen delivery IDs, used to reject replays. */
  seenDeliveryIds?: Set<string>;
}

export interface VerifiedDelivery {
  /** Delivery ID from `X-SorobanPulse-Delivery-Id`. */
  deliveryId: string;
  /** Signed unix timestamp (seconds). */
  timestamp: number;
  /** Which signature scheme verified the delivery. */
  scheme: "v2" | "v1-legacy";
}

function headerValue(
  headers: Record<string, string | string[] | undefined>,
  name: string,
): string | undefined {
  for (const [key, value] of Object.entries(headers)) {
    if (key.toLowerCase() !== name) continue;
    return Array.isArray(value) ? value[0] : value;
  }
  return undefined;
}

function constantTimeEqualHex(a: string, b: string): boolean {
  if (a.length !== b.length || a.length % 2 !== 0) return false;
  const ab = Buffer.from(a, "hex");
  const bb = Buffer.from(b, "hex");
  if (ab.length !== bb.length) return false;
  return timingSafeEqual(ab, bb);
}

function hmacHex(secret: string, message: string | Uint8Array): string {
  return createHmac("sha256", secret).update(message).digest("hex");
}

/**
 * Verifies a SorobanPulse webhook delivery.
 *
 * Prefers signature v2 (`t=<unix>,v1=<hex>` where the HMAC covers `"<t>.<body>"`)
 * and enforces the timestamp tolerance plus delivery-ID replay protection.
 * Falls back to the legacy body-only signature during the deprecation window.
 *
 * Throws on any verification failure.
 */
export function verifyWebhook(options: VerifyOptions): VerifiedDelivery {
  const {
    secret,
    body,
    headers,
    toleranceSeconds = DEFAULT_TOLERANCE_SECONDS,
    now = () => Math.floor(Date.now() / 1000),
    seenDeliveryIds,
  } = options;

  const deliveryId = headerValue(headers, DELIVERY_ID_HEADER);
  if (!deliveryId) throw new Error("missing X-SorobanPulse-Delivery-Id header");

  if (seenDeliveryIds?.has(deliveryId)) {
    throw new Error(`replayed delivery: ${deliveryId}`);
  }

  const signature = headerValue(headers, SIGNATURE_HEADER);
  if (signature) {
    const parts = new Map<string, string>();
    for (const piece of signature.split(",")) {
      const idx = piece.indexOf("=");
      if (idx > 0) parts.set(piece.slice(0, idx).trim(), piece.slice(idx + 1).trim());
    }
    const t = parts.get("t");
    const v1 = parts.get("v1");
    if (!t || !v1) throw new Error("malformed X-SorobanPulse-Signature header");

    const timestamp = Number(t);
    if (!Number.isInteger(timestamp)) throw new Error("invalid signature timestamp");

    const age = Math.abs(now() - timestamp);
    if (age > toleranceSeconds) {
      throw new Error(`signature timestamp outside tolerance (${age}s > ${toleranceSeconds}s)`);
    }

    const expected = hmacHex(secret, `${t}.${body}`);
    if (!constantTimeEqualHex(expected, v1)) throw new Error("signature mismatch");

    seenDeliveryIds?.add(deliveryId);
    return { deliveryId, timestamp, scheme: "v2" };
  }

  const legacy = headerValue(headers, LEGACY_SIGNATURE_HEADER);
  if (!legacy) throw new Error("missing webhook signature header");

  const expected = hmacHex(secret, body);
  if (!constantTimeEqualHex(expected, legacy)) throw new Error("signature mismatch");

  // Legacy signatures carry no timestamp; still record the delivery ID so a
  // replayed legacy delivery is rejected once seen.
  seenDeliveryIds?.add(deliveryId);
  return { deliveryId, timestamp: now(), scheme: "v1-legacy" };
}
