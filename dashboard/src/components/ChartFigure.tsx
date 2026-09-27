import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { MetricPoint } from "../api/client";
import { useFormat } from "../i18n/format";

interface ChartFigureProps {
  /** Translation namespace under "charts", e.g. "events". */
  name: "events" | "latency";
  dataKey: keyof Pick<MetricPoint, "eventsIngested" | "latencyMsP99">;
  data: MetricPoint[];
}

/**
 * Line chart with text alternatives: a role="img" summary for screen readers
 * and an expandable data table with every point.
 */
export function ChartFigure({ name, dataKey, data }: ChartFigureProps) {
  const { t } = useTranslation();
  const format = useFormat();
  const titleId = useId();

  const values = data.map((p) => p[dataKey]);
  const summary =
    values.length === 0
      ? t(`charts.${name}.empty`)
      : t(`charts.${name}.summary`, {
          latest: format.number(values[values.length - 1]),
          peak: format.number(Math.max(...values)),
        });

  return (
    <section className={`chart-section chart-${name}`} aria-labelledby={titleId}>
      <h2 id={titleId}>{t(`charts.${name}.title`)}</h2>
      <div role="img" aria-label={summary} className="chart-canvas">
        <ResponsiveContainer width="100%" height={260}>
          <LineChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="timestamp" tickFormatter={format.time} tick={{ fontSize: 12 }} minTickGap={24} />
            <YAxis tickFormatter={format.number} tick={{ fontSize: 12 }} width={48} />
            <Tooltip
              labelFormatter={(label) => format.dateTime(label as string)}
              formatter={(value) => [format.number(Number(value)), t(`charts.${name}.series`)]}
            />
            <Line type="monotone" dataKey={dataKey} className="chart-line" dot={false} isAnimationActive={false} />
          </LineChart>
        </ResponsiveContainer>
      </div>
      <details className="chart-data">
        <summary>{t("charts.showData")}</summary>
        <div className="table-scroll" role="region" aria-label={t(`charts.${name}.title`)} tabIndex={0}>
          <table>
            <caption className="visually-hidden">{t(`charts.${name}.title`)}</caption>
            <thead>
              <tr>
                <th scope="col">{t("charts.time")}</th>
                <th scope="col">{t(`charts.${name}.series`)}</th>
              </tr>
            </thead>
            <tbody>
              {data.map((point) => (
                <tr key={point.timestamp}>
                  <th scope="row">{format.dateTime(point.timestamp)}</th>
                  <td>{format.number(point[dataKey])}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </details>
    </section>
  );
}
