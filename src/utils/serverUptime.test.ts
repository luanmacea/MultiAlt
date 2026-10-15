import { describe, expect, it } from "vitest";
import { serverInfoParts, serverUptimeLabel } from "./serverUptime";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(options?.[key] ?? ""));

const NOW = Date.UTC(2026, 9, 15, 3, 0, 0);
const MIN = 60_000;
const HOUR = 60 * MIN;

describe("serverUptimeLabel", () => {
  it("says minutes, then hours, then days", () => {
    expect(serverUptimeLabel(NOW - 30_000, NOW, t)).toBe("server up for 1 min");
    expect(serverUptimeLabel(NOW - 25 * MIN, NOW, t)).toBe("server up for 25 min");
    expect(serverUptimeLabel(NOW - 3 * HOUR - 20 * MIN, NOW, t)).toBe("server up for 3 h");
    expect(serverUptimeLabel(NOW - 47 * HOUR, NOW, t)).toBe("server up for 47 h");
    expect(serverUptimeLabel(NOW - 72 * HOUR, NOW, t)).toBe("server up for 3 d");
  });

  it("says nothing without a timestamp or with one far in the future", () => {
    expect(serverUptimeLabel(null, NOW, t)).toBeNull();
    expect(serverUptimeLabel(undefined, NOW, t)).toBeNull();
    expect(serverUptimeLabel(Number.NaN, NOW, t)).toBeNull();
    expect(serverUptimeLabel(NOW + HOUR, NOW, t)).toBeNull();
    // Um relógio um pouco atrasado ainda é "agora".
    expect(serverUptimeLabel(NOW + 2 * MIN, NOW, t)).toBe("server up for 1 min");
  });
});

describe("serverInfoParts", () => {
  it("lists the region and the uptime that are known", () => {
    expect(serverInfoParts({ region: "Miami, US", startedAtMs: NOW - 3 * HOUR }, NOW, t)).toEqual([
      "Miami, US",
      "server up for 3 h",
    ]);
    expect(serverInfoParts({ region: null, startedAtMs: NOW - 3 * HOUR }, NOW, t)).toEqual(["server up for 3 h"]);
    expect(serverInfoParts({ region: "  ", startedAtMs: null }, NOW, t)).toEqual([]);
    expect(serverInfoParts(null, NOW, t)).toEqual([]);
  });
});
