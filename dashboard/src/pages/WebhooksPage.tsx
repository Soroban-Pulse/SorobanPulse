import { useEffect, useState } from "react";
import { dashboardApi, SubscriptionSummary, WebhookDelivery } from "../api/client";

export function WebhooksPage() {
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

  return (
    <div className="webhooks-page">
      <h1>Webhook Deliveries</h1>
      <select value={selectedId ?? ""} onChange={(e) => setSelectedId(e.target.value)}>
        {subscriptions.map((sub) => (
          <option key={sub.id} value={sub.id}>
            {sub.contractId} → {sub.webhookUrl}
          </option>
        ))}
      </select>

      <table>
        <thead>
          <tr>
            <th>Attempt</th>
            <th>Status code</th>
            <th>Delivered at</th>
          </tr>
        </thead>
        <tbody>
          {deliveries.map((delivery) => (
            <tr key={delivery.id}>
              <td>{delivery.attempt}</td>
              <td className={delivery.statusCode >= 400 ? "status-error" : "status-ok"}>
                {delivery.statusCode}
              </td>
              <td>{new Date(delivery.deliveredAt).toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
