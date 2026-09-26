// Defence in depth: the server already redacts stored delivery payloads, but
// the UI masks sensitive headers again before rendering in case it didn't.

const SENSITIVE_HEADER = /^(authorization|proxy-authorization|cookie|set-cookie|x-api-key|x-.*-signature|x-.*-token|.*secret.*)$/i;
const SENSITIVE_JSON_KEY = /"(password|secret|token|api_key|access_token|authorization)"\s*:\s*"[^"]*"/gi;

export const MAX_BODY_CHARS = 16_384;

export function redactHeaders(headers: Record<string, string> | undefined): [string, string][] {
  return Object.entries(headers ?? {})
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([k, v]) => [k, SENSITIVE_HEADER.test(k) ? '•••••• (redacted)' : v]);
}

export function prepareBody(body: string | undefined, serverTruncated = false): { text: string; truncated: boolean } {
  let text = body ?? '';
  // Pretty-print JSON when possible for readability.
  try {
    text = JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    // Not JSON; show as-is.
  }
  text = text.replace(SENSITIVE_JSON_KEY, (_m, key: string) => `"${key}": "••••••"`);
  if (text.length > MAX_BODY_CHARS) {
    return { text: text.slice(0, MAX_BODY_CHARS), truncated: true };
  }
  return { text, truncated: serverTruncated };
}
