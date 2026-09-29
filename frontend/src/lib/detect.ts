// Smart ID detection for the global search box (Issue #1105).
//
// Recognises Stellar strkeys (C… contracts, G… accounts, M… muxed accounts),
// 64-hex transaction hashes and integer ledger sequences. Everything else is
// treated as free text for full-text search.

export type Detection =
  | { kind: "empty" }
  | { kind: "contract"; value: string }
  | { kind: "account"; value: string; muxed: boolean }
  | { kind: "tx"; value: string }
  | { kind: "ledger"; value: number }
  | { kind: "text"; value: string; note?: string };

const BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const MAX_LEDGER = 0xffffffff; // ledger sequences are u32

// Strkey version bytes (see SEP-0023): the first base32 char encodes these.
const VERSION = { account: 6 << 3, muxed: 12 << 3, contract: 2 << 3 } as const;

function base32Decode(input: string): Uint8Array | null {
  const out: number[] = [];
  let bits = 0;
  let value = 0;
  for (const ch of input) {
    const idx = BASE32.indexOf(ch);
    if (idx < 0) return null;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return new Uint8Array(out);
}

function crc16xmodem(bytes: Uint8Array): number {
  let crc = 0;
  for (const b of bytes) {
    crc ^= b << 8;
    for (let i = 0; i < 8; i++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

/** Validates the strkey version byte and CRC16 checksum. */
export function isValidStrkey(key: string, version: number): boolean {
  const bytes = base32Decode(key);
  if (!bytes || bytes.length < 3 || bytes[0] !== version) return false;
  const payload = bytes.subarray(0, bytes.length - 2);
  const checksum = bytes[bytes.length - 2] | (bytes[bytes.length - 1] << 8);
  return crc16xmodem(payload) === checksum;
}

/** Strips whitespace and wrapping quotes that often come along with a paste. */
export function normalizeQuery(raw: string): string {
  return raw.trim().replace(/^["'`]+|["'`]+$/g, "").trim();
}

export function detect(raw: string): Detection {
  const q = normalizeQuery(raw);
  if (!q) return { kind: "empty" };

  // Ledger: plain integer (allow thousands separators like "51,234,567").
  const digits = q.replace(/[,_]/g, "");
  if (/^\d{1,10}$/.test(digits)) {
    const n = Number(digits);
    if (n > 0 && n <= MAX_LEDGER) return { kind: "ledger", value: n };
  }

  // Transaction hash: 64 hex chars, optionally 0x-prefixed.
  const hex = q.replace(/^0x/i, "");
  if (/^[0-9a-fA-F]{64}$/.test(hex)) return { kind: "tx", value: hex.toLowerCase() };

  // Strkeys are upper-case base32; accept lower-case pastes too.
  const upper = q.toUpperCase();
  const strkey = (prefix: string, len: number) => new RegExp(`^${prefix}[A-Z2-7]{${len - 1}}$`).test(upper);

  if (strkey("C", 56)) {
    return isValidStrkey(upper, VERSION.contract)
      ? { kind: "contract", value: upper }
      : { kind: "text", value: q, note: "Looks like a contract ID, but the checksum is invalid" };
  }
  if (strkey("G", 56)) {
    return isValidStrkey(upper, VERSION.account)
      ? { kind: "account", value: upper, muxed: false }
      : { kind: "text", value: q, note: "Looks like an account address, but the checksum is invalid" };
  }
  if (strkey("M", 69)) {
    return isValidStrkey(upper, VERSION.muxed)
      ? { kind: "account", value: upper, muxed: true }
      : { kind: "text", value: q, note: "Looks like a muxed account, but the checksum is invalid" };
  }

  return { kind: "text", value: q };
}

/** Where a detection should navigate to. */
export function routeFor(d: Exclude<Detection, { kind: "empty" }>): string {
  switch (d.kind) {
    case "contract":
      return `/contracts/${d.value}`;
    case "account":
      return `/accounts/${d.value}`;
    case "tx":
      return `/tx/${d.value}`;
    case "ledger":
      return `/explorer?ledger=${d.value}`;
    case "text":
      return `/explorer?search=${encodeURIComponent(d.value)}`;
  }
}

export const KIND_LABEL: Record<Exclude<Detection["kind"], "empty">, string> = {
  contract: "Contract",
  account: "Account",
  tx: "Transaction",
  ledger: "Ledger",
  text: "Search",
};

export function actionLabel(d: Exclude<Detection, { kind: "empty" }>): string {
  switch (d.kind) {
    case "contract":
      return "Go to contract";
    case "account":
      return d.muxed ? "Go to muxed account" : "Go to account";
    case "tx":
      return "Go to transaction";
    case "ledger":
      return `Show events in ledger ${d.value.toLocaleString()}`;
    case "text":
      return "Search events for";
  }
}

/**
 * Whether a partial input could be a contract ID prefix worth autocompleting
 * via `/v1/contracts/search` (which needs >= 4 alphanumeric characters).
 */
export function isContractPrefix(raw: string): boolean {
  const q = normalizeQuery(raw).toUpperCase();
  return /^C[A-Z2-7]{3,54}$/.test(q);
}
