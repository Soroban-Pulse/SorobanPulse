import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ErrorServerUnreachable } from "../src/components/ErrorServerUnreachable";

describe("ErrorServerUnreachable", () => {
  it("renders the server unreachable message", () => {
    render(<ErrorServerUnreachable />);
    expect(screen.getByText("Server unreachable")).toBeTruthy();
    expect(screen.getByText(/Unable to connect/)).toBeTruthy();
  });

  it("renders a retry button when onRetry is provided", () => {
    const handleRetry = vi.fn();
    render(<ErrorServerUnreachable onRetry={handleRetry} />);
    const button = screen.getByRole("button", { name: "Retry" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleRetry).toHaveBeenCalledOnce();
  });
});
