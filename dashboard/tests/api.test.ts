import { describe, expect, it, vi, beforeEach, fail } from "vitest";
import { ApiError, dashboardApi } from "../src/api/client";

describe("ApiError", () => {
  it("captures status, statusText, requestId, and retryAfter", () => {
    const err = new ApiError({
      status: 429,
      statusText: "Too Many Requests",
      requestId: "req-123",
      retryAfter: 30,
      message: "Rate limited",
    });
    expect(err.status).toBe(429);
    expect(err.statusText).toBe("Too Many Requests");
    expect(err.requestId).toBe("req-123");
    expect(err.retryAfter).toBe(30);
    expect(err.message).toBe("Rate limited");
    expect(err.name).toBe("ApiError");
  });

  it("handles null requestId and retryAfter", () => {
    const err = new ApiError({
      status: 500,
      statusText: "Internal Server Error",
      requestId: null,
      retryAfter: null,
      message: "Server error",
    });
    expect(err.requestId).toBeNull();
    expect(err.retryAfter).toBeNull();
  });
});

describe("dashboardApi error handling", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("throws ApiError on non-2xx responses", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      headers: new Headers({ "x-request-id": "req-456" }),
      text: async () => "Unauthorized",
    }) as unknown as typeof fetch;

    await expect(dashboardApi.getSystemStatus()).rejects.toThrow(ApiError);
  });

  it("captures Retry-After header on rate-limited responses", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 429,
      statusText: "Too Many Requests",
      headers: new Headers({
        "x-request-id": "req-789",
        "retry-after": "15",
      }),
      text: async () => "Rate limited",
    }) as unknown as typeof fetch;

    try {
      await dashboardApi.getSystemStatus();
      fail("Expected ApiError to be thrown");
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).retryAfter).toBe(15);
    }
  });
});
