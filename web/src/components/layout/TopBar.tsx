import { useLocation } from "react-router-dom";
import { ConnectionStatus } from "../ConnectionStatus.tsx";

const PAGE_TITLES: Record<string, string> = {
  "/": "Overview",
  "/events": "Events",
  "/contracts": "Contracts",
  "/subscriptions": "Subscriptions",
  "/status": "Indexer Status",
};

export function TopBar() {
  const { pathname } = useLocation();
  const title = PAGE_TITLES[pathname] ?? "Soroban Pulse";

  return (
    <header className="topbar" role="banner">
      <span className="topbar__title">{title}</span>
      <div className="topbar__right">
        <ConnectionStatus />
      </div>
    </header>
  );
}
