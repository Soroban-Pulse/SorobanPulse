import { useParams } from "react-router-dom";
import { EventList } from "../components/EventList";
import { api, errorMessage } from "../lib/api";
import { formatDateTime, formatNumber } from "../lib/format";
import { usePolling } from "../hooks/usePolling";

interface ContractSummary {
  total_events?: number;
  unique_tx_count?: number;
  first_event_at?: string | null;
  last_event_at?: string | null;
  ledger_range?: { min: number | null; max: number | null };
}

export function ContractPage() {
  const { contractId = "" } = useParams();
  const summary = usePolling(
    () => api<ContractSummary>(`/v1/contracts/${contractId}/summary`),
    0,
    [contractId],
  );
  const s = summary.data;

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <div className="muted">Contract</div>
          <h1 className="mono break">{contractId}</h1>
        </div>
      </div>
      <div className="card">
        {summary.error ? (
          <div className="muted">Summary unavailable: {errorMessage(summary.error)}</div>
        ) : (
          <div className="stats">
            <Stat label="Events" value={formatNumber(s?.total_events)} />
            <Stat label="Transactions" value={formatNumber(s?.unique_tx_count)} />
            <Stat label="First ledger" value={formatNumber(s?.ledger_range?.min)} />
            <Stat label="Last ledger" value={formatNumber(s?.ledger_range?.max)} />
            <Stat label="First event" value={formatDateTime(s?.first_event_at)} small />
            <Stat label="Last event" value={formatDateTime(s?.last_event_at)} small />
          </div>
        )}
      </div>
      <div className="card">
        <h2>Events</h2>
        <EventList path={`/v1/events/contract/${contractId}`} params={{ sort: "desc" }} />
      </div>
    </div>
  );
}

function Stat({ label, value, small }: { label: string; value: string; small?: boolean }) {
  return (
    <div className="stat">
      <div className="label">{label}</div>
      <div className="value" style={small ? { fontSize: 14 } : undefined}>
        {value}
      </div>
    </div>
  );
}
