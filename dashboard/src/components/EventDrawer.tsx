import { useEffect } from "react";
import { type Event } from "../api/eventTypes";
import { ScValViewer } from "./ScValViewer";
import { TruncatedText } from "./TruncatedText";

interface EventDrawerProps {
  event: Event | null;
  onClose: () => void;
}

export function EventDrawer({ event, onClose }: EventDrawerProps) {
  // Close on Escape key
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  if (!event) return null;

  return (
    <div className="event-drawer-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div className="event-drawer" onClick={(e) => e.stopPropagation()}>
        <div className="event-drawer-header">
          <h2>Event Detail</h2>
          <button className="event-drawer-close" onClick={onClose} aria-label="Close detail">
            ✕
          </button>
        </div>

        <div className="event-drawer-body">
          <section className="event-drawer-section">
            <h3>Summary</h3>
            <dl className="event-drawer-meta">
              <dt>Event ID</dt>
              <dd>
                <TruncatedText text={event.id} maxChars={16} />
              </dd>

              <dt>Tx Hash</dt>
              <dd>
                <TruncatedText text={event.txHash} maxChars={16} />
              </dd>

              <dt>Ledger</dt>
              <dd>{event.ledger.toLocaleString()}</dd>

              <dt>Contract</dt>
              <dd>
                <TruncatedText text={event.contractId} maxChars={12} />
              </dd>

              <dt>Type</dt>
              <dd>
                <span className={`event-type-badge event-type-badge--${event.eventType}`}>
                  {event.eventType}
                </span>
              </dd>

              <dt>Timestamp</dt>
              <dd>{new Date(event.timestamp).toLocaleString()}</dd>

              <dt>Successful Call</dt>
              <dd>{event.inSuccessfulCall ? "✓ Yes" : "✗ No"}</dd>

              <dt>Schema Version</dt>
              <dd>{event.schemaVersion}</dd>

              <dt>Anonymized</dt>
              <dd>{event.anonymized ? "Yes" : "No"}</dd>

              {event.fingerprint && (
                <>
                  <dt>Fingerprint</dt>
                  <dd>
                    <TruncatedText text={event.fingerprint} maxChars={16} />
                  </dd>
                </>
              )}

              <dt>Tenant</dt>
              <dd>{event.tenantId}</dd>

              <dt>Network</dt>
              <dd>{event.network}</dd>
            </dl>
          </section>

          <section className="event-drawer-section">
            <h3>ScVal Data</h3>
            <ScValViewer data={event.eventData} />
          </section>

          <section className="event-drawer-section">
            <h3>Raw JSON</h3>
            <pre className="scval-code">
              <code>{JSON.stringify(event.eventData, null, 2)}</code>
            </pre>
          </section>
        </div>
      </div>
    </div>
  );
}
