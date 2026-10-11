import { describe, expect, it } from "vitest";
import { accountLiveState, activeReconnectIds, attentionIds, type LiveSources } from "./accountLiveState";
import type { AutoReconnectEntry, ClientDrop } from "../types";

const DROP: ClientDrop = { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: 1 };

function reconnect(userId: number, phase: AutoReconnectEntry["phase"]): AutoReconnectEntry {
  return { userId, phase, attempt: 1, maxAttempts: 5, nextAttemptAtMs: null, reason: null, error: null, drop: DROP };
}

function sources(overrides: Partial<LiveSources> = {}): LiveSources {
  return {
    running: new Set(),
    joining: new Set(),
    launchQueue: null,
    clientHealth: new Map(),
    reconnecting: new Set(),
    sessions: new Map(),
    ...overrides,
  };
}

describe("accountLiveState", () => {
  it("idle account: none", () => {
    expect(accountLiveState(1, sources()).state).toBe("none");
  });

  it("running with a session: playing, with the game and since when", () => {
    const live = accountLiveState(
      1,
      sources({
        running: new Set([1]),
        sessions: new Map([[1, { userId: 1, placeId: 42, jobId: null, sinceMs: 1000, privateServer: null }]]),
      })
    );
    expect(live).toEqual({ state: "playing", placeId: 42, sinceMs: 1000 });
  });

  it("same priority as the Session page: reconnecting > not responding > dropped", () => {
    const health = new Map([[1, { pid: 1, logFound: true, drop: DROP, notResponding: true }]]);
    expect(accountLiveState(1, sources({ running: new Set([1]), clientHealth: health })).state).toBe("notResponding");
    expect(
      accountLiveState(1, sources({ running: new Set([1]), clientHealth: health, reconnecting: new Set([1]) })).state
    ).toBe("reconnecting");
  });

  it("running without a session and without a log: nothing to add", () => {
    expect(accountLiveState(1, sources({ running: new Set([1]) })).state).toBe("none");
  });

  it("joining from the queue carries the queue's game", () => {
    const live = accountLiveState(
      1,
      sources({
        launchQueue: {
          entries: [{ userId: 1, state: "launching", error: null, updatedAtMs: 1 }],
          active: true,
          placeId: 77,
          jobId: "",
        },
      })
    );
    expect(live).toEqual({ state: "joining", placeId: 77, sinceMs: null });
  });

  it("activeReconnectIds leaves out the ones that stopped or gave up", () => {
    const ids = activeReconnectIds([reconnect(1, "waiting"), reconnect(2, "gaveUp"), reconnect(3, "stopped")]);
    expect([...ids]).toEqual([1]);
  });

  it("attentionIds counts running and reconnecting accounts that need attention, once each", () => {
    const ids = attentionIds(
      sources({
        running: new Set([1, 2, 3]),
        clientHealth: new Map([
          [1, { pid: 1, logFound: true, drop: DROP }],
          [2, { pid: 2, logFound: true, drop: null, inGame: true }],
        ]),
        reconnecting: new Set([1, 4]),
      })
    );
    expect([...ids].sort()).toEqual([1, 4]);
  });
});
