import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import {
  cancelWaitForPlayer,
  getWaitWatch,
  setWaitHost,
  startWaitForPlayer,
  type WaitHost,
} from "./waitForPlayerWatch";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { WAIT_ERROR_BACKOFF_MAX_MS, WAIT_MAX_DURATION_MS, WAIT_POLL_INTERVAL_MS } from "../../utils/waitForPlayer";

function makeHost(byFollowing = true) {
  const launch = vi.fn<WaitHost["launch"]>(async () => true);
  const notify = vi.fn<WaitHost["notify"]>();
  const host: WaitHost = { byFollowing: () => byFollowing, launch, notify };
  return Object.assign(host, { launch, notify });
}

function presenceCalls() {
  return invokeMock.mock.calls.filter((call) => call[0] === "get_presence").length;
}

beforeEach(() => {
  vi.useFakeTimers();
  resetTauriMocks();
});

afterEach(() => {
  cancelWaitForPlayer();
  setWaitHost(null);
  vi.useRealTimers();
});

describe("waitForPlayerWatch", () => {
  it("checks every 30 s while the player is offline and launches once when they join", async () => {
    let type = 0;
    setInvokeHandler((cmd) =>
      cmd === "get_presence" ? [{ userId: 42, userPresenceType: type, rootPlaceId: 999, placeId: 111, gameId: null }] : undefined
    );
    const host = makeHost(true);
    setWaitHost(host);
    startWaitForPlayer({ targetUserId: 42, name: "Builderman", userIds: [1, 2] });

    expect(presenceCalls()).toBe(0);
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS);
    expect(presenceCalls()).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith("get_presence", { userIds: [42] });
    expect(getWaitWatch()?.phase).toBe("watching");

    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS);
    expect(presenceCalls()).toBe(2);
    expect(host.launch).not.toHaveBeenCalled();

    type = 2;
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS);
    expect(host.launch).toHaveBeenCalledTimes(1);
    expect(host.launch).toHaveBeenCalledWith(
      expect.objectContaining({ targetUserId: 42, userIds: [1, 2] }),
      { joinable: true, placeId: 999, jobId: "", follow: true }
    );
    expect(getWaitWatch()).toBeNull();

    // Um launch só: nada mais é consultado nem lançado.
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS * 5);
    expect(presenceCalls()).toBe(3);
    expect(host.launch).toHaveBeenCalledTimes(1);
  });

  it("with the exact-server mode, a hidden server keeps waiting", async () => {
    setInvokeHandler(() => [{ userId: 42, userPresenceType: 2, rootPlaceId: 999, placeId: 111, gameId: "" }]);
    const host = makeHost(false);
    setWaitHost(host);
    startWaitForPlayer({ targetUserId: 42, name: "B", userIds: [1] });

    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS * 3);
    expect(host.launch).not.toHaveBeenCalled();
    expect(getWaitWatch()?.checks).toBe(3);
  });

  it("cancel stops the checks", async () => {
    setInvokeHandler(() => [{ userId: 42, userPresenceType: 0 }]);
    setWaitHost(makeHost());
    startWaitForPlayer({ targetUserId: 42, name: "B", userIds: [1] });
    cancelWaitForPlayer();
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS * 4);
    expect(presenceCalls()).toBe(0);
    expect(getWaitWatch()).toBeNull();
  });

  it("without a host (Choose Game closed) the watch ends instead of polling", async () => {
    setInvokeHandler(() => [{ userId: 42, userPresenceType: 0 }]);
    startWaitForPlayer({ targetUserId: 42, name: "B", userIds: [1] });
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS);
    expect(presenceCalls()).toBe(0);
    expect(getWaitWatch()).toBeNull();
  });

  it("backs off on errors and honours a 429's Retry-After", async () => {
    let fail: string | null = "Failed to get presence (status 503)";
    setInvokeHandler(() => {
      if (fail) throw new Error(fail);
      return [{ userId: 42, userPresenceType: 0 }];
    });
    setWaitHost(makeHost());
    startWaitForPlayer({ targetUserId: 42, name: "B", userIds: [1] });

    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS);
    expect(presenceCalls()).toBe(1);
    expect(getWaitWatch()?.lastError).toContain("503");
    // 1º erro: 60 s.
    await vi.advanceTimersByTimeAsync(59_000);
    expect(presenceCalls()).toBe(1);
    fail = "Failed to get presence (status 429, retry after 400 s)";
    await vi.advanceTimersByTimeAsync(1_000);
    expect(presenceCalls()).toBe(2);
    // Retry-After de 400 s vence o backoff de 120 s.
    await vi.advanceTimersByTimeAsync(399_000);
    expect(presenceCalls()).toBe(2);
    fail = "Failed to get presence (status 429)";
    await vi.advanceTimersByTimeAsync(1_000);
    expect(presenceCalls()).toBe(3);
    // 429 sem Retry-After: teto de 5 min.
    await vi.advanceTimersByTimeAsync(WAIT_ERROR_BACKOFF_MAX_MS - 1_000);
    expect(presenceCalls()).toBe(3);
    fail = null;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(presenceCalls()).toBe(4);
    expect(getWaitWatch()?.lastError).toBeNull();
    expect(getWaitWatch()?.consecutiveErrors).toBe(0);
  });

  it("gives up after the max duration and tells the user", async () => {
    setInvokeHandler(() => [{ userId: 42, userPresenceType: 0 }]);
    const host = makeHost();
    setWaitHost(host);
    startWaitForPlayer({ targetUserId: 42, name: "Builderman", userIds: [1] });
    await vi.advanceTimersByTimeAsync(WAIT_MAX_DURATION_MS + WAIT_POLL_INTERVAL_MS);
    expect(getWaitWatch()).toBeNull();
    expect(host.notify).toHaveBeenCalledWith(expect.stringContaining("Stopped waiting for Builderman"));
    const calls = presenceCalls();
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS * 4);
    expect(presenceCalls()).toBe(calls);
    expect(host.launch).not.toHaveBeenCalled();
  });

  it("a new watch replaces the previous one", async () => {
    setInvokeHandler(() => [{ userId: 7, userPresenceType: 0 }]);
    setWaitHost(makeHost());
    startWaitForPlayer({ targetUserId: 42, name: "A", userIds: [1] });
    startWaitForPlayer({ targetUserId: 7, name: "B", userIds: [1] });
    await vi.advanceTimersByTimeAsync(WAIT_POLL_INTERVAL_MS);
    expect(presenceCalls()).toBe(1);
    expect(getWaitWatch()?.name).toBe("B");
  });
});
