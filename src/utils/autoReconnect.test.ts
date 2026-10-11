import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { autoReconnectLabel, fieldsWithReconnect, reconnectChoice, reconnectErrorText } from "./autoReconnect";
import ptCommon from "../locales/pt/common.json";
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

/**
 * O erro da tentativa vem do Rust em inglês (`RECONNECT_ERROR_*` em
 * commands/reconnect.rs) e aparecia cru na tela em português. A lista sai do
 * próprio fonte do Rust: frase nova lá sem tradução aqui reprova.
 */
describe("the error of an attempt", () => {
  const source = readFileSync(resolve(__dirname, "../../src-tauri/src/commands/reconnect.rs"), "utf8");
  const backendErrors = [...source.matchAll(/pub const RECONNECT_ERROR_\w+: &str = "([^"]+)";/g)].map((m) => m[1]);
  const pt = ptCommon as Record<string, string>;
  const ptT = (text: string, options?: Record<string, unknown>) => t(pt[text] ?? text, options);

  it("lists the messages the backend writes", () => {
    expect(backendErrors).toEqual(
      expect.arrayContaining(["It did not get into the game in 2 minutes", "The Roblox client did not start"])
    );
  });

  it("is translated for every message the backend writes", () => {
    for (const message of backendErrors) {
      const text = reconnectErrorText(message, ptT);
      expect(pt[message], message).toBeTruthy();
      expect(text, message).toBe(pt[message]);
      expect(text, message).not.toBe(message);
    }
  });

  it("goes translated into the reconnect line", () => {
    const info = autoReconnectLabel(
      entry({ phase: "gaveUp", attempt: 5, error: "It did not get into the game in 2 minutes" }),
      NOW,
      ptT
    );
    expect(info.detail).toBe(pt["It did not get into the game in 2 minutes"]);
  });

  it("keeps an unknown message as it came", () => {
    expect(reconnectErrorText("PID não detectado", ptT)).toBe("PID não detectado");
  });
});

/** Mesma regra de `reconnect_enabled` (commands/reconnect.rs). */
describe("reconnectChoice", () => {
  it("follows the default without a field on the account", () => {
    expect(reconnectChoice({}, true, false)).toEqual({
      own: null,
      globalDefault: true,
      nexusForced: false,
      effective: true,
    });
    expect(reconnectChoice(undefined, false, false).effective).toBe(false);
  });

  it("the account's field wins over the default", () => {
    expect(reconnectChoice({ AutoReconnect: "false" }, true, false)).toMatchObject({ own: false, effective: false });
    expect(reconnectChoice({ AutoReconnect: "true" }, false, false)).toMatchObject({ own: true, effective: true });
  });

  it("Nexus AutoRelaunch turns it on over both", () => {
    expect(reconnectChoice({ AutoReconnect: "false" }, false, true)).toMatchObject({ own: false, effective: true });
  });

  it("an unknown value counts as no choice", () => {
    expect(reconnectChoice({ AutoReconnect: "yes" }, true, false).own).toBeNull();
  });
});

describe("fieldsWithReconnect", () => {
  it("writes the choice and keeps the other fields", () => {
    expect(fieldsWithReconnect({ RobloxVersion: "LIVE:abc" }, false)).toEqual({
      RobloxVersion: "LIVE:abc",
      AutoReconnect: "false",
    });
  });

  it("null removes the field (back to the default) without touching the original", () => {
    const original = { AutoReconnect: "true", Other: "x" };
    expect(fieldsWithReconnect(original, null)).toEqual({ Other: "x" });
    expect(original.AutoReconnect).toBe("true");
  });
});
