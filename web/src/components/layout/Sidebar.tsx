import { NavLink } from "react-router-dom";
import { useSettings } from "../../context/SettingsContext.tsx";

interface NavItem {
  to: string;
  icon: string;
  label: string;
}

const NAV_ITEMS: NavItem[] = [
  { to: "/", icon: "⚡", label: "Overview" },
  { to: "/events", icon: "📋", label: "Events" },
  { to: "/events/stream", icon: "📡", label: "Live Stream" },
  { to: "/contracts", icon: "📄", label: "Contracts" },
  { to: "/subscriptions", icon: "🔔", label: "Subscriptions" },
];

const MONITORING_ITEMS: NavItem[] = [
  { to: "/status", icon: "🩺", label: "Indexer Status" },
];

/** Admin-only nav items — only shown when an admin key is configured. */
const ADMIN_ITEMS: NavItem[] = [
  { to: "/admin/subscriptions", icon: "🔑", label: "Manage Subscriptions" },
  { to: "/admin/keys", icon: "🗝", label: "API Keys" },
];

export function Sidebar() {
  const { hasAdminKey } = useSettings();

  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <span className="sidebar__brand-icon">🌐</span>
        Soroban Pulse
      </div>

      <ul className="sidebar__nav" role="navigation" aria-label="Main navigation">
        {NAV_ITEMS.map(({ to, icon, label }) => (
          <li key={to} className="sidebar__nav-item">
            <NavLink to={to} end={to === "/"}>
              <span aria-hidden="true">{icon}</span>
              {label}
            </NavLink>
          </li>
        ))}

        <li className="sidebar__nav-section">Monitoring</li>

        {MONITORING_ITEMS.map(({ to, icon, label }) => (
          <li key={to} className="sidebar__nav-item">
            <NavLink to={to}>
              <span aria-hidden="true">{icon}</span>
              {label}
            </NavLink>
          </li>
        ))}

        {hasAdminKey && (
          <>
            <li className="sidebar__nav-section">Admin</li>
            {ADMIN_ITEMS.map(({ to, icon, label }) => (
              <li key={to} className="sidebar__nav-item">
                <NavLink to={to}>
                  <span aria-hidden="true">{icon}</span>
                  {label}
                </NavLink>
              </li>
            ))}
          </>
        )}
      </ul>

      <div className="sidebar__footer">
        <NavLink to="/settings" className="sidebar__settings-link">
          <span aria-hidden="true">⚙</span>
          Settings
        </NavLink>
      </div>
    </aside>
  );
}
