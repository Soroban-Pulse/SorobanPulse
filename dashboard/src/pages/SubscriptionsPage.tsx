import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { dashboardApi, SubscriptionSummary, WebhookDelivery } from "../api/client";
import { DetailDrawer } from "../components/DetailDrawer";
import { LiveRegion } from "../components/LiveRegion";
import { Column, ResponsiveTable } from "../components/ResponsiveTable";
import { StatusBadge } from "../components/StatusBadge";
import { DeliveryOutcome } from "./WebhooksPage";
import { useFormat } from "../i18n/format";

type StatusFilter = SubscriptionSummary["status"] | "all";

export function SubscriptionsPage() {
  const { t } = useTranslation();
  const [subscriptions, setSubscriptions] = useState<SubscriptionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [selected, setSelected] = useState<SubscriptionSummary | null>(null);

  useEffect(() => {
    dashboardApi
      .listSubscriptions()
      .then(setSubscriptions)
      .catch(() => setError(true))
      .finally(() => setLoading(false));
  }, []);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return subscriptions.filter(
      (sub) =>
        (statusFilter === "all" || sub.status === statusFilter) &&
        (!needle ||
          sub.contractId.toLowerCase().includes(needle) ||
          sub.webhookUrl.toLowerCase().includes(needle)),
    );
  }, [subscriptions, query, statusFilter]);

  const columns: Column<SubscriptionSummary>[] = [
    { id: "contract", header: t("subscriptions.columns.contract"), cell: (s) => <code className="mono">{s.contractId}</code> },
    { id: "webhookUrl", header: t("subscriptions.columns.webhookUrl"), cell: (s) => s.webhookUrl },
    { id: "eventTypes", header: t("subscriptions.columns.eventTypes"), cell: (s) => s.eventTypes.join(", "), key: true },
    { id: "status", header: t("subscriptions.columns.status"), cell: (s) => <StatusBadge status={s.status} />, key: true },
  ];

  if (loading) return <p>{t("subscriptions.loading")}</p>;

  const resultCount = t("subscriptions.resultCount", { count: filtered.length, total: subscriptions.length });

  return (
    <div className="subscriptions-page">
      <h1>{t("subscriptions.heading")}</h1>
      {error && (
        <p className="error-text" role="alert">
          {t("common.loadError")}
        </p>
      )}

      <form role="search" aria-label={t("subscriptions.filters")} className="filters" onSubmit={(e) => e.preventDefault()}>
        <div className="field field-grow">
          <label htmlFor="sub-search">{t("subscriptions.search")}</label>
          <input id="sub-search" type="search" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="field">
          <label htmlFor="sub-status">{t("subscriptions.statusFilter")}</label>
          <select id="sub-status" value={statusFilter} onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}>
            <option value="all">{t("subscriptions.statusAll")}</option>
            <option value="active">{t("subscriptions.statusValue.active")}</option>
            <option value="paused">{t("subscriptions.statusValue.paused")}</option>
            <option value="failing">{t("subscriptions.statusValue.failing")}</option>
          </select>
        </div>
      </form>
      <p className="muted" aria-hidden="true">
        {resultCount}
      </p>
      <LiveRegion message={resultCount} />

      <ResponsiveTable
        caption={t("subscriptions.tableCaption")}
        columns={columns}
        rows={filtered}
        rowKey={(s) => s.id}
        emptyText={t("subscriptions.empty")}
        onOpen={setSelected}
        openLabel={(s) => t("subscriptions.openDetail", { contract: s.contractId })}
      />

      <DetailDrawer open={selected !== null} title={t("subscriptions.detail.title")} onClose={() => setSelected(null)}>
        {selected && <SubscriptionDetail subscription={selected} />}
      </DetailDrawer>
    </div>
  );
}

function SubscriptionDetail({ subscription }: { subscription: SubscriptionSummary }) {
  const { t } = useTranslation();
  const format = useFormat();
  const [deliveries, setDeliveries] = useState<WebhookDelivery[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setDeliveries(null);
    dashboardApi
      .listWebhookDeliveries(subscription.id)
      .then((d) => !cancelled && setDeliveries(d))
      .catch(() => !cancelled && setDeliveries([]));
    return () => {
      cancelled = true;
    };
  }, [subscription.id]);

  return (
    <>
      <dl className="detail-list">
        <dt>{t("subscriptions.detail.id")}</dt>
        <dd className="mono">{subscription.id}</dd>
        <dt>{t("subscriptions.columns.contract")}</dt>
        <dd className="mono">{subscription.contractId}</dd>
        <dt>{t("subscriptions.columns.webhookUrl")}</dt>
        <dd>{subscription.webhookUrl}</dd>
        <dt>{t("subscriptions.columns.eventTypes")}</dt>
        <dd>{subscription.eventTypes.join(", ")}</dd>
        <dt>{t("subscriptions.columns.status")}</dt>
        <dd>
          <StatusBadge status={subscription.status} />
        </dd>
      </dl>

      <h3>{t("subscriptions.detail.recentDeliveries")}</h3>
      {deliveries === null ? (
        <p>{t("subscriptions.detail.loadingDeliveries")}</p>
      ) : deliveries.length === 0 ? (
        <p className="empty-text">{t("subscriptions.detail.noDeliveries")}</p>
      ) : (
        <ul className="plain-list">
          {deliveries.slice(0, 10).map((d) => (
            <li key={d.id}>
              <DeliveryOutcome code={d.statusCode} />{" "}
              <time dateTime={d.deliveredAt}>{format.dateTime(d.deliveredAt)}</time>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
