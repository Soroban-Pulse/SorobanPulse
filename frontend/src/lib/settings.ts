import { useSyncExternalStore } from "react";
import { readJson, writeJson } from "./storage";

export interface Settings {
  /** Base URL of the Soroban Pulse API. Empty means same origin (dev proxy). */
  apiBaseUrl: string;
  /** Sent as `Authorization: Bearer` on regular requests when set. */
  apiKey: string;
  /** Sent on admin requests; unlocks the admin console and SLO report. */
  adminKey: string;
}

const KEY = "sp.settings";
const defaults: Settings = {
  apiBaseUrl: import.meta.env.VITE_API_BASE_URL ?? "",
  apiKey: "",
  adminKey: "",
};

let current: Settings = { ...defaults, ...readJson<Partial<Settings>>(KEY, {}) };
const listeners = new Set<() => void>();

export function getSettings(): Settings {
  return current;
}

export function updateSettings(patch: Partial<Settings>): void {
  current = { ...current, ...patch };
  writeJson(KEY, current);
  listeners.forEach((l) => l());
}

export function useSettings(): Settings {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => current,
  );
}
