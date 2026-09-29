import { lazy, Suspense } from "react";
import { BrowserRouter, Routes, Route, Navigate, useLocation } from "react-router-dom";
import { Layout } from "./components/layout/Layout.tsx";
import { useSettings } from "./context/SettingsContext.tsx";

// Lazy-load pages — keeps the initial bundle small
const OverviewPage      = lazy(() => import("./pages/OverviewPage.tsx"));
const EventsPage        = lazy(() => import("./pages/EventsPage.tsx"));
const LiveStreamPage    = lazy(() => import("./pages/LiveStreamPage.tsx"));
const ContractsPage     = lazy(() => import("./pages/ContractsPage.tsx"));
const SubscriptionsPage = lazy(() => import("./pages/SubscriptionsPage.tsx"));
const StatusPage        = lazy(() => import("./pages/StatusPage.tsx"));
const SettingsPage      = lazy(() => import("./pages/SettingsPage.tsx"));
const NotFoundPage      = lazy(() => import("./pages/NotFoundPage.tsx"));

function PageLoader() {
  return (
    <p className="text-muted" style={{ padding: "2rem" }}>
      Loading…
    </p>
  );
}

/**
 * Guards all non-settings routes.
 * When the dashboard has no server URL configured it redirects to /settings
 * so the user is forced through the first-run setup before anything else loads.
 * The `from` path is passed as `?next=` so settings can redirect back after saving.
 */
function RequireConfig({ children }: { children: React.ReactNode }) {
  const { isConfigured } = useSettings();
  const location = useLocation();

  if (!isConfigured) {
    return (
      <Navigate
        to={`/settings?next=${encodeURIComponent(location.pathname + location.search)}`}
        replace
      />
    );
  }

  return <>{children}</>;
}

export default function App() {
  return (
    <BrowserRouter>
      <Suspense fallback={<PageLoader />}>
        <Routes>
          <Route element={<Layout />}>
            {/* Settings is always accessible — it's how you get configured */}
            <Route path="settings" element={<SettingsPage />} />

            {/* All other routes require configuration */}
            <Route
              index
              element={
                <RequireConfig>
                  <OverviewPage />
                </RequireConfig>
              }
            />
            <Route
              path="events"
              element={
                <RequireConfig>
                  <EventsPage />
                </RequireConfig>
              }
            />
            <Route
              path="events/stream"
              element={
                <RequireConfig>
                  <LiveStreamPage />
                </RequireConfig>
              }
            />
            {/* Deep-link: /events/:id opens the drawer directly */}
            <Route
              path="events/:eventId"
              element={
                <RequireConfig>
                  <EventsPage />
                </RequireConfig>
              }
            />
            <Route
              path="contracts"
              element={
                <RequireConfig>
                  <ContractsPage />
                </RequireConfig>
              }
            />
            <Route
              path="subscriptions"
              element={
                <RequireConfig>
                  <SubscriptionsPage />
                </RequireConfig>
              }
            />
            <Route
              path="status"
              element={
                <RequireConfig>
                  <StatusPage />
                </RequireConfig>
              }
            />

            <Route path="*" element={<NotFoundPage />} />
          </Route>
        </Routes>
      </Suspense>
    </BrowserRouter>
  );
}
