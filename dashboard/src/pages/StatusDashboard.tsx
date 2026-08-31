import { useEffect, useState } from "react";
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer } from "recharts";
import { dashboardApi, MetricPoint, SystemStatus } from "../api/client";
import { StatTile } from "../components/StatTile";

export function StatusDashboard() {
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [metrics, setMetrics] = useState<MetricPoint[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      const [statusResult, metricsResult] = await Promise.all([
        dashboardApi.getSystemStatus(),
        dashboardApi.getMetrics(60),
      ]);
      if (!cancelled) {
        setStatus(statusResult);
        setMetrics(metricsResult);
        setLoading(false);
      }
    }

    load();
    const interval = setInterval(load, 15_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  if (loading) return <p>Loading system status…</p>;

  return (
    <div className="status-dashboard">
      <div className="stat-row">
        <StatTile label="Status" value={status?.status ?? "unknown"} />
        <StatTile label="Uptime" value={`${Math.floor((status?.uptimeSeconds ?? 0) / 3600)}h`} />
        <StatTile label="Version" value={status?.version ?? "-"} />
      </div>

      <section className="chart-section">
        <h2>Events ingested (last hour)</h2>
        <ResponsiveContainer width="100%" height={300}>
          <LineChart data={metrics}>
            <XAxis dataKey="timestamp" tick={{ fontSize: 10 }} />
            <YAxis />
            <Tooltip />
            <Line type="monotone" dataKey="eventsIngested" stroke="#5b8def" dot={false} />
          </LineChart>
        </ResponsiveContainer>
      </section>

      <section className="chart-section">
        <h2>p99 latency (ms)</h2>
        <ResponsiveContainer width="100%" height={300}>
          <LineChart data={metrics}>
            <XAxis dataKey="timestamp" tick={{ fontSize: 10 }} />
            <YAxis />
            <Tooltip />
            <Line type="monotone" dataKey="latencyMsP99" stroke="#e0725b" dot={false} />
          </LineChart>
        </ResponsiveContainer>
      </section>
    </div>
  );
}
