import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { dashboardApi, MetricPoint, SystemStatus } from "../api/client";
import { ChartFigure } from "../components/ChartFigure";
import { LiveRegion } from "../components/LiveRegion";
import { StatTile } from "../components/StatTile";
import { useFormat } from "../i18n/format";

export function StatusDashboard() {
  const { t } = useTranslation();
  const format = useFormat();
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [metrics, setMetrics] = useState<MetricPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const previousStatus = useRef<SystemStatus["status"] | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const [statusResult, metricsResult] = await Promise.all([
          dashboardApi.getSystemStatus(),
          dashboardApi.getMetrics(60),
        ]);
        if (cancelled) return;
        setStatus(statusResult);
        setMetrics(metricsResult);
        setUpdatedAt(Date.now());
        setError(false);
      } catch {
        if (!cancelled) setError(true);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    load();
    const interval = setInterval(load, 15_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  // Announce health changes only; a 15s poll would otherwise be very chatty.
  useEffect(() => {
    if (!status) return;
    if (previousStatus.current && previousStatus.current !== status.status) {
      setAnnouncement(t("status.changedAnnouncement", { status: t(`status.value.${status.status}`) }));
    }
    previousStatus.current = status.status;
  }, [status, t]);

  if (loading) return <p>{t("status.loading")}</p>;

  return (
    <div className="status-dashboard">
      <div className="page-header">
        <h1>{t("status.heading")}</h1>
        {updatedAt && <p className="muted">{t("status.lastUpdated", { time: format.time(updatedAt) })}</p>}
      </div>
      <LiveRegion message={announcement} />
      {error && (
        <p className="error-text" role="alert">
          {t("common.loadError")}
        </p>
      )}

      <dl className="stat-row">
        <StatTile
          label={t("status.tileStatus")}
          value={t(`status.value.${status?.status ?? "unknown"}`)}
        />
        <StatTile label={t("status.tileUptime")} value={format.hours(status?.uptimeSeconds ?? 0)} />
        <StatTile label={t("status.tileVersion")} value={status?.version ?? t("common.notAvailable")} />
      </dl>

      <ChartFigure name="events" dataKey="eventsIngested" data={metrics} />
      <ChartFigure name="latency" dataKey="latencyMsP99" data={metrics} />
    </div>
  );
}
