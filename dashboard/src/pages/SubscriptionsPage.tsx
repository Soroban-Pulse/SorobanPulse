import { useEffect, useState } from "react";
import { dashboardApi, SubscriptionSummary } from "../api/client";

export function SubscriptionsPage() {
  const [subscriptions, setSubscriptions] = useState<SubscriptionSummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    dashboardApi
      .listSubscriptions()
      .then(setSubscriptions)
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <p>Loading subscriptions…</p>;

  return (
    <div className="subscriptions-page">
      <h1>Subscriptions</h1>
      <table>
        <thead>
          <tr>
            <th>Contract</th>
            <th>Webhook URL</th>
            <th>Event types</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {subscriptions.map((sub) => (
            <tr key={sub.id}>
              <td>{sub.contractId}</td>
              <td>{sub.webhookUrl}</td>
              <td>{sub.eventTypes.join(", ")}</td>
              <td>
                <span className={`status-badge status-${sub.status}`}>{sub.status}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
