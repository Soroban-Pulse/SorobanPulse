export type Level = "ok" | "warn" | "danger" | "neutral";

/**
 * Colour for indexer lag relative to INDEXER_LAG_WARN_THRESHOLD (from
 * /v1/config): amber above the threshold, red above 3× the threshold.
 */
export function lagLevel(lag: number, threshold: number): Level {
  if (lag > threshold * 3) return "danger";
  if (lag > threshold) return "warn";
  return "ok";
}
