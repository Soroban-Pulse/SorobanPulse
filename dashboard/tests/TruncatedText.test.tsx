import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TruncatedText } from "../src/components/TruncatedText";

describe("TruncatedText", () => {
  it("renders short text as-is", () => {
    render(<TruncatedText text="short" />);
    expect(screen.getByText("short")).toBeTruthy();
  });

  it("truncates long text with middle ellipsis", () => {
    render(<TruncatedText text="abcdefghijklmnopqrstuvwxyz1234567890" maxChars={16} />);
    const el = screen.getByText(/…/);
    expect(el).toBeTruthy();
  });

  it("shows full text in a tooltip", () => {
    render(<TruncatedText text="abcdefghijklmnopqrstuvwxyz1234567890" maxChars={16} />);
    const el = screen.getByTitle("abcdefghijklmnopqrstuvwxyz1234567890");
    expect(el).toBeTruthy();
  });

  it("renders a copy button", () => {
    render(<TruncatedText text="abcdefghijklmnopqrstuvwxyz1234567890" maxChars={16} />);
    expect(screen.getByLabelText(/copy/i)).toBeTruthy();
  });
});
