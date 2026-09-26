import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { EmptyState } from "../src/components/EmptyState";
import { IconEmpty } from "../src/components/Icons";

describe("EmptyState", () => {
  it("renders title and description", () => {
    render(<EmptyState title="Nothing here" description="No data to show." />);
    expect(screen.getByText("Nothing here")).toBeTruthy();
    expect(screen.getByText("No data to show.")).toBeTruthy();
  });

  it("renders a custom icon", () => {
    render(<EmptyState title="Test" description="Desc" icon={<IconEmpty />} />);
    expect(screen.getByText("Test")).toBeTruthy();
  });

  it("renders an action button when provided", () => {
    const handleClick = vi.fn();
    render(
      <EmptyState
        title="Test"
        description="Desc"
        actionLabel="Click me"
        onAction={handleClick}
      />,
    );
    const button = screen.getByRole("button", { name: "Click me" });
    expect(button).toBeTruthy();
    fireEvent.click(button);
    expect(handleClick).toHaveBeenCalledOnce();
  });

  it("does not render an action button when not provided", () => {
    render(<EmptyState title="Test" description="Desc" />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
