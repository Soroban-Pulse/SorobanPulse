import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { SkeletonTable } from "../src/components/SkeletonTable";

describe("SkeletonTable", () => {
  it("renders with default rows and columns", () => {
    render(<SkeletonTable />);
    expect(screen.getByRole("status")).toBeTruthy();
  });

  it("renders the correct number of rows and columns", () => {
    render(<SkeletonTable rows={3} columns={2} />);
    const rows = screen.querySelectorAll(".skeleton-table-row, .skeleton-table-header");
    expect(rows.length).toBe(4); // 1 header + 3 body rows
  });
});
