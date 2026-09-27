import { useRef } from "react";
import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useAuth } from "../auth/AuthContext";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { ThemePreference } from "../hooks/useTheme";
import { CloseIcon } from "./Icons";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { ThemeToggle } from "./ThemeToggle";

interface SidebarProps {
  id: string;
  /** Below the mobile breakpoint the sidebar is an off-canvas modal drawer. */
  isDrawer: boolean;
  open: boolean;
  onClose: () => void;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
}

export function Sidebar({ id, isDrawer, open, onClose, theme, onThemeChange }: SidebarProps) {
  const { user, logout } = useAuth();
  const { t } = useTranslation();
  const ref = useRef<HTMLElement>(null);

  useFocusTrap(ref, isDrawer && open, onClose);

  const drawerProps = isDrawer
    ? { role: "dialog", "aria-modal": open || undefined, "aria-label": t("nav.menu") }
    : {};

  return (
    <aside id={id} ref={ref} className="sidebar" data-open={open} tabIndex={-1} {...drawerProps}>
      <div className="sidebar-header">
        <span className="sidebar-brand">{t("app.name")}</span>
        {isDrawer && (
          <button type="button" className="icon-button" aria-label={t("nav.closeMenu")} onClick={onClose}>
            <CloseIcon />
          </button>
        )}
      </div>

      {user && (
        <nav aria-label={t("nav.primary")}>
          <ul className="nav-list">
            <li>
              <NavLink to="/" end className="nav-link" onClick={onClose}>
                {t("nav.status")}
              </NavLink>
            </li>
            <li>
              <NavLink to="/events" className="nav-link" onClick={onClose}>
                {t("nav.events")}
              </NavLink>
            </li>
            <li>
              <NavLink to="/subscriptions" className="nav-link" onClick={onClose}>
                {t("nav.subscriptions")}
              </NavLink>
            </li>
            <li>
              <NavLink to="/webhooks" className="nav-link" onClick={onClose}>
                {t("nav.webhooks")}
              </NavLink>
            </li>
            <li>
              <NavLink to="/live" className="nav-link" onClick={onClose}>
                {t("nav.live")}
              </NavLink>
            </li>
          </ul>
        </nav>
      )}

      <div className="sidebar-footer">
        <LanguageSwitcher />
        <ThemeToggle theme={theme} onChange={onThemeChange} />
        {user && (
          <button type="button" onClick={logout}>
            {t("nav.signOut")}
          </button>
        )}
      </div>
    </aside>
  );
}
