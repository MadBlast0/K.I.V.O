/**
 * Every UI string goes through i18next with ICU message format (UX §9, UX-49). English is the
 * source locale; other languages add a file in `locales/` and are listed in `LANGUAGES`. Dates and
 * numbers use `Intl` through ICU's `{n, number}` and `{d, date}` formats.
 */
import { createInstance } from "i18next";
import ICU from "i18next-icu";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";

/** Languages the UI ships in, with their text direction (UX-50: RTL works by switching `dir`). */
export const LANGUAGES = [{ code: "en", name: "English", dir: "ltr" }] as const;

const i18n = createInstance();
void i18n
  .use(ICU)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en } },
    lng: "en",
    fallbackLng: "en",
    interpolation: { escapeValue: false },
    returnNull: false,
  });

/** Applies a language to the document: `lang` and `dir` for screen readers and RTL layout. */
export function applyLanguage(code: string) {
  const language = LANGUAGES.find((l) => l.code === code) ?? LANGUAGES[0];
  void i18n.changeLanguage(language.code);
  document.documentElement.lang = language.code;
  document.documentElement.dir = language.dir;
}

export default i18n;
