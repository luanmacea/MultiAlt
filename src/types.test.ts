import { afterEach, describe, expect, it, vi } from "vitest";
import { getFreshnessColor, parseGroupName, timeAgo } from "./types";

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
  it("flags an account that was never used", () => {
    expect(getFreshnessColor("")).toBe("#fa1a0d");
  });

  it("returns null while the account is fresh", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
    expect(getFreshnessColor(ago(0))).toBeNull();
    expect(getFreshnessColor(ago(19 * DAY))).toBeNull();
  });

  it("fades from amber to red between 20 and 30 days", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);

    expect(getFreshnessColor(ago(20 * DAY))).toBe("rgb(255,204,77)");
    expect(getFreshnessColor(ago(25 * DAY))).toBe("rgb(253,115,45)");
    expect(getFreshnessColor(ago(30 * DAY))).toBe("rgb(250,26,13)");
    // past 30 days the ramp is clamped
    expect(getFreshnessColor(ago(100 * DAY))).toBe("rgb(250,26,13)");
  });
});
