import { lazy, Suspense } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "./components/layout/Layout.tsx";

// Lazy-load pages — keeps the initial bundle small
const OverviewPage = lazy(() => import("./pages/OverviewPage.tsx"));
const EventsPage = lazy(() => import("./pages/EventsPage.tsx"));
const ContractsPage = lazy(() => import("./pages/ContractsPage.tsx"));
const SubscriptionsPage = lazy(() => import("./pages/SubscriptionsPage.tsx"));
const StatusPage = lazy(() => import("./pages/StatusPage.tsx"));
const NotFoundPage = lazy(() => import("./pages/NotFoundPage.tsx"));

function PageLoader() {
  return (
    <p className="text-muted" style={{ padding: "2rem" }}>
      Loading…
    </p>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <Suspense fallback={<PageLoader />}>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<OverviewPage />} />
            <Route path="events" element={<EventsPage />} />
            <Route path="contracts" element={<ContractsPage />} />
            <Route path="subscriptions" element={<SubscriptionsPage />} />
            <Route path="status" element={<StatusPage />} />
            <Route path="*" element={<NotFoundPage />} />
          </Route>
        </Routes>
      </Suspense>
    </BrowserRouter>
  );
}
