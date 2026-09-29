import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EventFilterBar } from "../src/components/EventFilterBar";
import type { EventFilterParams } from "../src/api/eventTypes";

const defaultFilters: EventFilterParams = {};

describe("EventFilterBar", () => {
  it("renders all filter controls", () => {
    render(<EventFilterBar filters={defaultFilters} onChange={vi.fn()} onSearch={vi.fn()} />);
    expect(screen.getByLabelText(/search/i)).toBeTruthy();
    expect(screen.getByLabelText(/type/i)).toBeTruthy();
    expect(screen.getByLabelText(/contract/i)).toBeTruthy();
    expect(screen.getByLabelText(/ledger/i)).toBeTruthy();
    expect(screen.getByLabelText(/topic/i)).toBeTruthy();
    expect(screen.getByText("Apply")).toBeTruthy();
    expect(screen.getByText("Reset")).toBeTruthy();
  });

  it("calls onChange when search input changes", async () => {
    const onChange = vi.fn();
    render(<EventFilterBar filters={defaultFilters} onChange={onChange} onSearch={vi.fn()} />);
    const searchInput = screen.getByLabelText(/search/i);
    await userEvent.type(searchInput, "transfer");
    expect(onChange).toHaveBeenCalled();
  });

  it("calls onChange when type dropdown changes", async () => {
    const onChange = vi.fn();
    render(<EventFilterBar filters={defaultFilters} onChange={onChange} onSearch={vi.fn()} />);
    const typeSelect = screen.getByLabelText(/type/i);
    await userEvent.selectOptions(typeSelect, "contract");
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ eventType: "contract" })
    );
  });

  it("calls onSearch when Apply is clicked", async () => {
    const onSearch = vi.fn();
    render(<EventFilterBar filters={defaultFilters} onChange={vi.fn()} onSearch={onSearch} />);
    await userEvent.click(screen.getByText("Apply"));
    expect(onSearch).toHaveBeenCalledOnce();
  });

  it("calls onChange with empty filters when Reset is clicked", async () => {
    const onChange = vi.fn();
    const filtersWithValues: EventFilterParams = { search: "test", eventType: "contract" };
    render(<EventFilterBar filters={filtersWithValues} onChange={onChange} onSearch={vi.fn()} />);
    await userEvent.click(screen.getByText("Reset"));
    expect(onChange).toHaveBeenCalledWith({});
  });

  it("calls onSearch when Enter is pressed in a filter input", async () => {
    const onSearch = vi.fn();
    render(<EventFilterBar filters={defaultFilters} onChange={vi.fn()} onSearch={onSearch} />);
    const searchInput = screen.getByLabelText(/search/i);
    await userEvent.type(searchInput, "transfer{Enter}");
    expect(onSearch).toHaveBeenCalledOnce();
  });
});
