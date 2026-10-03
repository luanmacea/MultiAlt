import { afterEach, describe, expect, it } from "vitest";
import i18n, { DEFAULT_LANGUAGE, SUPPORTED_LANGUAGES, normalizeLanguage } from "./index";

afterEach(async () => {
  await i18n.changeLanguage(DEFAULT_LANGUAGE);
});

describe("normalizeLanguage", () => {
  it("defaults to English for empty input", () => {
    expect(normalizeLanguage()).toBe("en");
    expect(normalizeLanguage("")).toBe("en");
    expect(normalizeLanguage(null)).toBe("en");
    expect(normalizeLanguage(undefined)).toBe("en");
  });

  it("maps any de-* tag to German", () => {
    expect(normalizeLanguage("de")).toBe("de");
    expect(normalizeLanguage("DE-AT")).toBe("de");
    expect(normalizeLanguage("  deutsch ")).toBe("de");
  });

  it("maps any pt-* tag to Brazilian Portuguese", () => {
    expect(normalizeLanguage("pt")).toBe("pt");
    expect(normalizeLanguage("PT-BR")).toBe("pt");
    expect(normalizeLanguage("  pt-pt ")).toBe("pt");
    expect(normalizeLanguage("português")).toBe("pt");
  });

  it("maps any es-* tag to Spanish", () => {
    expect(normalizeLanguage("es")).toBe("es");
    expect(normalizeLanguage("ES-MX")).toBe("es");
    expect(normalizeLanguage("  es-419 ")).toBe("es");
    expect(normalizeLanguage("español")).toBe("es");
    expect(normalizeLanguage("Spanish")).toBe("es");
  });

  it("maps everything else to English", () => {
    expect(normalizeLanguage("en-US")).toBe("en");
    expect(normalizeLanguage("klingon")).toBe("en");
  });

  it("exposes the supported set", () => {
    expect([...SUPPORTED_LANGUAGES]).toEqual(["en", "de", "pt", "es"]);
    expect(DEFAULT_LANGUAGE).toBe("en");
  });
});

describe("i18n instance", () => {
  it("initializes on English with English as fallback", () => {
    expect(i18n.language).toBe("en");
    expect(i18n.options.fallbackLng).toEqual(["en"]);
  });

  it("treats the whole English phrase as the key (no key/ns separators)", () => {
    expect(i18n.options.keySeparator).toBe(false);
    expect(i18n.options.nsSeparator).toBe(false);
    expect(i18n.t("Settings")).toBe("Settings");
    // a phrase with dots and colons is not split into namespaces/sub-keys
    expect(i18n.t("Loading...")).toBe("Loading...");
  });

  it("translates known keys after switching to German and falls back for unknown ones", async () => {
    await i18n.changeLanguage("de");
    expect(i18n.t("Settings")).toBe("Einstellungen");
    expect(i18n.t("Cancel")).toBe("Abbrechen");
    expect(i18n.t("A phrase nobody translated", { defaultValue: "A phrase nobody translated" })).toBe(
      "A phrase nobody translated"
    );
  });

  it("translates known keys after switching to Portuguese", async () => {
    await i18n.changeLanguage("pt");
    expect(i18n.t("Settings")).toBe("Configurações");
    expect(i18n.t("Cancel")).toBe("Cancelar");
  });

  it("translates known keys after switching to Spanish", async () => {
    await i18n.changeLanguage("es");
    expect(i18n.t("Settings")).toBe("Configuración");
    expect(i18n.t("Cancel")).toBe("Cancelar");
    expect(i18n.t("Spanish")).toBe("Español");
  });

  it("reports whether a key exists", () => {
    expect(i18n.exists("Settings")).toBe(true);
    expect(i18n.exists("definitely not a translation key")).toBe(false);
  });

  it("interpolates without HTML escaping", () => {
    expect(i18n.t("Added {{name}}", { name: "Tom & Jerry" })).toBe("Added Tom & Jerry");
  });
});
