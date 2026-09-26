import { ApiError } from '../../api/client';
import { contractsApi } from '../../api/contracts';
import { formatDate, formatNumber, truncateMiddle } from '../../lib/format';
import { useAsync } from '../../lib/useAsync';

/**
 * WASM upgrade history. Contract upgrade tracking is not available on every
 * deployment yet, so the section hides itself when the endpoint is missing.
 */
export function WasmTimeline({ contractId }: { contractId: string }) {
  const versions = useAsync(() => contractsApi.wasmVersions(contractId), [contractId]);

  if (versions.loading) return null;
  if (versions.error instanceof ApiError && (versions.error.status === 404 || versions.error.status === 501)) return null;
  if (versions.error) return null;

  const list = [...(versions.data?.data ?? [])].sort((a, b) => b.ledger - a.ledger);
  if (list.length === 0) return null;

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>WASM versions</h2>
        <span className="muted small">{list.length} deployed</span>
      </header>
      <ol className="timeline">
        {list.map((v, i) => (
          <li key={v.wasm_hash + v.ledger} className={i === 0 ? 'timeline-item current' : 'timeline-item'}>
            <span className="timeline-dot" aria-hidden="true" />
            <div>
              <div className="mono" title={v.wasm_hash}>
                {truncateMiddle(v.wasm_hash, 8)} {i === 0 && <span className="badge badge-ok">current</span>}
              </div>
              <div className="muted small">
                ledger #{formatNumber(v.ledger)} · {formatDate(v.upgraded_at)}
                {v.tx_hash ? ` · tx ${truncateMiddle(v.tx_hash, 6)}` : ''}
              </div>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
