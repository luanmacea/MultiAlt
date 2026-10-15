import { describe, expect, it } from "vitest";
import {
  evaluateWaitPresence,
  isRateLimitError,
  nextCheckDelayMs,
  parseRetryAfterMs,
  planNextCheck,
  WAIT_ERROR_BACKOFF_MAX_MS,
  WAIT_MAX_DURATION_MS,
  WAIT_POLL_INTERVAL_MS,
} from "./waitForPlayer";

describe("evaluateWaitPresence", () => {
  const inGame = { userPresenceType: 2, placeId: 111, rootPlaceId: 999, gameId: "job-x" };

  it("offline, website and Studio are not joinable in either mode", () => {
    for (const type of [0, 1, 3]) {
      expect(evaluateWaitPresence({ userPresenceType: type, rootPlaceId: 999 }, true)).toEqual({
        joinable: false,
        reason: "not-in-game",
      });
      expect(evaluateWaitPresence({ userPresenceType: type, rootPlaceId: 999, gameId: "j" }, false).joinable).toBe(false);
    }
    expect(evaluateWaitPresence(null, true).joinable).toBe(false);
  });

  it("following: in a game is enough, even with the server hidden, and uses the root place", () => {
    expect(evaluateWaitPresence({ userPresenceType: 2, placeId: null, rootPlaceId: null, gameId: null }, true)).toEqual({
      joinable: true,
      placeId: 0,
      jobId: "",
      follow: true,
    });
    expect(evaluateWaitPresence(inGame, true)).toEqual({ joinable: true, placeId: 999, jobId: "", follow: true });
  });

  it("exact server: needs a visible Job ID and uses the server's place", () => {
    expect(evaluateWaitPresence(inGame, false)).toEqual({ joinable: true, placeId: 111, jobId: "job-x", follow: false });
    expect(evaluateWaitPresence({ ...inGame, gameId: "" }, false)).toEqual({ joinable: false, reason: "server-hidden" });
    expect(evaluateWaitPresence({ ...inGame, placeId: null, rootPlaceId: null }, false).joinable).toBe(false);
  });

  it("reads the snake_case shape too", () => {
    expect(
      evaluateWaitPresence({ user_presence_type: 2, place_id: 5, root_place_id: 6, game_id: "g" }, false)
    ).toEqual({ joinable: true, placeId: 5, jobId: "g", follow: false });
  });
});

describe("retry-after / rate limit", () => {
  it("reads the seconds the backend appends to a 429", () => {
    expect(parseRetryAfterMs("Failed to get presence (status 429, retry after 90 s)")).toBe(90_000);
    expect(parseRetryAfterMs("Failed to get presence (status 429)")).toBeNull();
  });

  it("recognises a 429", () => {
    expect(isRateLimitError("Failed to get presence (status 429)")).toBe(true);
    expect(isRateLimitError("Too Many Requests")).toBe(true);
    expect(isRateLimitError("Failed to get presence (status 503)")).toBe(false);
  });
});

describe("nextCheckDelayMs", () => {
  it("polls every 30 s while there is no error", () => {
    expect(WAIT_POLL_INTERVAL_MS).toBe(30_000);
    expect(nextCheckDelayMs(0)).toBe(WAIT_POLL_INTERVAL_MS);
  });

  it("doubles on consecutive errors up to 5 min", () => {
    const err = "Failed to get presence (status 503)";
    expect(nextCheckDelayMs(1, err)).toBe(60_000);
    expect(nextCheckDelayMs(2, err)).toBe(120_000);
    expect(nextCheckDelayMs(3, err)).toBe(240_000);
    expect(nextCheckDelayMs(4, err)).toBe(WAIT_ERROR_BACKOFF_MAX_MS);
    expect(nextCheckDelayMs(20, err)).toBe(WAIT_ERROR_BACKOFF_MAX_MS);
    expect(WAIT_ERROR_BACKOFF_MAX_MS).toBe(300_000);
  });

  it("a 429 without Retry-After goes straight to the ceiling", () => {
    expect(nextCheckDelayMs(1, "Failed to get presence (status 429)")).toBe(WAIT_ERROR_BACKOFF_MAX_MS);
  });

  it("Retry-After wins when it asks for longer, never shortens the backoff", () => {
    expect(nextCheckDelayMs(1, "status 429, retry after 600 s")).toBe(600_000);
    expect(nextCheckDelayMs(3, "status 429, retry after 10 s")).toBe(240_000);
  });
});

describe("planNextCheck", () => {
  it("gives up after the max duration (2 h by default)", () => {
    expect(WAIT_MAX_DURATION_MS).toBe(2 * 60 * 60_000);
    expect(planNextCheck({ startedAt: 0, now: WAIT_MAX_DURATION_MS, consecutiveErrors: 0 })).toEqual({ action: "give-up" });
    expect(planNextCheck({ startedAt: 0, now: 1000, consecutiveErrors: 0, maxDurationMs: 1000 })).toEqual({ action: "give-up" });
  });

  it("waits the poll interval, but the last check lands on the deadline", () => {
    expect(planNextCheck({ startedAt: 0, now: 0, consecutiveErrors: 0 })).toEqual({
      action: "wait",
      delayMs: WAIT_POLL_INTERVAL_MS,
    });
    expect(planNextCheck({ startedAt: 0, now: WAIT_MAX_DURATION_MS - 5_000, consecutiveErrors: 0 })).toEqual({
      action: "wait",
      delayMs: 5_000,
    });
  });

  it("carries the error backoff", () => {
    expect(
      planNextCheck({ startedAt: 0, now: 0, consecutiveErrors: 2, lastError: "status 503" })
    ).toEqual({ action: "wait", delayMs: 120_000 });
  });
});
