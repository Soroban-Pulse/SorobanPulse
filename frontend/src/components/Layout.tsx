import { useState } from "react";
import { Link, NavLink, Outlet } from "react-router-dom";
import { useThemeSync } from "../lib/theme";
import { GlobalSearch } from "./GlobalSearch";
import { SettingsDialog } from "./SettingsDialog";
import { ThemeToggle } from "./ThemeToggle";

export function Layout() {
  useThemeSync();
  const [settingsOpen, setSettingsOpen] = useState(false);
  return (
    <>
      <header className="app-header">
        <Link to="/" className="brand">
          Soroban Pulse
        </Link>
        <nav className="nav" aria-label="Main">
          <NavLink to="/explorer">Explorer</NavLink>
          <NavLink to="/status">Status</NavLink>
          <NavLink to="/admin">Admin</NavLink>
        </nav>
        <div className="header-spacer" />
        <GlobalSearch />
        <div className="header-actions">
          <ThemeToggle />
          <button type="button" className="ghost" onClick={() => setSettingsOpen(true)} aria-label="Settings" title="Settings">
            ⚙
          </button>
        </div>
      </header>
      <main>
        <Outlet context={{ openSettings: () => setSettingsOpen(true) }} />
      </main>
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </>
  );
}

export interface LayoutContext {
  openSettings: () => void;
}
