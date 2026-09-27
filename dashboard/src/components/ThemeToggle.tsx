import { useId } from "react";
import { useTranslation } from "react-i18next";
import { ThemePreference } from "../hooks/useTheme";

interface ThemeToggleProps {
  theme: ThemePreference;
  onChange: (theme: ThemePreference) => void;
}

export function ThemeToggle({ theme, onChange }: ThemeToggleProps) {
  const { t } = useTranslation();
  const id = useId();

  return (
    <div className="field">
      <label htmlFor={id}>{t("settings.theme")}</label>
      <select id={id} value={theme} onChange={(e) => onChange(e.target.value as ThemePreference)}>
        <option value="system">{t("settings.themeSystem")}</option>
        <option value="light">{t("settings.themeLight")}</option>
        <option value="dark">{t("settings.themeDark")}</option>
      </select>
    </div>
  );
}
