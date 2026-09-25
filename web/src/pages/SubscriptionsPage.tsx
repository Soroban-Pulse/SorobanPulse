/**
 * Subscription management page.
 * Full implementation is tracked separately; this scaffold renders the
 * shell so routing and navigation work end-to-end.
 */
export default function SubscriptionsPage() {
  return (
    <div>
      <div className="page-header">
        <h1>Subscriptions</h1>
        <p>Manage webhook and alert subscriptions for contract events.</p>
      </div>

      <div className="card">
        <div className="empty-state">
          <div className="empty-state__icon">🔔</div>
          <p>Subscription management coming soon.</p>
        </div>
      </div>
    </div>
  );
}
