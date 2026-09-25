import { useStatus } from "../api/hooks.ts";

/**
 * Detailed indexer status page.
 */
export default function StatusPage() {
  const { data, isLoading, isError, dataUpdatedAt } = useStatus();

  return (
    <div>
      <div className="page-header">
        <h1>Indexer Status</h1>
        <p>Live health and progress of the SorobanPulse indexer.</p>
      </div>

      {isLoading && <p className="text-muted">Fetching status…</p>}
      {isError && <div className="error-banner">⚠ Could not reach the indexer API</div>}

      {data && (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
            gap: "1rem",
          }}
        >
          <InfoCard label="State" value={data.indexing ? "🟢 Indexing" : "🔴 Paused"} />
          <InfoCard label="Latest Ledger" value={data.latest_ledger.toLocaleString()} />
          {dataUpdatedAt > 0 && (
            <InfoCard
              label="Last Checked"
              value={new Date(dataUpdatedAt).toLocaleTimeString()}
            />
          )}
        </div>
      )}
    </div>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="card">
      <p className="text-muted" style={{ fontSize: "0.75rem", marginBottom: "0.25rem" }}>
        {label}
      </p>
      <p style={{ fontSize: "1.125rem", fontWeight: 600 }}>{value}</p>
    </div>
  );
}
