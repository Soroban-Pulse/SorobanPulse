import { setThemePreference, useThemePreference, type ThemePreference } from "../lib/theme";

const ORDER: ThemePreference[] = ["system", "light", "dark"];
const LABEL: Record<ThemePreference, string> = { system: "System theme", light: "Light theme", dark: "Dark theme" };
const ICON: Record<ThemePreference, string> = { system: "◐", light: "☀", dark: "☾" };

/** Cycles System → Light → Dark. The choice persists in localStorage. */
export function ThemeToggle() {
  const pref = useThemePreference();
  const next = ORDER[(ORDER.indexOf(pref) + 1) % ORDER.length];
  return (
    <button
      type="button"
      className="ghost"
      onClick={() => setThemePreference(next)}
      title={`${LABEL[pref]} — click for ${LABEL[next].toLowerCase()}`}
      aria-label={`${LABEL[pref]}. Switch to ${LABEL[next].toLowerCase()}`}
    >
      <span aria-hidden>{ICON[pref]}</span>
      <span className="muted" style={{ fontSize: 12 }}>
        {pref === "system" ? "Auto" : pref === "light" ? "Light" : "Dark"}
      </span>
    </button>
  );
}
