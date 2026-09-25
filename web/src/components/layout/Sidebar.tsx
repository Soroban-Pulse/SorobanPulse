import { NavLink } from "react-router-dom";

interface NavItem {
  to: string;
  icon: string;
  label: string;
}

const NAV_ITEMS: NavItem[] = [
  { to: "/", icon: "⚡", label: "Overview" },
  { to: "/events", icon: "📋", label: "Events" },
  { to: "/contracts", icon: "📄", label: "Contracts" },
  { to: "/subscriptions", icon: "🔔", label: "Subscriptions" },
];

const MONITORING_ITEMS: NavItem[] = [
  { to: "/status", icon: "🩺", label: "Indexer Status" },
];

export function Sidebar() {
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
      </ul>
    </aside>
  );
}
