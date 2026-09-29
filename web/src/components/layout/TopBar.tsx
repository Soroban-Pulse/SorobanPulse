import { Link, useLocation } from "react-router-dom";
import { ConnectionStatus } from "../ConnectionStatus.tsx";

const PAGE_TITLES: Record<string, string> = {
  "/": "Overview",
  "/events": "Events",
  "/events/stream": "Live Stream",
  "/contracts": "Contracts",
  "/subscriptions": "Subscriptions",
  "/status": "Indexer Status",
  "/settings": "Settings",
};

export function TopBar() {
  const { pathname } = useLocation();
  const title = PAGE_TITLES[pathname] ?? "Soroban Pulse";

  return (
    <header className="topbar" role="banner">
      <span className="topbar__title">{title}</span>
      <div className="topbar__right">
        <ConnectionStatus />
        <Link
          to="/settings"
          className="topbar__settings-btn"
          aria-label="Settings"
          title="Settings"
        >
          ⚙
        </Link>
      </div>
    </header>
  );
}
