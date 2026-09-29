import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Error5xx } from "../src/components/Error5xx";

describe("Error5xx", () => {
  it("renders the default 500 message without requestId", () => {
    render(<Error5xx requestId={null} />);
    expect(screen.getByText("Server error (500)")).toBeTruthy();
    expect(screen.getByText(/Something went wrong on our end/)).toBeTruthy();
    expect(screen.queryByText(/request ID/)).toBeNull();
  });

  it("renders the request ID when provided", () => {
    render(<Error5xx requestId="req-abc-123" />);
    expect(screen.getByText(/request ID req-abc-123/)).toBeTruthy();
  });

  it("renders the correct status code", () => {
    render(<Error5xx requestId={null} statusCode={503} />);
    expect(screen.getByText("Server error (503)")).toBeTruthy();
  });

  it("renders a retry button when onRetry is provided", () => {
    const handleRetry = vi.fn();
    render(<Error5xx requestId="req-123" onRetry={handleRetry} />);
    const button = screen.getByRole("button", { name: "Retry" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleRetry).toHaveBeenCalledOnce();
  });
});
