import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { AuthProvider, useAuth } from "../src/auth/AuthContext";

function Probe() {
  const { user, login, logout } = useAuth();
  return (
    <div>
      <span data-testid="user">{user ? user.email : "anonymous"}</span>
      <button onClick={() => login("test@example.com", "pw")}>login</button>
      <button onClick={logout}>logout</button>
    </div>
  );
}

describe("AuthContext", () => {
  beforeEach(() => {
    localStorage.clear();
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ token: "test-token" }),
    }) as unknown as typeof fetch;
  });

  it("starts unauthenticated", () => {
    render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    expect(screen.getByTestId("user").textContent).toBe("anonymous");
  });

  it("logs in and persists the session", async () => {
    render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    fireEvent.click(screen.getByText("login"));
    await waitFor(() => expect(screen.getByTestId("user").textContent).toBe("test@example.com"));
    expect(localStorage.getItem("sorobanpulse.dashboard.auth")).toContain("test-token");
  });

  it("logs out and clears storage", async () => {
    render(
      <AuthProvider>
        <Probe />
      </AuthProvider>,
    );
    fireEvent.click(screen.getByText("login"));
    await waitFor(() => expect(screen.getByTestId("user").textContent).toBe("test@example.com"));
    fireEvent.click(screen.getByText("logout"));
    expect(screen.getByTestId("user").textContent).toBe("anonymous");
    expect(localStorage.getItem("sorobanpulse.dashboard.auth")).toBeNull();
  });
});
