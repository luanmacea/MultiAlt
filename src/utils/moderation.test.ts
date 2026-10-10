import { describe, expect, it } from "vitest";
import type { ModerationStatus } from "../types";
import { formatDayMonth, isActiveBan, moderationBadge, moderationLabel } from "./moderation";

const t = (text: string, opts?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, k) => String(opts?.[k] ?? ""));

const NOW = new Date(2026, 9, 9, 12, 0); // 09/10/2026 12:00, horário local

function status(partial: Partial<ModerationStatus>): ModerationStatus {
  return { state: "clean", until: null, note: null, punishment: null, ...partial };
}

describe("moderation labels", () => {
  it("formats the end of a ban as dd/mm", () => {
    expect(formatDayMonth(new Date(2026, 9, 12, 18, 5).toISOString())).toBe("12/10");
  });

  it("says until when the account is banned", () => {
    const until = new Date(2026, 9, 12, 18, 0).toISOString();
    expect(moderationLabel(status({ state: "banned", until }), t, NOW)).toBe("Banned until 12/10");
  });

  it("names a ban without an end date, a warning and a termination", () => {
    expect(moderationLabel(status({ state: "banned" }), t, NOW)).toBe("Banned");
    expect(moderationLabel(status({ state: "warned" }), t, NOW)).toBe("Warned");
    expect(moderationLabel(status({ state: "terminated" }), t, NOW)).toBe("Terminated");
    expect(moderationLabel(status({ state: "clean" }), t, NOW)).toBe("No moderation");
  });

  /** O ban acabou, mas o Roblox ainda pede para reativar a conta no site. */
  it("says when a ban is over", () => {
    const until = new Date(2026, 9, 8, 12, 0).toISOString();
    const s = status({ state: "banned", until });
    expect(isActiveBan(s, NOW)).toBe(false);
    expect(moderationLabel(s, t, NOW)).toBe("Ban ended 08/10 — reactivate it on roblox.com");
  });

  it("maps states to the row badge: bans and terminations share one, warnings another", () => {
    expect(moderationBadge(status({ state: "banned" }), NOW)).toBe("banned");
    expect(moderationBadge(status({ state: "terminated" }), NOW)).toBe("banned");
    expect(moderationBadge(status({ state: "warned" }), NOW)).toBe("warned");
    expect(moderationBadge(status({ state: "clean" }), NOW)).toBeNull();
    expect(moderationBadge(undefined, NOW)).toBeNull();
    // Ban que acabou ainda pede atenção (reativar no site): selo de aviso.
    expect(
      moderationBadge(status({ state: "banned", until: new Date(2026, 9, 1).toISOString() }), NOW)
    ).toBe("warned");
  });
});
