import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, vi } from "vitest";
import { Layout } from "../components/layout/Layout.tsx";

// Stub the status hook so the test doesn't need a real API
vi.mock("../api/hooks.ts", () => ({
  useStatus: () => ({ data: undefined, isLoading: true, isError: false }),
  useEvents: () => ({ data: undefined, isLoading: true, isError: false }),
  useContracts: () => ({ data: undefined, isLoading: true, isError: false }),
}));

function renderWithProviders(ui: React.ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Layout", () => {
  it("renders sidebar navigation", () => {
    renderWithProviders(<Layout />);
    expect(screen.getByRole("navigation", { name: /main navigation/i })).toBeInTheDocument();
  });

  it("renders the brand name", () => {
    renderWithProviders(<Layout />);
    expect(screen.getByText(/soroban pulse/i)).toBeInTheDocument();
  });

  it("shows Overview link", () => {
    renderWithProviders(<Layout />);
    expect(screen.getByRole("link", { name: /overview/i })).toBeInTheDocument();
  });

  it("shows Events link", () => {
    renderWithProviders(<Layout />);
    expect(screen.getByRole("link", { name: /events/i })).toBeInTheDocument();
  });

  it("renders header banner element", () => {
    renderWithProviders(<Layout />);
    expect(screen.getByRole("banner")).toBeInTheDocument();
  });
});
