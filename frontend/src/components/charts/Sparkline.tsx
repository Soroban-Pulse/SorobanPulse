import { useState } from 'react';

interface SparklineProps {
  /** Values in chronological order. */
  values: number[];
  /** Optional per-point failure counts; points with failures get a critical marker. */
  failures?: number[];
  labels?: string[];
  width?: number;
  height?: number;
  ariaLabel: string;
}

/**
 * Compact trend line for table rows. Single series, so no legend; the column
 * header names it. Failures are marked with the reserved critical color.
 */
export function Sparkline({ values, failures, labels, width = 120, height = 28, ariaLabel }: SparklineProps) {
  const [hover, setHover] = useState<number | null>(null);
  if (values.length === 0) return <span className="muted">no data</span>;

  const pad = 3;
  const max = Math.max(1, ...values);
  const stepX = values.length > 1 ? (width - pad * 2) / (values.length - 1) : 0;
  const x = (i: number) => pad + i * stepX;
  const y = (v: number) => height - pad - (v / max) * (height - pad * 2);
  const path = values.map((v, i) => `${i === 0 ? 'M' : 'L'}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(' ');

  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - rect.left) / rect.width) * width;
    const i = stepX ? Math.round((px - pad) / stepX) : 0;
    setHover(Math.max(0, Math.min(values.length - 1, i)));
  };

  const total = values.reduce((a, b) => a + b, 0);
  const totalFailed = failures?.reduce((a, b) => a + b, 0) ?? 0;

  return (
    <span className="sparkline">
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={`${ariaLabel}: ${total} sent, ${totalFailed} failed`}
        onPointerMove={onMove}
        onPointerLeave={() => setHover(null)}
      >
        <path d={path} fill="none" stroke="var(--series-1)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
        {failures?.map((f, i) =>
          f > 0 ? (
            <circle key={i} cx={x(i)} cy={y(values[i])} r={3} fill="var(--status-critical)" stroke="var(--surface-1)" strokeWidth={1.5} />
          ) : null,
        )}
        {hover !== null && (
          <>
            <line x1={x(hover)} x2={x(hover)} y1={0} y2={height} stroke="var(--text-muted)" strokeWidth={1} />
            <circle cx={x(hover)} cy={y(values[hover])} r={3} fill="var(--series-1)" stroke="var(--surface-1)" strokeWidth={1.5} />
          </>
        )}
      </svg>
      {hover !== null && (
        <span className="chart-tooltip sparkline-tooltip">
          <strong>{values[hover]}</strong> sent
          {failures && (
            <>
              {' · '}
              <strong>{failures[hover] ?? 0}</strong> failed
            </>
          )}
          {labels?.[hover] && <span className="muted"> · {labels[hover]}</span>}
        </span>
      )}
    </span>
  );
}
