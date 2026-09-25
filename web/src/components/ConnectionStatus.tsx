import { useStatus } from "../api/hooks.ts";

/**
 * Top-bar pill that shows live indexer connection state.
 * - Loading  → amber pulsing dot
 * - Indexing → green dot + latest ledger number
 * - Paused   → red dot
 * - Error    → red dot + short error hint
 */
export function ConnectionStatus() {
  const { data, isLoading, isError } = useStatus();

  if (isLoading) {
    return (
      <div className="conn-status conn-status--loading" aria-live="polite">
        <span className="conn-status__dot" />
        <span>Connecting…</span>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="conn-status conn-status--offline" aria-live="polite">
        <span className="conn-status__dot" />
        <span>Offline</span>
      </div>
    );
  }

  const state = data.indexing ? "conn-status--online" : "conn-status--offline";
  const label = data.indexing
    ? `Indexing · ledger ${data.latest_ledger.toLocaleString()}`
    : `Paused · ledger ${data.latest_ledger.toLocaleString()}`;

  return (
    <div className={`conn-status ${state}`} aria-live="polite" title={label}>
      <span className="conn-status__dot" />
      <span>{label}</span>
    </div>
  );
}
