import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SkeletonChart } from "../src/components/SkeletonChart";

describe("SkeletonChart", () => {
  it("renders with default height", () => {
    render(<SkeletonChart />);
    const el = screen.getByRole("status");
    expect(el).toBeTruthy();
    expect(el).toHaveClass("skeleton-chart");
  });

  it("renders with custom height", () => {
    render(<SkeletonChart height="400px" />);
    const el = screen.getByRole("status");
    expect(el).toHaveStyle({ height: "400px" });
  });

  it("renders 12 chart bars", () => {
    render(<SkeletonChart />);
    const bars = screen.querySelectorAll(".skeleton-chart-bar");
    expect(bars.length).toBe(12);
  });
});
