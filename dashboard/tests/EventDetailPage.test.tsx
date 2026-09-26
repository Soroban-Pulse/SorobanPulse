import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { EventDetailPage } from "../src/pages/EventDetailPage";

vi.mock("../src/api/client", () => ({
  dashboardApi: {
    getEventById: vi.fn(),
  },
}));

import { dashboardApi } from "../src/api/client";

const mockEvent = {
  id: "550d4e3a-f3a2-c1b8-d4e3-a2c1b8d4e3a2",
  contractId: "CAABCDEF1234567890abcdef1234567890abcdef12",
  eventType: "contract" as const,
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
};

function renderWithRouter(initialRoute = "/events/550d4e3a-f3a2-c1b8-d4e3-a2c1b8d4e3a2") {
  return render(
    <MemoryRouter initialEntries={[initialRoute]}>
      <Routes>
        <Route path="/events/:eventId" element={<EventDetailPage />} />
      </Routes>
    </MemoryRouter>
  );
}

describe("EventDetailPage", () => {
  it("renders loading state initially", () => {
    (dashboardApi.getEventById as vi.Mock).mockReturnValue(
      new Promise(() => {})
    );
    renderWithRouter();
    expect(screen.getByText("Loading event…")).toBeTruthy();
  });

  it("renders event details after loading", async () => {
    (dashboardApi.getEventById as vi.Mock).mockResolvedValue(mockEvent);
    renderWithRouter();

    expect(await screen.findByText("Event Detail")).toBeTruthy();
    expect(screen.getByText("42,193,400")).toBeTruthy();
    expect(screen.getByText("contract")).toBeTruthy();
  });

  it("renders not-found state for missing events", async () => {
    (dashboardApi.getEventById as vi.Mock).mockResolvedValue(null);
    renderWithRouter();

    expect(await screen.findByText("Event not found.")).toBeTruthy();
  });

  it("has a back button that navigates", async () => {
    (dashboardApi.getEventById as vi.Mock).mockResolvedValue(mockEvent);
    renderWithRouter();

    const backBtn = await screen.findByText(/← Back/);
    expect(backBtn).toBeTruthy();
  });
});
