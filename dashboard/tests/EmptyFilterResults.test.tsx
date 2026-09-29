import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { EmptyFilterResults } from "../src/components/EmptyFilterResults";

describe("EmptyFilterResults", () => {
  it("renders default message", () => {
    render(<EmptyFilterResults />);
    expect(screen.getByText("No results")).toBeTruthy();
    expect(screen.getByText(/No events match the current filter/)).toBeTruthy();
  });

  it("renders a clear filter button when onClearFilter is provided", () => {
    const handleClear = vi.fn();
    render(<EmptyFilterResults onClearFilter={handleClear} />);
    const button = screen.getByRole("button", { name: "Clear filter" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleClear).toHaveBeenCalledOnce();
  });
});
