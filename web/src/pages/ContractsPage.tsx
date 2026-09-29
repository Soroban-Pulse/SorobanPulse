import { useState } from "react";
import { useContracts } from "../api/hooks.ts";

/**
 * Paginated list of indexed contract IDs.
 */
export default function ContractsPage() {
  const [page, setPage] = useState(1);
  const limit = 50;

  const { data, isLoading, isError } = useContracts({ page, limit });
  const totalPages = data ? Math.ceil(data.total / limit) : 0;

  return (
    <div>
      <div className="page-header">
        <h1>Contracts</h1>
        <p>All Soroban contracts tracked by the indexer.</p>
      </div>

      <div className="card">
        {isLoading && <p className="text-muted">Loading contracts…</p>}
        {isError && <div className="error-banner">⚠ Failed to load contracts</div>}

        {data && (
          <>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>#</th>
                    <th>Contract ID</th>
                  </tr>
                </thead>
                <tbody>
                  {data.data.length === 0 ? (
                    <tr>
                      <td colSpan={2} style={{ textAlign: "center", padding: "2rem" }}>
                        <span className="text-muted">No contracts indexed yet.</span>
                      </td>
                    </tr>
                  ) : (
                    data.data.map((id, i) => (
                      <tr key={id}>
                        <td className="text-muted">{(page - 1) * limit + i + 1}</td>
                        <td className="mono">{id}</td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>

            <div className="pagination">
              <span className="pagination__info">
                {data.total.toLocaleString()} contracts · page {page}
                {totalPages > 0 ? ` of ${totalPages}` : ""}
              </span>
              <button
                className="btn"
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page === 1}
              >
                ← Prev
              </button>
              <button
                className="btn"
                onClick={() => setPage((p) => p + 1)}
                disabled={page >= totalPages}
              >
                Next →
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
