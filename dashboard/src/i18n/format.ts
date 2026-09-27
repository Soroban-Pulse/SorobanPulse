import { useMemo } from "react";
import { useTranslation } from "react-i18next";

const RELATIVE_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 60 * 60 * 24 * 365],
  ["month", 60 * 60 * 24 * 30],
  ["day", 60 * 60 * 24],
  ["hour", 60 * 60],
  ["minute", 60],
  ["second", 1],
];

/**
 * Locale- and timezone-aware formatters built on Intl. Always format numbers,
 * dates and durations through this hook rather than toLocaleString() or string
 * concatenation so the output follows the selected language.
 */
export function useFormat() {
  const { i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? i18n.language;

  return useMemo(() => {
    const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const number = new Intl.NumberFormat(locale);
    const dateTime = new Intl.DateTimeFormat(locale, {
      dateStyle: "medium",
      timeStyle: "medium",
      timeZone,
    });
    const time = new Intl.DateTimeFormat(locale, { timeStyle: "short", timeZone });
    const hours = new Intl.NumberFormat(locale, {
      style: "unit",
      unit: "hour",
      unitDisplay: "short",
      maximumFractionDigits: 0,
    });
    const relative = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });

    return {
      locale,
      timeZone,
      number: (value: number) => number.format(value),
      dateTime: (value: string | number | Date) => dateTime.format(new Date(value)),
      time: (value: string | number | Date) => time.format(new Date(value)),
      hours: (seconds: number) => hours.format(Math.floor(seconds / 3600)),
      relative: (value: string | number | Date, now = Date.now()) => {
        const diffSeconds = (new Date(value).getTime() - now) / 1000;
        const [unit, size] =
          RELATIVE_UNITS.find(([, s]) => Math.abs(diffSeconds) >= s) ?? RELATIVE_UNITS[5];
        return relative.format(Math.round(diffSeconds / size), unit);
      },
    };
  }, [locale]);
}
