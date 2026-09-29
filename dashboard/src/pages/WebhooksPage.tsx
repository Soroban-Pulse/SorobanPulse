import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { dashboardApi, SubscriptionSummary, WebhookDelivery } from "../api/client";
import { Column, ResponsiveTable } from "../components/ResponsiveTable";
import { useFormat } from "../i18n/format";

/** Status code with a text outcome so success/failure is not conveyed by colour alone. */
export function DeliveryOutcome({ code }: { code: number }) {
  const { t } = useTranslation();
  const failed = code >= 400;
  return (
    <span className={failed ? "status-error" : "status-ok"}>
      {t(failed ? "webhooks.outcome.failure" : "webhooks.outcome.success", { code })}
    </span>
  );
}

export function WebhooksPage() {
  const { t } = useTranslation();
  const format = useFormat();
  const [subscriptions, setSubscriptions] = useState<SubscriptionSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [deliveries, setDeliveries] = useState<WebhookDelivery[]>([]);

  useEffect(() => {
    dashboardApi.listSubscriptions().then((subs) => {
      setSubscriptions(subs);
      if (subs.length > 0) setSelectedId(subs[0].id);
    });
  }, []);

  useEffect(() => {
    if (!selectedId) return;
    dashboardApi.listWebhookDeliveries(selectedId).then(setDeliveries);
  }, [selectedId]);

  const columns: Column<WebhookDelivery>[] = [
    { id: "attempt", header: t("webhooks.columns.attempt"), cell: (d) => format.number(d.attempt) },
    { id: "statusCode", header: t("webhooks.columns.statusCode"), cell: (d) => <DeliveryOutcome code={d.statusCode} />, key: true },
    {
      id: "deliveredAt",
      header: t("webhooks.columns.deliveredAt"),
      cell: (d) => <time dateTime={d.deliveredAt}>{format.dateTime(d.deliveredAt)}</time>,
      key: true,
    },
  ];

  return (
    <div className="webhooks-page">
      <h1>{t("webhooks.heading")}</h1>
      <div className="filters">
        <div className="field field-grow">
          <label htmlFor="webhook-subscription">{t("webhooks.subscription")}</label>
          <select id="webhook-subscription" value={selectedId ?? ""} onChange={(e) => setSelectedId(e.target.value)}>
            {subscriptions.map((sub) => (
              <option key={sub.id} value={sub.id}>
                {sub.contractId} → {sub.webhookUrl}
              </option>
            ))}
          </select>
        </div>
      </div>

      <ResponsiveTable
        caption={t("webhooks.tableCaption")}
        columns={columns}
        rows={deliveries}
        rowKey={(d) => d.id}
        emptyText={t("webhooks.empty")}
      />
    </div>
  );
}
