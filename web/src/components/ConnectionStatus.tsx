/**
 * ConnectionStatus
 *
 * Top-bar pill that reflects the current connection + auth state.
 *
 * States:
 *  - not configured  → grey "Not configured" (links to /settings)
 *  - loading         → amber pulsing dot
 *  - connected       → green dot + latest ledger
 *  - paused          → amber dot + ledger
 *  - auth failed     → red "401 / 403" message
 *  - unreachable     → red "Offline" message
 */

import { Link } from "react-router-dom";
import { useSettings } from "../context/SettingsContext.tsx";
import { useStatus } from "../api/hooks.ts";
import { ApiError } from "../api/client.ts";

export function ConnectionStatus() {
  const { isConfigured } = useSettings();
  const { data, isLoading, error } = useStatus({
    // Only poll when we have credentials; avoids a flood of failed requests
    // before the user has configured anything.
    enabled: isConfigured,
    // Don't retry auth failures — they won't self-heal.
    retry: (failureCount, err) => {
      if (err instanceof ApiError && err.isUnauthorized) return false;
      return failureCount < 2;
    },
  });

  // ── Not configured ──────────────────────────────────────────────────────────
  if (!isConfigured) {
    return (
      <Link to="/settings" className="conn-status conn-status--unconfigured" aria-label="Go to settings to configure the server connection">
        <span className="conn-status__dot" aria-hidden="true" />
        <span>Not configured</span>
      </Link>
    );
  }

  // ── Loading ─────────────────────────────────────────────────────────────────
  if (isLoading) {
    return (
      <div className="conn-status conn-status--loading" aria-live="polite">
        <span className="conn-status__dot" aria-hidden="true" />
        <span>Connecting…</span>
      </div>
    );
  }

  // ── Auth failure ────────────────────────────────────────────────────────────
  if (error instanceof ApiError && error.isUnauthorized) {
    return (
      <Link to="/settings" className="conn-status conn-status--auth-fail" aria-live="polite" title="Authentication failed — click to update your API key">
        <span className="conn-status__dot" aria-hidden="true" />
        <span>HTTP {error.status} — check API key</span>
      </Link>
    );
  }

  // ── Unreachable ─────────────────────────────────────────────────────────────
  if (error || !data) {
    return (
      <Link to="/settings" className="conn-status conn-status--offline" aria-live="polite" title="Server unreachable — click to update settings">
        <span className="conn-status__dot" aria-hidden="true" />
        <span>Offline</span>
      </Link>
    );
  }

  // ── Connected ───────────────────────────────────────────────────────────────
  const isIndexing = data.indexing;
  const stateClass = isIndexing ? "conn-status--online" : "conn-status--paused";
  const label = isIndexing
    ? `Indexing · ledger ${data.latest_ledger.toLocaleString()}`
    : `Paused · ledger ${data.latest_ledger.toLocaleString()}`;

  return (
    <div className={`conn-status ${stateClass}`} aria-live="polite" title={label}>
      <span className="conn-status__dot" aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}
