import { describe, expect, it } from "vitest";
import type { ModerationStatus } from "../types";
import {
  accountCheckSummaryText,
  buildCheckReport,
  mergeCheckResults,
  type AccountCheckSummary,
} from "./accountCheck";

const t = (text: string, opts?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, k) => String(opts?.[k] ?? ""));

function summary(partial: Partial<AccountCheckSummary>): AccountCheckSummary {
  return { total: 0, ok: 0, warned: 0, invalid: 0, banned: 0, unknown: 0, results: [], ...partial };
}

describe("accountCheckSummaryText", () => {
  it("reads like the spec: ok, invalid, banned, couldn't check", () => {
    expect(
      accountCheckSummaryText(summary({ total: 23, ok: 18, invalid: 3, banned: 1, unknown: 1 }), t)
    ).toBe("18 ok, 3 invalid, 1 banned, 1 couldn't check");
  });

  it("mentions warnings only when there are some", () => {
    expect(accountCheckSummaryText(summary({ total: 3, ok: 2, warned: 1 }), t)).toBe(
      "2 ok, 0 invalid, 0 banned, 0 couldn't check, 1 warned"
    );
  });
});

describe("buildCheckReport", () => {
  const banned: ModerationStatus = { state: "banned", until: "2026-10-12T12:00:00Z", note: "Spam", punishment: "Ban 3 Days" };
  const warned: ModerationStatus = { state: "warned", until: null, note: null, punishment: "Warn" };

  it("puts banned first, then warned, invalid, unknown and ok, keeping input order inside a group", () => {
    const report = buildCheckReport(
      [
        { userId: 1, outcome: "ok" },
        { userId: 2, outcome: "unknown" },
        { userId: 3, outcome: "banned" },
        { userId: 4, outcome: "invalid" },
        { userId: 5, outcome: "warned" },
        { userId: 6, outcome: "banned" },
        { userId: 7, outcome: "ok" },
        { userId: 8, outcome: "invalid" },
      ],
      new Map()
    );
    expect(report.rows.map((r) => r.userId)).toEqual([3, 6, 5, 4, 8, 2, 1, 7]);
  });

  it("counts every outcome", () => {
    const report = buildCheckReport(
      [
        { userId: 1, outcome: "ok" },
        { userId: 2, outcome: "banned" },
        { userId: 3, outcome: "banned" },
        { userId: 4, outcome: "unknown" },
      ],
      new Map()
    );
    expect(report.counts).toEqual({ banned: 2, warned: 0, invalid: 0, unknown: 1, ok: 1 });
  });

  it("attaches the moderation detail only to banned and warned rows", () => {
    const report = buildCheckReport(
      [
        { userId: 1, outcome: "banned" },
        { userId: 2, outcome: "warned" },
        { userId: 3, outcome: "ok" },
        { userId: 4, outcome: "banned" },
      ],
      // A conta 3 tem um status velho no mapa: a linha dela não pode carregá-lo.
      new Map<number, ModerationStatus>([
        [1, banned],
        [2, warned],
        [3, banned],
      ])
    );
    const byId = new Map(report.rows.map((r) => [r.userId, r]));
    expect(byId.get(1)?.moderation).toBe(banned);
    expect(byId.get(2)?.moderation).toBe(warned);
    expect(byId.get(3)?.moderation).toBeUndefined();
    // Detalhe que ainda não chegou (o evento `account-moderation` vem separado).
    expect(byId.get(4)?.moderation).toBeUndefined();
  });

  it("merges a retry over the first check: retried accounts change, the others stay", () => {
    const merged = mergeCheckResults(
      [
        { userId: 1, outcome: "banned" },
        { userId: 2, outcome: "unknown" },
        { userId: 3, outcome: "unknown" },
      ],
      [
        { userId: 2, outcome: "ok" },
        { userId: 3, outcome: "unknown" },
      ]
    );
    expect(merged).toEqual([
      { userId: 1, outcome: "banned" },
      { userId: 2, outcome: "ok" },
      { userId: 3, outcome: "unknown" },
    ]);
  });

  it("is empty for an empty check", () => {
    expect(buildCheckReport([], new Map())).toEqual({
      rows: [],
      counts: { banned: 0, warned: 0, invalid: 0, unknown: 0, ok: 0 },
    });
  });
});
