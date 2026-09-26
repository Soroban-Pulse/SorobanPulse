import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ErrorUnauthorized } from "../src/components/ErrorUnauthorized";

describe("ErrorUnauthorized", () => {
  it("renders the unauthorized message", () => {
    render(<ErrorUnauthorized />);
    expect(screen.getByText("Unauthorized")).toBeTruthy();
    expect(screen.getByText(/don't have permission/)).toBeTruthy();
  });

  it("renders a retry button when onRetry is provided", () => {
    const handleRetry = vi.fn();
    render(<ErrorUnauthorized onRetry={handleRetry} />);
    const button = screen.getByRole("button", { name: /sign out and sign in again/i });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleRetry).toHaveBeenCalledOnce();
  });
});
