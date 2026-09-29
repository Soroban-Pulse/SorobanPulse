import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "../locales/en.json";
import es from "../locales/es.json";

// To add a language: create src/locales/<code>.json, import it here, add it to
// `resources` and SUPPORTED_LOCALES. See docs/web-dashboard.md.
export const SUPPORTED_LOCALES = [
  { code: "en", nativeName: "English" },
  { code: "es", nativeName: "Español" },
] as const;

export type LocaleCode = (typeof SUPPORTED_LOCALES)[number]["code"];

const STORAGE_KEY = "sorobanpulse.dashboard.locale";
const FALLBACK: LocaleCode = "en";

function isSupported(code: string | null | undefined): code is LocaleCode {
  return SUPPORTED_LOCALES.some((l) => l.code === code);
}

/** Stored choice first, then the first supported browser language, then English. */
function detectLocale(): LocaleCode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isSupported(stored)) return stored;
  } catch {
    // storage unavailable (private mode); fall through to the browser language
  }
  for (const tag of navigator.languages ?? [navigator.language]) {
    const base = tag.toLowerCase().split("-")[0];
    if (isSupported(base)) return base;
  }
  return FALLBACK;
}

function applyDocumentLocale(code: string) {
  document.documentElement.lang = code;
  document.title = i18n.t("app.title");
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    es: { translation: es },
  },
  lng: detectLocale(),
  fallbackLng: FALLBACK,
  supportedLngs: SUPPORTED_LOCALES.map((l) => l.code),
  interpolation: { escapeValue: false }, // React already escapes
  returnNull: false,
});

applyDocumentLocale(i18n.language);
i18n.on("languageChanged", applyDocumentLocale);

export function setLocale(code: LocaleCode) {
  try {
    localStorage.setItem(STORAGE_KEY, code);
  } catch {
    // non-persistent choice is fine
  }
  return i18n.changeLanguage(code);
}

export default i18n;
