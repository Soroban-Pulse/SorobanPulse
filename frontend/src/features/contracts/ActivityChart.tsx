import { useState } from 'react';
import { contractsApi } from '../../api/contracts';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { TimeSeriesChart, type SeriesPoint } from '../../components/charts/TimeSeriesChart';
import { useAsync } from '../../lib/useAsync';

type Range = '24h' | '7d' | '30d';
const RANGES: Range[] = ['24h', '7d', '30d'];

// Stellar closes a ledger roughly every 5–6s; pad the lower bound so the
// ledger window always covers the full 24 hours.
const LEDGERS_PER_DAY_UPPER = 20_000;

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

/** Fill missing buckets with zero so gaps read as "no activity", not interpolated lines. */
function fillBuckets(counts: Map<number, number>, start: number, step: number, n: number): SeriesPoint[] {
  return Array.from({ length: n }, (_, i) => {
    const t = start + i * step;
    return { t: new Date(t), value: counts.get(t) ?? 0 };
  });
}

async function load(contractId: string, range: Range, maxLedger: number | null): Promise<SeriesPoint[]> {
  const now = Date.now();
  if (range === '24h') {
    // Hourly buckets from the aggregate endpoint, bounded by ledger so the query stays small.
    const res = await contractsApi.timeseries(contractId, '1h', maxLedger ? Math.max(0, maxLedger - LEDGERS_PER_DAY_UPPER) : undefined);
    const start = Math.floor((now - 23 * HOUR) / HOUR) * HOUR;
    const counts = new Map<number, number>();
    for (const b of res.data) counts.set(new Date(b.bucket_start).getTime(), b.event_count);
    return fillBuckets(counts, start, HOUR, 24);
  }
  // Daily buckets come from the pre-aggregated stats history.
  const days = range === '7d' ? 7 : 30;
  const res = await contractsApi.statsHistory(contractId, days);
  const today = new Date();
  const startUtc = Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate()) - (days - 1) * DAY;
  const counts = new Map<number, number>();
  for (const p of res.data) counts.set(new Date(`${p.date}T00:00:00Z`).getTime(), p.event_count);
  return fillBuckets(counts, startUtc, DAY, days);
}

export function ActivityChart({ contractId, maxLedger }: { contractId: string; maxLedger: number | null }) {
  const [range, setRange] = useState<Range>('7d');
  const series = useAsync(() => load(contractId, range, maxLedger), [contractId, range, maxLedger]);

  const hourly = range === '24h';
  const tick = (d: Date) =>
    hourly
      ? d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
      : d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });
  const tip = (d: Date) =>
    hourly
      ? d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
      : d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric', timeZone: 'UTC' });

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Events {hourly ? 'per hour' : 'per day'}</h2>
        <div className="segmented" role="group" aria-label="Time range">
          {RANGES.map((r) => (
            <button key={r} type="button" aria-pressed={range === r} className={range === r ? 'active' : undefined} onClick={() => setRange(r)}>
              {r}
            </button>
          ))}
        </div>
      </header>
      <AsyncBoundary loading={series.loading} error={series.error} onRetry={series.reload}>
        <TimeSeriesChart points={series.data ?? []} measure="Events" formatTick={tick} formatTooltip={tip} />
      </AsyncBoundary>
    </section>
  );
}
