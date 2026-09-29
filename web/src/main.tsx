import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { SettingsProvider } from "./context/SettingsContext.tsx";
import App from "./App.tsx";
import "./index.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Don't retry on 4xx — those are client errors, not transient failures.
      retry: (failureCount, error) => {
        if (
          error instanceof Error &&
          "status" in error &&
          typeof (error as { status: unknown }).status === "number" &&
          (error as { status: number }).status < 500
        ) {
          return false;
        }
        return failureCount < 3;
      },
      // Keep data fresh in the background without a loading spinner on every focus.
      staleTime: 5_000,
    },
  },
});

const root = document.getElementById("root");
if (!root) throw new Error("Root element #root not found in index.html");

createRoot(root).render(
  <StrictMode>
    {/* SettingsProvider must wrap everything so the client and components
        can read credentials and isConfigured from any depth in the tree. */}
    <SettingsProvider>
      <QueryClientProvider client={queryClient}>
        <App />
        {/* Dev-only query inspector — tree-shaken in production builds */}
        <ReactQueryDevtools initialIsOpen={false} />
      </QueryClientProvider>
    </SettingsProvider>
  </StrictMode>,
);
