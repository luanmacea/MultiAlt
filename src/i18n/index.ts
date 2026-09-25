import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enCommon from "../locales/en/common.json";
import deCommon from "../locales/de/common.json";
import ptCommon from "../locales/pt/common.json";

export const SUPPORTED_LANGUAGES = ["en", "de", "pt"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

/**
 * Fonte unica das opcoes de idioma: Settings > Geral e o passo de idioma do tour
 * liam listas separadas, e `pt` entrou so numa delas — o tour mostrava o codigo
 * cru "pt" e travava o Avancar. Os rotulos sao chaves do catalogo, entao cada
 * idioma ve os nomes na sua propria lingua.
 */
export const LANGUAGE_OPTIONS: { value: SupportedLanguage; label: string }[] = [
  { value: "en", label: "English" },
  { value: "de", label: "German" },
  { value: "pt", label: "Portuguese (Brazil)" },
];
export const DEFAULT_LANGUAGE: SupportedLanguage = "en";

export function normalizeLanguage(input?: string | null): SupportedLanguage {
  if (!input) return DEFAULT_LANGUAGE;
  const candidate = input.toLowerCase().trim();
  if (candidate.startsWith("de")) return "de";
  if (candidate.startsWith("pt") || candidate.startsWith("portug")) return "pt";
  return "en";
}

void i18n
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: enCommon },
      de: { translation: deCommon },
      pt: { translation: ptCommon },
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
