import { Link, useParams } from 'react-router-dom';
import { contractsApi } from '../../api/contracts';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { BarList } from '../../components/charts/BarList';
import { useAsync } from '../../lib/useAsync';
import { isContractId } from '../../lib/validation';
import { ActivityChart } from './ActivityChart';
import { ContractEventsTable } from './ContractEventsTable';
import { ContractHeader } from './ContractHeader';
import { SummaryCards } from './SummaryCards';
import { WasmTimeline } from './WasmTimeline';

/**
 * Contract home page. Every chart reads from aggregate endpoints (summary
 * materialized view, stats history, timeseries) so load time does not grow
 * with the contract's event count.
 */
export function ContractOverviewPage() {
  const { id = '' } = useParams();
  const summary = useAsync(() => contractsApi.summary(id), [id]);

  if (!isContractId(id)) {
    return (
      <div className="page">
        <p className="alert alert-error">
          <span className="mono">{id}</span> is not a valid contract ID. <Link to="/contracts">Search contracts</Link>
        </p>
      </div>
    );
  }

  const s = summary.data;
  const breakdown = s
    ? [
        { label: 'contract', value: s.event_type_breakdown.contract },
        { label: 'diagnostic', value: s.event_type_breakdown.diagnostic },
        { label: 'system', value: s.event_type_breakdown.system },
      ]
    : [];

  return (
    <div className="page">
      <nav className="breadcrumbs">
        <Link to="/contracts">Contracts</Link> / <span className="mono">{id.slice(0, 8)}…</span>
      </nav>

      <ContractHeader contractId={id} />

      <AsyncBoundary loading={summary.loading} error={summary.error} onRetry={summary.reload}>
        {s && <SummaryCards summary={s} />}
      </AsyncBoundary>

      <div className="two-col wide-left">
        {/* Wait for the summary so the 24h query can be bounded by the latest ledger. */}
        {summary.loading ? (
          <section className="panel">
            <div className="skeleton" />
          </section>
        ) : (
          <ActivityChart contractId={id} maxLedger={s?.ledger_range.max ?? null} />
        )}
        <section className="panel">
          <header className="panel-header">
            <h2>Top event types</h2>
          </header>
          <AsyncBoundary loading={summary.loading} error={summary.error}>
            <BarList items={breakdown} ariaLabel="Events by type" />
          </AsyncBoundary>
        </section>
      </div>

      <WasmTimeline contractId={id} />
      <ContractEventsTable contractId={id} />
    </div>
  );
}
