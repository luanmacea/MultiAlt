import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enCommon from "../locales/en/common.json";
import deCommon from "../locales/de/common.json";

export const SUPPORTED_LANGUAGES = ["en", "de"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];
export const DEFAULT_LANGUAGE: SupportedLanguage = "en";

export function normalizeLanguage(input?: string | null): SupportedLanguage {
  if (!input) return DEFAULT_LANGUAGE;
  const candidate = input.toLowerCase().trim();
  if (candidate.startsWith("de")) return "de";
  return "en";
}

void i18n
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: enCommon },
      de: { translation: deCommon },
    },
    lng: DEFAULT_LANGUAGE,
    fallbackLng: DEFAULT_LANGUAGE,
    supportedLngs: [...SUPPORTED_LANGUAGES],
    defaultNS: "translation",
    ns: ["translation"],
    interpolation: {
      escapeValue: false,
    },
    keySeparator: false,
    nsSeparator: false,
    returnNull: false,
    returnEmptyString: false,
    react: {
      useSuspense: false,
    },
  });

export default i18n;
