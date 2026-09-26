import { createBrowserRouter, Navigate, RouterProvider } from "react-router-dom";
import { Layout } from "./components/Layout";
import { AccountPage } from "./pages/AccountPage";
import { AdminPage } from "./pages/AdminPage";
import { ContractPage } from "./pages/ContractPage";
import { Explorer } from "./pages/Explorer";
import { NotFound } from "./pages/NotFound";
import { StatusPage } from "./pages/StatusPage";
import { TxPage } from "./pages/TxPage";

const router = createBrowserRouter([
  {
    element: <Layout />,
    children: [
      { index: true, element: <Navigate to="/explorer" replace /> },
      { path: "explorer", element: <Explorer /> },
      { path: "contracts/:contractId", element: <ContractPage /> },
      { path: "accounts/:accountId", element: <AccountPage /> },
      { path: "tx/:txHash", element: <TxPage /> },
      { path: "status", element: <StatusPage /> },
      { path: "admin", element: <AdminPage /> },
      { path: "*", element: <NotFound /> },
    ],
  },
]);

export function App() {
  return <RouterProvider router={router} />;
import { NavLink, Navigate, Route, Routes } from 'react-router-dom';
import { ChannelsPage } from './features/channels/ChannelsPage';
import { ContractOverviewPage } from './features/contracts/ContractOverviewPage';
import { ContractsSearchPage } from './features/contracts/ContractsSearchPage';
import { DlqPage } from './features/deliveries/DlqPage';
import { SettingsPage } from './features/settings/SettingsPage';
import { SubscriptionDetailPage } from './features/subscriptions/SubscriptionDetailPage';
import { SubscriptionsPage } from './features/subscriptions/SubscriptionsPage';

const NAV = [
  { to: '/contracts', label: 'Contracts' },
  { to: '/subscriptions', label: 'Subscriptions' },
  { to: '/channels', label: 'Channels' },
  { to: '/admin/dlq', label: 'Dead letters' },
  { to: '/settings', label: 'Settings' },
];

export function App() {
  return (
    <div className="app">
      <nav className="sidebar" aria-label="Main">
        <div className="brand">Soroban Pulse</div>
        <ul>
          {NAV.map((n) => (
            <li key={n.to}>
              <NavLink to={n.to} className={({ isActive }) => (isActive ? 'active' : undefined)}>
                {n.label}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <main className="main">
        <Routes>
          <Route path="/" element={<Navigate to="/contracts" replace />} />
          <Route path="/contracts" element={<ContractsSearchPage />} />
          <Route path="/contracts/:id" element={<ContractOverviewPage />} />
          <Route path="/subscriptions" element={<SubscriptionsPage />} />
          <Route path="/subscriptions/:id" element={<SubscriptionDetailPage />} />
          <Route path="/channels" element={<ChannelsPage />} />
          <Route path="/admin/dlq" element={<DlqPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<p className="page">Page not found.</p>} />
        </Routes>
      </main>
    </div>
  );
}
