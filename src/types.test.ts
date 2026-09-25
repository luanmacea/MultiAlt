import { afterEach, describe, expect, it, vi } from "vitest";
import { getFreshnessColor, maskAccountName, parseGroupName, timeAgo } from "./types";

const NOW = new Date("2026-01-15T12:00:00.000Z").getTime();

function ago(ms: number) {
  return new Date(NOW - ms).toISOString();
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

afterEach(() => {
  vi.useRealTimers();
});

describe("parseGroupName", () => {
  it("extracts a numeric sort prefix and strips it from the label", () => {
    expect(parseGroupName("10 Bots")).toEqual({ sortKey: 10, displayName: "Bots" });
    expect(parseGroupName("001 Mains")).toEqual({ sortKey: 1, displayName: "Mains" });
    expect(parseGroupName("7Alts")).toEqual({ sortKey: 7, displayName: "Alts" });
  });

  it("consumes the whitespace that follows the prefix", () => {
    expect(parseGroupName("3   Spaced")).toEqual({ sortKey: 3, displayName: "Spaced" });
  });

  it("falls back to a huge sort key for unprefixed groups", () => {
    expect(parseGroupName("Default")).toEqual({ sortKey: 999999, displayName: "Default" });
    expect(parseGroupName("")).toEqual({ sortKey: 999999, displayName: "" });
  });

  // O prefixo antigo parava em 3 digitos, entao "2024 Alts" virava "4 Alts" na
  // tela: o usuario digitava um ano e via o nome partido ao meio.
  it("takes the whole run of leading digits, not just three", () => {
    expect(parseGroupName("2024 Alts")).toEqual({ sortKey: 2024, displayName: "Alts" });
    expect(parseGroupName("1234 Group")).toEqual({ sortKey: 1234, displayName: "Group" });
  });

  it("keeps the original text when the prefix is the whole name", () => {
    expect(parseGroupName("42")).toEqual({ sortKey: 42, displayName: "42" });
  });
});

describe("timeAgo", () => {
  it("returns 'never' for an empty date", () => {
    expect(timeAgo("")).toBe("never");
  });

  it("formats minutes, hours and days", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);

    expect(timeAgo(ago(30_000))).toBe("now");
    expect(timeAgo(ago(5 * MINUTE))).toBe("5m");
    expect(timeAgo(ago(3 * HOUR))).toBe("3h");
    expect(timeAgo(ago(2 * DAY))).toBe("2d");
  });

  it("switches to months and years past the thresholds", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);

    expect(timeAgo(ago(31 * DAY))).toBe("1mo");
    expect(timeAgo(ago(200 * DAY))).toBe("6mo");
    expect(timeAgo(ago(400 * DAY))).toBe("1y");
  });

  it("clamps future dates to 'just now'", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
    expect(timeAgo(new Date(NOW + HOUR).toISOString())).toBe("just now");
  });
});

describe("getFreshnessColor", () => {
  it("leaves an account that was never used unpainted", () => {
    // Sem LastUse nao ha idade conhecida: pintar de vermelho fazia a conta
    // recem-importada parecer uma sessao quebrada.
    expect(getFreshnessColor("")).toBeNull();
  });

  it("returns null while the account is fresh", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
    expect(getFreshnessColor(ago(0))).toBeNull();
    expect(getFreshnessColor(ago(19 * DAY))).toBeNull();
  });

  it("fades from amber to orange between 20 and 30 days", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);

    expect(getFreshnessColor(ago(20 * DAY))).toBe("rgb(255,204,77)");
    expect(getFreshnessColor(ago(25 * DAY))).toBe("rgb(252,160,50)");
    expect(getFreshnessColor(ago(30 * DAY))).toBe("rgb(249,115,22)");
    // past 30 days the ramp is clamped
    expect(getFreshnessColor(ago(100 * DAY))).toBe("rgb(249,115,22)");
  });

  it("never reaches the red reserved for an invalid session", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);

    for (const days of [20, 22, 25, 28, 30, 60, 365, 3650]) {
      const color = getFreshnessColor(ago(days * DAY));
      const channels = color?.match(/\d+/g)?.map(Number) ?? [];
      expect(channels).toHaveLength(3);
      // O vermelho de "sessao invalida" tem o verde perto de zero; o
      // envelhecimento tem que continuar legivelmente ambar/laranja.
      expect(channels[1]).toBeGreaterThanOrEqual(100);
    }
  });
});

describe("maskAccountName", () => {
  it("returns the name untouched while Hidden is off", () => {
    expect(maskAccountName("ann", false, 2)).toBe("ann");
    expect(maskAccountName("ann", false, 0)).toBe("ann");
  });

  it("keeps only the configured preview letters while Hidden is on", () => {
    expect(maskAccountName("annabelle", true, 3)).toBe("ann********");
  });

  it("hides the whole name when no preview letters are configured", () => {
    expect(maskAccountName("annabelle", true, 0)).toBe("************");
  });

  it("hides the whole name when the preview would show all of it", () => {
    expect(maskAccountName("ann", true, 3)).toBe("************");
    expect(maskAccountName("ann", true, 9)).toBe("************");
  });
});
