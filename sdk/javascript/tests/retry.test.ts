import { withRetry, isRetryableStatusCode } from "../src/retry";

describe("withRetry", () => {
  it("returns the result on first success without retrying", async () => {
    const fn = jest.fn().mockResolvedValue("ok");
    const result = await withRetry(fn, () => true, { maxRetries: 3, baseDelayMs: 1 });
    expect(result).toBe("ok");
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("retries retryable errors up to maxRetries then throws", async () => {
    const fn = jest.fn().mockRejectedValue(new Error("boom"));
    await expect(
      withRetry(fn, () => true, { maxRetries: 2, baseDelayMs: 1 }),
    ).rejects.toThrow("boom");
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it("does not retry non-retryable errors", async () => {
    const fn = jest.fn().mockRejectedValue(new Error("fatal"));
    await expect(
      withRetry(fn, () => false, { maxRetries: 5, baseDelayMs: 1 }),
    ).rejects.toThrow("fatal");
    expect(fn).toHaveBeenCalledTimes(1);
  });
});

describe("isRetryableStatusCode", () => {
  it("treats 429 and 5xx as retryable by default", () => {
    expect(isRetryableStatusCode(429)).toBe(true);
    expect(isRetryableStatusCode(503)).toBe(true);
  });

  it("treats 400 and 404 as non-retryable by default", () => {
    expect(isRetryableStatusCode(400)).toBe(false);
    expect(isRetryableStatusCode(404)).toBe(false);
  });
});
