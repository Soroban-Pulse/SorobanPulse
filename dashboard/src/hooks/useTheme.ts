import { useEffect, useState } from "react";

export type ThemePreference = "system" | "light" | "dark";

const STORAGE_KEY = "sorobanpulse.dashboard.theme";

function readPreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark") return stored;
  } catch {
    // storage unavailable
  }
  return "system";
}

/**
 * Stamps `data-theme` on <html> for an explicit choice; "system" removes it so
 * the prefers-color-scheme media query in styles.css decides.
 */
export function useTheme() {
  const [theme, setTheme] = useState<ThemePreference>(readPreference);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    try {
      if (theme === "system") localStorage.removeItem(STORAGE_KEY);
      else localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // non-persistent choice is fine
    }
  }, [theme]);

  return [theme, setTheme] as const;
}
