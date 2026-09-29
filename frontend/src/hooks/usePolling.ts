import { useCallback, useEffect, useRef, useState } from "react";

export interface PollState<T> {
  data: T | undefined;
  error: unknown;
  loading: boolean;
  lastUpdated: Date | null;
  refresh: () => Promise<void>;
}

/**
 * Runs `fn` immediately and then every `intervalMs` (pass 0 to disable
 * polling). Pauses while the tab is hidden and refreshes when it returns.
 */
export function usePolling<T>(fn: () => Promise<T>, intervalMs: number, deps: unknown[] = []): PollState<T> {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const seq = useRef(0);

  const refresh = useCallback(async () => {
    const id = ++seq.current;
    try {
      const result = await fnRef.current();
      if (id !== seq.current) return;
      setData(result);
      setError(null);
    } catch (err) {
      if (id !== seq.current) return;
      setError(err);
    } finally {
      if (id === seq.current) {
        setLoading(false);
        setLastUpdated(new Date());
      }
    }
  }, []);

  useEffect(() => {
    setLoading(true);
    refresh();
    if (!intervalMs) return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") refresh();
    }, intervalMs);
    const onVisible = () => document.visibilityState === "visible" && refresh();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [intervalMs, refresh, ...deps]);

  return { data, error, loading, lastUpdated, refresh };
}

/** Re-renders every `ms` so relative times ("updated 4s ago") stay fresh. */
export function useNow(ms = 1000): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}
