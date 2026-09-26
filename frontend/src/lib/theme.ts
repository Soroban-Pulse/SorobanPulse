import { useEffect, useSyncExternalStore } from "react";

export type ThemePreference = "system" | "light" | "dark";
export type Theme = "light" | "dark";

// Key must match the inline script in index.html.
const KEY = "sp.theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");

function readPreference(): ThemePreference {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

let preference = readPreference();
const listeners = new Set<() => void>();

export function resolveTheme(pref: ThemePreference): Theme {
  return pref === "system" ? (media.matches ? "dark" : "light") : pref;
}

function apply() {
  document.documentElement.dataset.theme = resolveTheme(preference);
}

export function setThemePreference(pref: ThemePreference): void {
  preference = pref;
  try {
    if (pref === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, pref);
  } catch {
    /* ignore */
  }
  apply();
  listeners.forEach((l) => l());
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

export function useThemePreference(): ThemePreference {
  return useSyncExternalStore(subscribe, () => preference);
}

/** Keeps the document theme in sync with OS changes while on "system". */
export function useThemeSync(): void {
  useEffect(() => {
    apply();
    const onChange = () => {
      if (preference === "system") {
        apply();
        listeners.forEach((l) => l());
      }
    };
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);
}
