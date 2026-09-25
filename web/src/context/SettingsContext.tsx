/**
 * SettingsContext
 *
 * Stores the three user-supplied credentials:
 *   - serverUrl  – base URL of the SorobanPulse API (e.g. "https://api.example.com")
 *   - apiKey     – regular read key  (sent as X-Api-Key)
 *   - adminKey   – optional admin key (sent as X-Admin-Key)
 *
 * Storage strategy
 *   - sessionStorage by default so keys are gone when the tab closes.
 *   - When the user opts in to "remember on this device", values are
 *     mirrored in localStorage.
 *   - Keys are NEVER placed in the URL.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

// ────────────────────────────────────────────────────────────────────────────
// Storage keys
// ────────────────────────────────────────────────────────────────────────────

const SK = {
  SERVER_URL: "sp_server_url",
  API_KEY: "sp_api_key",
  ADMIN_KEY: "sp_admin_key",
  REMEMBER: "sp_remember", // stored only in localStorage (flag itself is benign)
} as const;

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

function readStorage(key: string): string {
  // Prefer sessionStorage (current session); fall back to localStorage (remembered).
  return (
    sessionStorage.getItem(key) ?? localStorage.getItem(key) ?? ""
  );
}

function writeStorage(key: string, value: string, persist: boolean): void {
  sessionStorage.setItem(key, value);
  if (persist) {
    localStorage.setItem(key, value);
  } else {
    localStorage.removeItem(key);
  }
}

function clearStorage(key: string): void {
  sessionStorage.removeItem(key);
  localStorage.removeItem(key);
}

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

export interface Settings {
  serverUrl: string;
  apiKey: string;
  adminKey: string;
  /** Whether to persist credentials in localStorage */
  remember: boolean;
}

export interface SettingsContextValue extends Settings {
  isConfigured: boolean;
  hasAdminKey: boolean;
  save: (next: Partial<Settings>) => void;
  clear: () => void;
}

// ────────────────────────────────────────────────────────────────────────────
// Context
// ────────────────────────────────────────────────────────────────────────────

const SettingsContext = createContext<SettingsContextValue | null>(null);

// ────────────────────────────────────────────────────────────────────────────
// Provider
// ────────────────────────────────────────────────────────────────────────────

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [serverUrl, setServerUrl] = useState(() => readStorage(SK.SERVER_URL));
  const [apiKey, setApiKey] = useState(() => readStorage(SK.API_KEY));
  const [adminKey, setAdminKey] = useState(() => readStorage(SK.ADMIN_KEY));
  const [remember, setRemember] = useState(
    () => localStorage.getItem(SK.REMEMBER) === "true",
  );

  // Sync state → storage whenever any value changes.
  useEffect(() => {
    writeStorage(SK.SERVER_URL, serverUrl, remember);
  }, [serverUrl, remember]);

  useEffect(() => {
    writeStorage(SK.API_KEY, apiKey, remember);
  }, [apiKey, remember]);

  useEffect(() => {
    writeStorage(SK.ADMIN_KEY, adminKey, remember);
  }, [adminKey, remember]);

  useEffect(() => {
    if (remember) {
      localStorage.setItem(SK.REMEMBER, "true");
    } else {
      localStorage.removeItem(SK.REMEMBER);
    }
  }, [remember]);

  const save = useCallback((next: Partial<Settings>) => {
    if (next.serverUrl !== undefined) setServerUrl(next.serverUrl.trim());
    if (next.apiKey !== undefined) setApiKey(next.apiKey.trim());
    if (next.adminKey !== undefined) setAdminKey(next.adminKey.trim());
    if (next.remember !== undefined) setRemember(next.remember);
  }, []);

  const clear = useCallback(() => {
    clearStorage(SK.SERVER_URL);
    clearStorage(SK.API_KEY);
    clearStorage(SK.ADMIN_KEY);
    clearStorage(SK.REMEMBER);
    setServerUrl("");
    setApiKey("");
    setAdminKey("");
    setRemember(false);
  }, []);

  const value = useMemo<SettingsContextValue>(
    () => ({
      serverUrl,
      apiKey,
      adminKey,
      remember,
      isConfigured: Boolean(serverUrl && apiKey),
      hasAdminKey: Boolean(adminKey),
      save,
      clear,
    }),
    [serverUrl, apiKey, adminKey, remember, save, clear],
  );

  return (
    <SettingsContext.Provider value={value}>
      {children}
    </SettingsContext.Provider>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Hook
// ────────────────────────────────────────────────────────────────────────────

export function useSettings(): SettingsContextValue {
  const ctx = useContext(SettingsContext);
  if (!ctx) {
    throw new Error("useSettings must be used inside <SettingsProvider>");
  }
  return ctx;
}
