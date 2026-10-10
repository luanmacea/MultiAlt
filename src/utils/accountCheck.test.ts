import { describe, expect, it } from "vitest";
import { accountCheckSummaryText, type AccountCheckSummary } from "./accountCheck";

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
