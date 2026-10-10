import { describe, expect, it } from "vitest";
import { autoReconnectLabel } from "./autoReconnect";
import type { AutoReconnectEntry } from "../types";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(options?.[key] ?? ""));

const NOW = 1_700_000_000_000;

function entry(partial: Partial<AutoReconnectEntry>): AutoReconnectEntry {
  return {
    userId: 1,
    phase: "waiting",
    attempt: 1,
    maxAttempts: 5,
    nextAttemptAtMs: NOW + 30_000,
    reason: null,
    error: null,
    drop: { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: NOW },
    ...partial,
  };
}

describe("autoReconnectLabel", () => {
  it("counts down to the next attempt", () => {
    const info = autoReconnectLabel(entry({ attempt: 2 }), NOW, t);
    expect(info.label).toBe("Reconnecting in 30 s (attempt 2/5)");
    expect(info).toMatchObject({ tone: "pending", canTryNow: true, canTryAgain: false });
    expect(autoReconnectLabel(entry({ nextAttemptAtMs: NOW + 120_000 }), NOW, t).label).toBe(
      "Reconnecting in 2 min (attempt 1/5)"
    );
    // Passou da hora (a passada do backend ainda não pegou): nunca negativo.
    expect(autoReconnectLabel(entry({ nextAttemptAtMs: NOW - 5_000 }), NOW, t).label).toBe(
      "Reconnecting in 0 s (attempt 1/5)"
    );
  });

  it("says what is happening now", () => {
    expect(autoReconnectLabel(entry({ phase: "waitingForInternet" }), NOW, t).label).toBe(
      "Waiting for the internet to come back (attempt 1/5)"
    );
    expect(autoReconnectLabel(entry({ phase: "launching", attempt: 3 }), NOW, t)).toMatchObject({
      label: "Reconnecting now (attempt 3/5)",
      canTryNow: false,
    });
    expect(autoReconnectLabel(entry({ phase: "checking" }), NOW, t).label).toBe(
      "Reopened, checking it stays in the game (attempt 1/5)"
    );
  });

  it("gave up, with the last error in the tooltip and a way to try again", () => {
    const info = autoReconnectLabel(entry({ phase: "gaveUp", attempt: 5, error: "boom" }), NOW, t);
    expect(info).toMatchObject({ label: "Gave up after 5 tries", detail: "boom", tone: "final", canTryAgain: true });
  });

  it("names why it will not reconnect", () => {
    const cases: [AutoReconnectEntry["reason"], string, boolean][] = [
      ["joinedElsewhere", "Not reconnecting: the account joined somewhere else", true],
      ["closedByUser", "Not reconnecting: the client was closed", true],
      ["banned", "Not reconnecting: the account is banned", false],
      ["sessionExpired", "Not reconnecting: sign in to this account again", true],
      ["noDestination", "Not reconnecting: no game to go back to", false],
      ["openedOutsideApp", "Not reconnecting: the account is open outside the app", true],
    ];
    for (const [reason, label, canTryAgain] of cases) {
      expect(autoReconnectLabel(entry({ phase: "stopped", reason }), NOW, t)).toMatchObject({
        label,
        tone: "final",
        canTryAgain,
      });
    }
  });
});

describe("the countdown", () => {
  it("is seconds under a minute and minutes after", () => {
    const at = (ms: number) => autoReconnectLabel(entry({ nextAttemptAtMs: NOW + ms }), NOW, t).label;
    expect(at(9_500)).toBe("Reconnecting in 10 s (attempt 1/5)");
    expect(at(60_000)).toBe("Reconnecting in 1 min (attempt 1/5)");
    expect(at(299_000)).toBe("Reconnecting in 5 min (attempt 1/5)");
  });
});
