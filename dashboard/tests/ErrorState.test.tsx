import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ErrorState } from "../src/components/ErrorState";
import { IconError } from "../src/components/Icons";

describe("ErrorState", () => {
  it("renders title and description", () => {
    render(<ErrorState title="Oops" description="Something went wrong." />);
    expect(screen.getByText("Oops")).toBeTruthy();
    expect(screen.getByText("Something went wrong.")).toBeTruthy();
  });

  it("renders a custom icon", () => {
    render(<ErrorState title="Test" description="Desc" icon={<IconError />} />);
    expect(screen.getByText("Test")).toBeTruthy();
  });

  it("renders a retry button when provided", () => {
    const handleRetry = vi.fn();
    render(<ErrorState title="Test" description="Desc" retryLabel="Retry" onRetry={handleRetry} />);
    const button = screen.getByRole("button", { name: "Retry" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleRetry).toHaveBeenCalledOnce();
  });

  it("does not render a retry button when not provided", () => {
    render(<ErrorState title="Test" description="Desc" />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
