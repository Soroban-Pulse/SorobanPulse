import { useMemo, useRef, useState } from 'react';
import { formatCompact, formatNumber } from '../../lib/format';

export interface SeriesPoint {
  t: Date;
  value: number;
}

interface TimeSeriesChartProps {
  points: SeriesPoint[];
  /** Name of the measure, used in the tooltip and table header. */
  measure: string;
  formatTick: (d: Date) => string;
  formatTooltip: (d: Date) => string;
  height?: number;
}

const PAD = { top: 12, right: 12, bottom: 24, left: 44 };
const WIDTH = 720;

function niceMax(v: number): number {
  if (v <= 0) return 1;
  const mag = 10 ** Math.floor(Math.log10(v));
  const n = v / mag;
  const step = n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10;
  return step * mag;
}

/**
 * Single-series area chart with a crosshair tooltip and a table view.
 * One series, so the section title names it and no legend is drawn.
 */
export function TimeSeriesChart({ points, measure, formatTick, formatTooltip, height = 220 }: TimeSeriesChartProps) {
  const [hover, setHover] = useState<number | null>(null);
  const [asTable, setAsTable] = useState(false);
  const svgRef = useRef<SVGSVGElement>(null);

  const geom = useMemo(() => {
    const innerW = WIDTH - PAD.left - PAD.right;
    const innerH = height - PAD.top - PAD.bottom;
    const max = niceMax(Math.max(0, ...points.map((p) => p.value)));
    const n = points.length;
    const x = (i: number) => PAD.left + (n > 1 ? (i / (n - 1)) * innerW : innerW / 2);
    const y = (v: number) => PAD.top + innerH - (v / max) * innerH;
    const line = points.map((p, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)},${y(p.value).toFixed(1)}`).join(' ');
    const area = n ? `${line} L${x(n - 1).toFixed(1)},${y(0)} L${x(0).toFixed(1)},${y(0)} Z` : '';
    const yTicks = [0, max / 2, max];
    const tickEvery = Math.max(1, Math.ceil(n / 6));
    return { x, y, line, area, yTicks, tickEvery, innerW };
  }, [points, height]);

  if (points.length === 0) return <div className="empty">No events in this range.</div>;

  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const px = ((e.clientX - rect.left) / rect.width) * WIDTH;
    const rel = (px - PAD.left) / geom.innerW;
    const i = Math.round(rel * (points.length - 1));
    setHover(Math.max(0, Math.min(points.length - 1, i)));
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowRight') setHover((h) => Math.min(points.length - 1, (h ?? -1) + 1));
    if (e.key === 'ArrowLeft') setHover((h) => Math.max(0, (h ?? points.length) - 1));
  };

  const hp = hover !== null ? points[hover] : null;

  return (
    <div className="chart">
      <div className="chart-toolbar">
        <button type="button" className="btn btn-small btn-ghost" onClick={() => setAsTable((v) => !v)}>
          {asTable ? 'Show chart' : 'Show table'}
        </button>
      </div>
      {asTable ? (
        <div className="table-wrap chart-table">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th className="num">{measure}</th>
              </tr>
            </thead>
            <tbody>
              {points.map((p) => (
                <tr key={p.t.toISOString()}>
                  <td>{formatTooltip(p.t)}</td>
                  <td className="num">{formatNumber(p.value)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="chart-plot">
          <svg
            ref={svgRef}
            viewBox={`0 0 ${WIDTH} ${height}`}
            role="img"
            aria-label={`${measure} over time`}
            tabIndex={0}
            onPointerMove={onMove}
            onPointerLeave={() => setHover(null)}
            onKeyDown={onKey}
            onBlur={() => setHover(null)}
          >
            {geom.yTicks.map((t) => (
              <g key={t}>
                <line x1={PAD.left} x2={WIDTH - PAD.right} y1={geom.y(t)} y2={geom.y(t)} className="grid" />
                <text x={PAD.left - 6} y={geom.y(t)} className="axis-label" textAnchor="end" dominantBaseline="middle">
                  {formatCompact(t)}
                </text>
              </g>
            ))}
            {points.map((p, i) =>
              i % geom.tickEvery === 0 ? (
                <text key={i} x={geom.x(i)} y={height - 6} className="axis-label" textAnchor="middle">
                  {formatTick(p.t)}
                </text>
              ) : null,
            )}
            <path d={geom.area} fill="var(--series-1)" fillOpacity={0.12} />
            <path d={geom.line} fill="none" stroke="var(--series-1)" strokeWidth={2} strokeLinejoin="round" />
            {hp && hover !== null && (
              <>
                <line x1={geom.x(hover)} x2={geom.x(hover)} y1={PAD.top} y2={height - PAD.bottom} className="crosshair" />
                <circle cx={geom.x(hover)} cy={geom.y(hp.value)} r={4} fill="var(--series-1)" stroke="var(--surface-1)" strokeWidth={2} />
              </>
            )}
          </svg>
          {hp && hover !== null && (
            <div
              className="chart-tooltip"
              style={{
                left: `${(geom.x(hover) / WIDTH) * 100}%`,
                transform: hover > points.length / 2 ? 'translateX(calc(-100% - 8px))' : 'translateX(8px)',
              }}
            >
              <strong>{formatNumber(hp.value)}</strong> {measure.toLowerCase()}
              <div className="muted">{formatTooltip(hp.t)}</div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
