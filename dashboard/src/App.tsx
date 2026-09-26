import { Navigate, Route, Routes } from "react-router-dom";
import { RequireAuth } from "./auth/RequireAuth";
import { LoginPage } from "./pages/LoginPage";
import { StatusDashboard } from "./pages/StatusDashboard";
import { SubscriptionsPage } from "./pages/SubscriptionsPage";
import { WebhooksPage } from "./pages/WebhooksPage";
import { EventExplorerPage } from "./pages/EventExplorerPage";
import { EventDetailPage } from "./pages/EventDetailPage";
import { NavBar } from "./components/NavBar";

export function App() {
  return (
    <div className="app-shell">
      <NavBar />
      <main className="app-content">
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
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  );
}
