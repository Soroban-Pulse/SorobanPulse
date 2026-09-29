import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EventTable } from "../src/components/EventTable";
import type { Event } from "../src/api/eventTypes";

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

describe("EventTable", () => {
  it("renders a table with event rows", () => {
    render(<EventTable events={mockEvents} onRowClick={vi.fn()} />);
    expect(screen.getByText("42,193,400")).toBeTruthy();
  });

  it("calls onRowClick when a row is clicked", async () => {
    const onRowClick = vi.fn();
    render(<EventTable events={mockEvents} onRowClick={onRowClick} />);
    const row = screen.getByRole("row").first();
    await userEvent.click(row);
    expect(onRowClick).toHaveBeenCalledWith(mockEvents[0]);
  });

  it("calls onRowClick when Enter is pressed on a row", async () => {
    const onRowClick = vi.fn();
    render(<EventTable events={mockEvents} onRowClick={onRowClick} />);
    const row = screen.getByRole("row").first();
    await userEvent.keyboard("{Enter}");
    expect(onRowClick).toHaveBeenCalledWith(mockEvents[0]);
  });

  it("renders loading state", () => {
    render(<EventTable events={[]} onRowClick={vi.fn()} loading={true} />);
    expect(screen.getByText("Loading events…")).toBeTruthy();
  });

  it("renders event type badge", () => {
    render(<EventTable events={mockEvents} onRowClick={vi.fn()} />);
    expect(screen.getByText("contract")).toBeTruthy();
  });
});
