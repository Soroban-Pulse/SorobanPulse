import { useState, useEffect } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { dashboardApi, type Event } from "../api/client";
import { ScValViewer } from "../components/ScValViewer";
import { TruncatedText } from "../components/TruncatedText";

export function EventDetailPage() {
  const { eventId } = useParams<{ eventId: string }>();
  const navigate = useNavigate();
  const [event, setEvent] = useState<Event | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!eventId) return;

    setLoading(true);
    dashboardApi
      .getEventById(eventId)
      .then(setEvent)
      .catch(() => setEvent(null))
      .finally(() => setLoading(false));
  }, [eventId]);

  if (loading) {
    return (
      <div className="event-detail-page">
        <p>Loading event…</p>
      </div>
    );
  }

  if (!event) {
    return (
      <div className="event-detail-page">
        <p>Event not found.</p>
        <button className="btn btn-secondary" onClick={() => navigate(-1)}>
          Go Back
        </button>
      </div>
    );
  }

  return (
    <div className="event-detail-page">
      <div className="event-detail-header">
        <button className="btn btn-secondary" onClick={() => navigate(-1)}>
          ← Back
        </button>
        <h1>Event Detail</h1>
      </div>

      <section className="event-detail-section">
        <h2>Summary</h2>
        <dl className="event-detail-meta">
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
          <dd>{event.eventType}</dd>
          <dt>Timestamp</dt>
          <dd>{new Date(event.timestamp).toLocaleString()}</dd>
          <dt>Successful Call</dt>
          <dd>{event.inSuccessfulCall ? "Yes" : "No"}</dd>
          <dt>Schema Version</dt>
          <dd>{event.schemaVersion}</dd>
          <dt>Anonymized</dt>
          <dd>{event.anonymized ? "Yes" : "No"}</dd>
          <dt>Tenant</dt>
          <dd>{event.tenantId}</dd>
          <dt>Network</dt>
          <dd>{event.network}</dd>
        </dl>
      </section>

      <section className="event-detail-section">
        <h2>ScVal Data</h2>
        <ScValViewer data={event.eventData} />
      </section>

      <section className="event-detail-section">
        <h2>Raw JSON</h2>
        <pre className="scval-code">
          <code>{JSON.stringify(event.eventData, null, 2)}</code>
        </pre>
      </section>
    </div>
  );
}
