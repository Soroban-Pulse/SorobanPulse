import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ErrorRateLimited } from "../src/components/ErrorRateLimited";

describe("ErrorRateLimited", () => {
  it("renders the rate limited message without retryAfter", () => {
    render(<ErrorRateLimited />);
    expect(screen.getByText("Rate limited")).toBeTruthy();
    expect(screen.getByText(/making requests too quickly/)).toBeTruthy();
  });

  it("renders retryAfter seconds when provided", () => {
    render(<ErrorRateLimited retryAfter={30} />);
    expect(screen.getByText(/wait 30 seconds/)).toBeTruthy();
  });

  it("renders singular 'second' for retryAfter=1", () => {
    render(<ErrorRateLimited retryAfter={1} />);
    expect(screen.getByText(/wait 1 second/)).toBeTruthy();
  });

  it("renders a retry button when onRetry is provided", () => {
    const handleRetry = vi.fn();
    render(<ErrorRateLimited retryAfter={5} onRetry={handleRetry} />);
    const button = screen.getByRole("button", { name: "Retry now" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleRetry).toHaveBeenCalledOnce();
  });
});
