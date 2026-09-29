import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { EmptySubscriptions } from "../src/components/EmptySubscriptions";

describe("EmptySubscriptions", () => {
  it("renders default message", () => {
    render(<EmptySubscriptions />);
    expect(screen.getByText("No subscriptions")).toBeTruthy();
    expect(screen.getByText(/You haven't created any subscriptions/)).toBeTruthy();
  });

  it("renders a create button when onCreateSubscription is provided", () => {
    const handleCreate = vi.fn();
    render(<EmptySubscriptions onCreateSubscription={handleCreate} />);
    const button = screen.getByRole("button", { name: "Create subscription" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleCreate).toHaveBeenCalledOnce();
  });
});
