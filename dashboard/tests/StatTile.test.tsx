import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatTile } from "../src/components/StatTile";

describe("StatTile", () => {
  it("renders the label and value", () => {
    render(<StatTile label="Status" value="healthy" />);
    expect(screen.getByText("Status")).toBeTruthy();
    expect(screen.getByText("healthy")).toBeTruthy();
  });
});
