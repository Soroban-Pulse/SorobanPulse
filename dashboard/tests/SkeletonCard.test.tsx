import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SkeletonCard } from "../src/components/SkeletonCard";

describe("SkeletonCard", () => {
  it("renders with default dimensions", () => {
    render(<SkeletonCard />);
    const el = screen.getByRole("status");
    expect(el).toBeTruthy();
    expect(el).toHaveClass("skeleton-card");
  });

  it("renders with custom width and height", () => {
    render(<SkeletonCard width="200px" height="100px" />);
    const el = screen.getByRole("status");
    expect(el).toHaveStyle({ width: "200px", height: "100px" });
  });
});
