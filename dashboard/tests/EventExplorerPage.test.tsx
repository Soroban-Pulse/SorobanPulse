import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { EventExplorerPage } from "../src/pages/EventExplorerPage";

vi.mock("../src/api/client", () => ({
  dashboardApi: {
    listEvents: vi.fn(),
  },
}));

import { dashboardApi } from "../src/api/client";
import type { Event, EventListResponse } from "../src/api/eventTypes";

const mockEvents: Event[] = [
  {
    id: "550d4e3a-f3a2-c1b8-d4e3-a2c1b8d4e3a2",
    contractId: "CAABCDEF1234567890abcdef1234567890abcdef12",
    eventType: "contract",
    txHash: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    ledger: 42193400,
    timestamp: "2026-01-15T10:30:00Z",
    eventData: { topic: ["transfer"], amount: 1000 },
    inSuccessfulCall: true,
    createdAt: "2026-01-15T10:30:00Z",
    schemaVersion: 20,
    anonymized: false,
    tenantId: "default",
    network: "soroban-mainnet",
  },
];

function renderWithRouter() {
  return render(
    <MemoryRouter initialEntries={["/events"]}>
      <Routes>
        <Route path="/events" element={<EventExplorerPage />} />
      </Routes>
    </MemoryRouter>
  );
}

describe("EventExplorerPage", () => {
  it("renders the page heading", () => {
    (dashboardApi.listEvents as vi.Mock).mockResolvedValue({
      events: [],
      page: 1,
      limit: 20,
      total: 0,
      hasMore: false,
    } as EventListResponse);
    renderWithRouter();
    expect(screen.getByText("Event Explorer")).toBeTruthy();
  });

  it("renders the filter bar", () => {
    (dashboardApi.listEvents as vi.Mock).mockResolvedValue({
      events: [],
      page: 1,
      limit: 20,
      total: 0,
      hasMore: false,
    } as EventListResponse);
    renderWithRouter();
    expect(screen.getByLabelText(/search/i)).toBeTruthy();
  });

  it("renders empty state when no events", async () => {
    (dashboardApi.listEvents as vi.Mock).mockResolvedValue({
      events: [],
      page: 1,
      limit: 20,
      total: 0,
      hasMore: false,
    } as EventListResponse);
    renderWithRouter();
    expect(await screen.findByText("No events match the current filters.")).toBeTruthy();
  });

  it("renders events in a table when data is present", async () => {
    (dashboardApi.listEvents as vi.Mock).mockResolvedValue({
      events: mockEvents,
      page: 1,
      limit: 20,
      total: 1,
      hasMore: false,
    } as EventListResponse);
    renderWithRouter();
    expect(await screen.findByText("42,193,400")).toBeTruthy();
  });
});
