import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ScValViewer } from "../src/components/ScValViewer";

const sampleData = {
  topic: ["transfer"],
  amount: 1000,
  from: "CAABCDEF1234567890abcdef1234567890abcdef12",
  to: "CBDEF1234567890abcdef1234567890abcdef1234",
  asset: "XLM",
};

describe("ScValViewer", () => {
  it("renders tree view by default", () => {
    render(<ScValViewer data={sampleData} />);
    expect(screen.getByText("topic:")).toBeTruthy();
    expect(screen.getByText("amount:")).toBeTruthy();
  });

  it("switches to table view", async () => {
    render(<ScValViewer data={sampleData} />);
    await userEvent.click(screen.getByText("Table"));
    expect(screen.getByText("topic")).toBeTruthy();
    expect(screen.getByText("amount")).toBeTruthy();
  });

  it("switches to code view", async () => {
    render(<ScValViewer data={sampleData} />);
    await userEvent.click(screen.getByText("Code"));
    expect(screen.getByText(/"topic"/)).toBeTruthy();
  });

  it("renders nested objects in tree view", () => {
    const nestedData = {
      outer: {
        inner: "value",
      },
    };
    render(<ScValViewer data={nestedData} />);
    expect(screen.getByText("outer:")).toBeTruthy();
    expect(screen.getByText("inner:")).toBeTruthy();
  });

  it("renders arrays in tree view", () => {
    const arrayData = { items: ["a", "b", "c"] };
    render(<ScValViewer data={arrayData} />);
    expect(screen.getByText("items:")).toBeTruthy();
  });
});
