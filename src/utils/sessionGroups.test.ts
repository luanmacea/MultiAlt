import { describe, expect, it } from "vitest";
import { groupRunning, runningStatus, shortJobId, type RunningStatus } from "./sessionGroups";
import type { ClientHealth, CurrentSession } from "../types";

function session(userId: number, placeId: number, jobId: string | null, privateServer: boolean | null = false): CurrentSession {
  return { userId, placeId, jobId, sinceMs: 1, privateServer };
}

function health(partial: Partial<ClientHealth> = {}): ClientHealth {
  return { pid: 1, logFound: true, drop: null, ...partial };
}

const DROP = { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: 1 } as const;

describe("runningStatus", () => {
  it("puts reconnecting first, then not responding, then the drop", () => {
    expect(runningStatus(session(1, 1, "a"), health({ drop: DROP, notResponding: true }), true)).toBe("reconnecting");
    expect(runningStatus(session(1, 1, "a"), health({ drop: DROP, notResponding: true }), false)).toBe("notResponding");
    expect(runningStatus(undefined, health({ drop: DROP }), false)).toBe("dropped");
  });

  it("is playing with a session, waiting when the log says it has no game yet, else unknown", () => {
    expect(runningStatus(session(1, 1, "a"), health(), false)).toBe("playing");
    expect(runningStatus(undefined, health({ inGame: false }), false)).toBe("waiting");
    expect(runningStatus(undefined, undefined, false)).toBe("unknown");
  });
});

describe("groupRunning", () => {
  const statusOf =
    (map: Record<number, RunningStatus>) =>
    (id: number): RunningStatus =>
      map[id] ?? "playing";

  it("puts accounts of the same game and server together, biggest groups first", () => {
    const sessions = new Map([
      [1, session(1, 100, "job-a")],
      [2, session(2, 200, "job-b")],
      [3, session(3, 100, "job-a")],
      [4, session(4, 100, "job-c")],
    ]);
    const groups = groupRunning([1, 2, 3, 4], sessions, statusOf({}));
    expect(groups.attention).toEqual([]);
    expect(groups.noGame).toEqual([]);
    expect(groups.games.map((g) => [g.placeId, g.userIds])).toEqual([
      [100, [1, 3, 4]],
      [200, [2]],
    ]);
    expect(groups.games[0].servers.map((s) => [s.jobId, s.userIds])).toEqual([
      ["job-a", [1, 3]],
      ["job-c", [4]],
    ]);
  });

  it("keeps a private server and an unknown server apart from the public ones", () => {
    const sessions = new Map([
      [1, session(1, 100, "job-a", true)],
      [2, session(2, 100, null, null)],
      [3, session(3, 100, "job-b", false)],
    ]);
    const [game] = groupRunning([1, 2, 3], sessions, statusOf({})).games;
    expect(game.servers.map((s) => [s.jobId, s.privateServer, s.userIds])).toEqual([
      ["job-a", true, [1]],
      [null, null, [2]],
      ["job-b", false, [3]],
    ]);
  });

  it("lifts accounts that need attention out of their game, keeping the list order", () => {
    const sessions = new Map([
      [1, session(1, 100, "job-a")],
      [2, session(2, 100, "job-a")],
      [3, session(3, 100, "job-a")],
    ]);
    const groups = groupRunning([1, 2, 3, 4], sessions, statusOf({ 2: "notResponding", 4: "dropped" }));
    expect(groups.attention).toEqual([2, 4]);
    expect(groups.games[0].userIds).toEqual([1, 3]);
  });

  it("lists accounts without a game apart", () => {
    const groups = groupRunning([5, 6], new Map(), statusOf({ 5: "waiting", 6: "unknown" }));
    expect(groups.games).toEqual([]);
    expect(groups.noGame).toEqual([5, 6]);
  });
});

describe("shortJobId", () => {
  it("shows the first 8 characters of a Job ID", () => {
    expect(shortJobId("1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d")).toBe("1a2b3c4d");
    expect(shortJobId("abc")).toBe("abc");
  });
});
