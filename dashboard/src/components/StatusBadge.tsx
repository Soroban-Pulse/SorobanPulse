import { useTranslation } from "react-i18next";
import { SubscriptionSummary } from "../api/client";

export function StatusBadge({ status }: { status: SubscriptionSummary["status"] }) {
  const { t } = useTranslation();
  return <span className={`status-badge status-${status}`}>{t(`subscriptions.statusValue.${status}`)}</span>;
}
