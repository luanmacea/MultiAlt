import { describe, expect, it } from "vitest";
import {
  AWAY_DIGEST_CAP,
  AWAY_DEDUPE_MS,
  EMPTY_AWAY_DIGEST,
  afkFailureEvents,
  awayDigestReducer,
  awayEventText,
  awayUnreadCount,
  clientHealthEvent,
  type AwayDigestState,
  type AwayEventInput,
} from "./awayDigest";
import type { AfkStatus } from "../store";
import type { AutoReconnectEntry, ClientDrop } from "../types";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_m, key: string) => String(options?.[key] ?? ""));

const LOST: ClientDrop = { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: 0 };

function add(state: AwayDigestState, event: AwayEventInput): AwayDigestState {
  return awayDigestReducer(state, { type: "add", event });
}

function afk(accounts: { userId: number; code: string | null; atMs?: number }[]): AfkStatus {
  return {
    active: true,
    startedAtMs: 1,
    intervalSeconds: 600,
    key: "Space",
    mode: "key",
    clickX: 50,
    clickY: 50,
    accounts: accounts.map((a) => ({
      userId: a.userId,
      lastSendAtMs: a.atMs ?? 1_000,
      nextSendAtMs: (a.atMs ?? 1_000) + 600_000,
      sends: 0,
      lastError: a.code ? `raw ${a.code}` : null,
      lastErrorCode: a.code,
    })),
  };
}

describe("awayDigestReducer", () => {
  it("keeps the newest first, even when an event arrives with an older time", () => {
    let s = EMPTY_AWAY_DIGEST;
    s = add(s, { kind: "dropped", userId: 1, atMs: 10_000, drop: LOST });
    s = add(s, { kind: "notResponding", userId: 2, atMs: 30_000 });
    // Falha do AFK chega com a hora do envio, que pode ser anterior.
    s = add(s, { kind: "afkSendFailed", userId: 3, atMs: 20_000, code: "noWindow", error: null });
    expect(s.events.map((e) => e.atMs)).toEqual([30_000, 20_000, 10_000]);
    expect(new Set(s.events.map((e) => e.id)).size).toBe(3);
  });

  it(`keeps only the last ${AWAY_DIGEST_CAP} events`, () => {
    let s = EMPTY_AWAY_DIGEST;
    for (let i = 0; i < AWAY_DIGEST_CAP + 10; i++) {
      s = add(s, { kind: "notResponding", userId: i, atMs: i * 60_000 });
    }
    expect(s.events).toHaveLength(AWAY_DIGEST_CAP);
    expect(s.events[0].userId).toBe(AWAY_DIGEST_CAP + 9);
    expect(s.events[AWAY_DIGEST_CAP - 1].userId).toBe(10);
  });

  it("drops a repeat of the same event within a few seconds, but not a different one", () => {
    let s = EMPTY_AWAY_DIGEST;
    s = add(s, { kind: "dropped", userId: 1, atMs: 1_000, drop: LOST });
    s = add(s, { kind: "dropped", userId: 1, atMs: 1_000 + AWAY_DEDUPE_MS - 1, drop: LOST });
    expect(s.events).toHaveLength(1);
    // Outra conta, outro motivo ou fora da janela: entra.
    s = add(s, { kind: "dropped", userId: 2, atMs: 2_000, drop: LOST });
    s = add(s, { kind: "dropped", userId: 1, atMs: 2_000, drop: { ...LOST, kind: "kicked", reason: null } });
    s = add(s, { kind: "dropped", userId: 1, atMs: 1_000 + AWAY_DEDUPE_MS + 1, drop: LOST });
    expect(s.events).toHaveLength(4);
  });

  it("counts unread since the last look, and clear empties without bringing them back", () => {
    let s = EMPTY_AWAY_DIGEST;
    s = add(s, { kind: "notResponding", userId: 1, atMs: 1 });
    s = add(s, { kind: "respondingAgain", userId: 1, atMs: 2 });
    expect(awayUnreadCount(s)).toBe(2);
    s = awayDigestReducer(s, { type: "seen" });
    expect(awayUnreadCount(s)).toBe(0);
    s = add(s, { kind: "emergencyStop", userId: null, atMs: 3, afk: true, recordings: false });
    expect(awayUnreadCount(s)).toBe(1);
    s = awayDigestReducer(s, { type: "clear" });
    expect(s.events).toEqual([]);
    expect(awayUnreadCount(s)).toBe(0);
    s = add(s, { kind: "notResponding", userId: 4, atMs: 4 });
    expect(awayUnreadCount(s)).toBe(1);
  });

  it("returns the same state when nothing changes (no extra render)", () => {
    const s = add(EMPTY_AWAY_DIGEST, { kind: "notResponding", userId: 1, atMs: 1 });
    const seen = awayDigestReducer(s, { type: "seen" });
    expect(awayDigestReducer(seen, { type: "seen" })).toBe(seen);
    expect(awayDigestReducer(EMPTY_AWAY_DIGEST, { type: "clear" })).toBe(EMPTY_AWAY_DIGEST);
  });
});

describe("clientHealthEvent", () => {
  it("reads the three shapes the backend sends (client_health.rs)", () => {
    expect(clientHealthEvent({ userId: 1, drop: LOST }, 5)).toMatchObject({ kind: "dropped", userId: 1, atMs: 5, drop: LOST });
    expect(clientHealthEvent({ userId: 1, notResponding: true }, 5)).toMatchObject({ kind: "notResponding" });
    expect(clientHealthEvent({ userId: 1, notResponding: false }, 5)).toMatchObject({ kind: "respondingAgain" });
    expect(clientHealthEvent({ userId: 1, drop: null }, 5)).toMatchObject({ kind: "backInGame" });
  });

  it("ignores a payload without an account", () => {
    expect(clientHealthEvent({ drop: LOST } as never, 5)).toBeNull();
  });
});

describe("afkFailureEvents", () => {
  it("notes a failure once, when it starts or changes, not on every cycle", () => {
    let prev = new Map<number, string | null>();
    let out = afkFailureEvents(prev, afk([{ userId: 1, code: "focusDenied", atMs: 7 }, { userId: 2, code: null }]));
    expect(out.events).toEqual([
      { kind: "afkSendFailed", userId: 1, atMs: 7, code: "focusDenied", error: "raw focusDenied" },
    ]);
    prev = out.next;
    out = afkFailureEvents(prev, afk([{ userId: 1, code: "focusDenied", atMs: 9 }]));
    expect(out.events).toEqual([]);
    prev = out.next;
    out = afkFailureEvents(prev, afk([{ userId: 1, code: "noWindow", atMs: 11 }]));
    expect(out.events.map((e) => e.code)).toEqual(["noWindow"]);
    // Voltou a enviar e falhou de novo: é outra falha.
    prev = afkFailureEvents(out.next, afk([{ userId: 1, code: null }])).next;
    expect(afkFailureEvents(prev, afk([{ userId: 1, code: "noWindow", atMs: 20 }])).events).toHaveLength(1);
  });

  it("forgets everything when the mode stops", () => {
    const prev = afkFailureEvents(new Map(), afk([{ userId: 1, code: "noWindow" }])).next;
    expect(afkFailureEvents(prev, null).next.size).toBe(0);
  });
});

describe("awayEventText", () => {
  const entry = (phase: string): AutoReconnectEntry =>
    ({ userId: 1, phase, attempt: 5, maxAttempts: 5, nextAttemptAtMs: null, reason: null, error: null, drop: LOST }) as AutoReconnectEntry;

  it("says what happened, amber for problems and green for recoveries", () => {
    const text = (event: AwayEventInput) => awayEventText({ id: 1, ...event }, t);
    expect(text({ kind: "dropped", userId: 1, atMs: 0, drop: LOST })).toEqual({
      text: "Disconnected: lost connection",
      tone: "problem",
    });
    expect(text({ kind: "backInGame", userId: 1, atMs: 0 }).tone).toBe("recovered");
    expect(text({ kind: "reconnected", userId: 1, atMs: 0 }).tone).toBe("recovered");
    expect(text({ kind: "respondingAgain", userId: 1, atMs: 0 }).tone).toBe("recovered");
    expect(text({ kind: "notResponding", userId: 1, atMs: 0 })).toEqual({ text: "Not responding", tone: "problem" });
    expect(text({ kind: "reconnectEnded", userId: 1, atMs: 0, reconnect: entry("gaveUp") }).text).toBe(
      "Gave up after 5 tries"
    );
    expect(text({ kind: "afkSendFailed", userId: 1, atMs: 0, code: "noWindow", error: "x" }).text).toBe(
      "AFK send failed: no Roblox window open"
    );
    expect(text({ kind: "afkSendFailed", userId: 1, atMs: 0, code: "weird", error: "boom" }).text).toBe(
      "AFK send failed: boom"
    );
    expect(text({ kind: "emergencyStop", userId: null, atMs: 0, afk: true, recordings: false }).text).toBe(
      "You shook the mouse, so MultiAlt stopped AFK mode."
    );
    expect(text({ kind: "rejoinFailed", userId: 1, atMs: 0, error: "no build" }).text).toBe(
      "Auto Rejoin could not reopen it: no build"
    );
  });
});
