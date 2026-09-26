export interface RetryOptions {
  maxRetries: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  retryableStatusCodes?: number[];
}

const DEFAULT_RETRYABLE_STATUS_CODES = [408, 429, 500, 502, 503, 504];

function computeBackoffMs(attempt: number, baseDelayMs: number, maxDelayMs: number): number {
  const exponential = baseDelayMs * 2 ** attempt;
  const jitter = Math.random() * baseDelayMs;
  return Math.min(exponential + jitter, maxDelayMs);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Runs `fn` with exponential backoff + jitter retry, retrying only on
 * network failures or status codes in `retryableStatusCodes`.
 */
export async function withRetry<T>(
  fn: () => Promise<T>,
  isRetryableError: (err: unknown) => boolean,
  options: RetryOptions,
): Promise<T> {
  const baseDelayMs = options.baseDelayMs ?? 250;
  const maxDelayMs = options.maxDelayMs ?? 10_000;

  let lastError: unknown;
  for (let attempt = 0; attempt <= options.maxRetries; attempt++) {
    try {
      return await fn();
    } catch (err) {
      lastError = err;
      const retryable = isRetryableError(err);
      if (!retryable || attempt === options.maxRetries) {
        throw err;
      }
      await sleep(computeBackoffMs(attempt, baseDelayMs, maxDelayMs));
    }
  }
  throw lastError;
}

export function isRetryableStatusCode(
  statusCode: number,
  retryableStatusCodes: number[] = DEFAULT_RETRYABLE_STATUS_CODES,
): boolean {
  return retryableStatusCodes.includes(statusCode);
}
