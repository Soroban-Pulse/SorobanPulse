import { useId } from "react";
import { useTranslation } from "react-i18next";
import { LocaleCode, SUPPORTED_LOCALES, setLocale } from "../i18n";

export function LanguageSwitcher() {
  const { t, i18n } = useTranslation();
  const id = useId();

  return (
    <div className="field">
      <label htmlFor={id}>{t("settings.language")}</label>
      <select
        id={id}
        value={i18n.resolvedLanguage}
        onChange={(e) => setLocale(e.target.value as LocaleCode)}
      >
        {SUPPORTED_LOCALES.map((locale) => (
          <option key={locale.code} value={locale.code} lang={locale.code}>
            {locale.nativeName}
          </option>
        ))}
      </select>
    </div>
  );
}
