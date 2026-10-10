import { describe, expect, it } from "vitest";
import type { SessionRecord } from "../types";
import {
  JOIN_AGAIN_WINDOW_MS,
  canJoinAgain,
  csvCell,
  formatDuration,
  historyCsv,
  maskNamesInText,
  playtimeByGame,
  sessionDurationMs,
  sessionEndLabel,
} from "./sessionHistory";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, key) => String(options?.[key] ?? ""));
const NOW = new Date(2026, 9, 10, 12, 0).getTime();
const MIN = 60_000;
const DAY = 86_400_000;

function session(overrides: Partial<SessionRecord> = {}): SessionRecord {
  return {
    startedAt: NOW - 60 * MIN,
    endedAt: NOW - 30 * MIN,
    placeId: 10,
    jobId: "job-a",
    end: "left",
    dropKind: null,
    reason: null,
    code: null,
    message: null,
    ...overrides,
  };
}

describe("histórico — tempo de jogo", () => {
  it("adds up each game's time in the last 14 days, biggest first", () => {
    const totals = playtimeByGame(
      [
        session({ placeId: 10, startedAt: NOW - 60 * MIN, endedAt: NOW - 30 * MIN }),
        session({ placeId: 20, startedAt: NOW - 120 * MIN, endedAt: NOW - 60 * MIN }),
        session({ placeId: 10, startedAt: NOW - 10 * MIN, endedAt: null, end: "ongoing" }),
      ],
      NOW
    );
    expect(totals).toEqual([
      { placeId: 20, ms: 60 * MIN },
      { placeId: 10, ms: 40 * MIN },
    ]);
  });

  it("only the part inside the window counts, and unknown ends count nothing", () => {
    const totals = playtimeByGame(
      [
        session({ startedAt: NOW - 14 * DAY - 30 * MIN, endedAt: NOW - 14 * DAY + 30 * MIN }),
        session({ placeId: 30, endedAt: null, end: "unknown" }),
        session({ placeId: null, end: "moderated" }),
      ],
      NOW
    );
    expect(totals).toEqual([{ placeId: 10, ms: 30 * MIN }]);
  });

  it("formats durations short", () => {
    expect(formatDuration(45_000)).toBe("45s");
    expect(formatDuration(12 * MIN)).toBe("12m");
    expect(formatDuration(65 * MIN)).toBe("1h 05m");
    expect(sessionDurationMs(session({ endedAt: null, end: "ongoing" }), NOW)).toBe(60 * MIN);
    expect(sessionDurationMs(session({ endedAt: null, end: "unknown" }), NOW)).toBeNull();
  });
});

describe("histórico — Join again", () => {
  it("only for a known server that ended a short while ago, and not when it shut down", () => {
    expect(canJoinAgain(session(), NOW)).toBe(true);
    expect(canJoinAgain(session({ jobId: null }), NOW)).toBe(false);
    expect(canJoinAgain(session({ end: "ongoing", endedAt: null }), NOW)).toBe(false);
    expect(canJoinAgain(session({ endedAt: NOW - JOIN_AGAIN_WINDOW_MS - 1 }), NOW)).toBe(false);
    expect(canJoinAgain(session({ end: "dropped", dropKind: "serverShutdown" }), NOW)).toBe(false);
    expect(canJoinAgain(session({ end: "dropped", dropKind: "disconnected", reason: "connectionLost" }), NOW)).toBe(true);
  });
});

describe("histórico — como terminou", () => {
  it("a drop uses the same words as the Session panel", () => {
    const label = sessionEndLabel(
      session({ end: "dropped", dropKind: "disconnected", reason: "connectionLost", code: 277 }),
      t
    );
    expect(label).toEqual({ label: "Disconnected: lost connection", detail: "Roblox error code 277", tone: "drop" });
    expect(sessionEndLabel(session({ end: "ongoing" }), t).tone).toBe("playing");
  });

  it("with names hidden the account name never shows inside a kick message", () => {
    const masking = { hideUsernames: true, hiddenNameLetters: 0 };
    expect(maskNamesInText("Bye bobalt!", { Username: "BobAlt", Alias: "" }, masking)).toBe("Bye ************!");
    expect(maskNamesInText("Bye bobalt!", { Username: "BobAlt", Alias: "" }, { ...masking, hideUsernames: false })).toBe(
      "Bye bobalt!"
    );
  });
});

describe("histórico — CSV", () => {
  it("cells that a spreadsheet would run as a formula get a leading quote", () => {
    for (const evil of ["=HYPERLINK(\"x\")", "+1", "-2+3", "@SUM(A1)", "\tcmd"]) {
      expect(csvCell(evil).replace(/^"/, "").startsWith("'")).toBe(true);
    }
    expect(csvCell("plain")).toBe("plain");
    expect(csvCell('a "b", c')).toBe('"a ""b"", c"');
    expect(csvCell(42)).toBe("42");
  });

  it("one row per session, with the game name and how it ended", () => {
    const csv = historyCsv(
      [
        session({
          end: "dropped",
          dropKind: "kicked",
          code: 267,
          message: "=cmd|' /C calc'!A0",
        }),
      ],
      () => "Blox Fruits",
      t,
      NOW
    );
    const [header, row] = csv.trim().split("\r\n");
    expect(header).toBe("Start time,End time,Minutes,Game,Place ID,Server (Job ID),How it ended,Code,Message");
    expect(row).toContain("Blox Fruits");
    expect(row).toContain(",30,");
    expect(row).toContain("267");
    // A mensagem do jogo virou texto, não fórmula.
    expect(row).toContain(`"'=cmd|' /C calc'!A0"`.replace(/^"|"$/g, ""));
    expect(row).not.toMatch(/,=cmd/);
  });
});
