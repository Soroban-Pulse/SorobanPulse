import { useEffect, useRef, useState } from "react";
import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { RequireAuth } from "./auth/RequireAuth";
import { LoginPage } from "./pages/LoginPage";
import { StatusDashboard } from "./pages/StatusDashboard";
import { SubscriptionsPage } from "./pages/SubscriptionsPage";
import { WebhooksPage } from "./pages/WebhooksPage";
import { EventExplorerPage } from "./pages/EventExplorerPage";
import { EventDetailPage } from "./pages/EventDetailPage";
import { LiveStreamPage } from "./pages/LiveStreamPage";
import { Sidebar } from "./components/Sidebar";
import { MenuIcon } from "./components/Icons";
import { MOBILE_QUERY, useMediaQuery } from "./hooks/useMediaQuery";
import { useBodyScrollLock } from "./hooks/useFocusTrap";
import { useTheme } from "./hooks/useTheme";

const SIDEBAR_ID = "app-sidebar";

export function App() {
  const { t } = useTranslation();
  const location = useLocation();
  const isMobile = useMediaQuery(MOBILE_QUERY);
  const [theme, setTheme] = useTheme();
  const [navOpen, setNavOpen] = useState(false);
  const mainRef = useRef<HTMLElement>(null);
  const firstRender = useRef(true);
  const drawerOpen = isMobile && navOpen;

  useEffect(() => setNavOpen(false), [location.pathname, isMobile]);
  useBodyScrollLock(drawerOpen);

  // Hide the page behind the open navigation drawer from keyboard and AT.
  useEffect(() => {
    mainRef.current?.toggleAttribute("inert", drawerOpen);
  }, [drawerOpen]);

  // Move focus to the main region on client-side navigation so keyboard and
  // screen reader users start at the new page's content.
  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false;
      return;
    }
    mainRef.current?.focus();
  }, [location.pathname]);

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        {t("a11y.skipToContent")}
      </a>

      <header className="topbar">
        <button
          type="button"
          className="icon-button"
          data-drawer-toggle
          aria-expanded={navOpen}
          aria-controls={SIDEBAR_ID}
          aria-label={navOpen ? t("nav.closeMenu") : t("nav.openMenu")}
          onClick={() => setNavOpen((open) => !open)}
        >
          <MenuIcon />
        </button>
        <span className="topbar-brand">{t("app.name")}</span>
      </header>

      <Sidebar
        id={SIDEBAR_ID}
        isDrawer={isMobile}
        open={navOpen}
        onClose={() => setNavOpen(false)}
        theme={theme}
        onThemeChange={setTheme}
      />
      {drawerOpen && <div className="scrim scrim-nav" aria-hidden="true" />}

      <main id="main-content" ref={mainRef} className="app-content" tabIndex={-1}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route
            path="/"
            element={
              <RequireAuth>
                <StatusDashboard />
              </RequireAuth>
            }
          />
          <Route
            path="/events"
            element={
              <RequireAuth>
                <EventExplorerPage />
              </RequireAuth>
            }
          />
          <Route
            path="/events/:eventId"
            element={
              <RequireAuth>
                <EventDetailPage />
              </RequireAuth>
            }
          />
          <Route
            path="/subscriptions"
            element={
              <RequireAuth>
                <SubscriptionsPage />
              </RequireAuth>
            }
          />
          <Route
            path="/webhooks"
            element={
              <RequireAuth>
                <WebhooksPage />
              </RequireAuth>
            }
          />
          <Route
            path="/live"
            element={
              <RequireAuth>
                <LiveStreamPage />
              </RequireAuth>
            }
          />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  );
}
