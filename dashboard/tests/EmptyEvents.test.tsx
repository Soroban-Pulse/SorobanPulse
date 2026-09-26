import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { EmptyEvents } from "../src/components/EmptyEvents";

describe("EmptyEvents", () => {
  it("renders default message without lag", () => {
    render(<EmptyEvents />);
    expect(screen.getByText("No events yet")).toBeTruthy();
    expect(screen.getByText(/The indexer is still catching up/)).toBeTruthy();
  });

  it("renders lag message when lagSeconds is provided", () => {
    render(<EmptyEvents lagSeconds={12} />);
    expect(screen.getByText(/current lag is 12s/)).toBeTruthy();
  });
});
