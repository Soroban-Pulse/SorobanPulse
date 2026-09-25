import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar.tsx";
import { TopBar } from "./TopBar.tsx";

/**
 * Root shell layout: sidebar | topbar / content area.
 * All pages render inside <Outlet />.
 */
export function Layout() {
  return (
    <div className="shell">
      <Sidebar />
      <TopBar />
      <main className="content" id="main-content">
        <Outlet />
      </main>
    </div>
  );
}
