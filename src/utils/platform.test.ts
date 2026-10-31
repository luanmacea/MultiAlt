import { afterEach, describe, expect, it } from "vitest";
import type { PlatformCapabilities } from "../types";
import { isLinuxPlatform, isWindowsPlatform, platformFeatures } from "./platform";

function caps(os: string): PlatformCapabilities {
  return {
    os,
    sessionType: "",
    preferredRunner: "",
    detectedRunner: "",
    runnerPath: null,
    supportsSingleLaunch: true,
    supportsMultiLaunch: true,
    supportsWatcher: true,
    supportsWatcherMemory: true,
    supportsWindowControls: true,
    supportsBotting: true,
    supportsUpdater: true,
    supportsClientSettings: true,
    supportsLiveAudio: false,
    supportsMemoryTrim: false,
    supportsLiveView: true,
    supportsAfkInput: true,
    supportsIsolation: true,
    supportsMultiRobloxMutex: true,
    supportsVersions: true,
    supportsOptimization: true,
    supportsExternalClients: true,
    supportsPython: true,
    reasons: [],
    warnings: [],
  };
}

/** Exactly what `commands/platform_info.rs` reports for Linux. */
function linuxCaps(): PlatformCapabilities {
  return {
    ...caps("linux"),
    preferredRunner: "sober",
    detectedRunner: "sober",
    supportsWatcher: false,
    supportsWatcherMemory: false,
    supportsWindowControls: false,
    supportsBotting: false,
    supportsClientSettings: false,
    supportsLiveView: false,
    supportsAfkInput: false,
    supportsIsolation: false,
    supportsMultiRobloxMutex: false,
    supportsVersions: false,
    supportsOptimization: false,
    supportsExternalClients: false,
    supportsPython: false,
  };
}

const originalUserAgent = navigator.userAgent;

function setUserAgent(value: string) {
  Object.defineProperty(navigator, "userAgent", { value, configurable: true });
}

afterEach(() => {
  setUserAgent(originalUserAgent);
});

describe("isWindowsPlatform", () => {
  it("trusts the reported capabilities first", () => {
    setUserAgent("Mozilla/5.0 (X11; Linux x86_64)");
    expect(isWindowsPlatform(caps("windows"))).toBe(true);
  });

  it("is false for any other reported OS, even on a Windows user agent", () => {
    setUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    expect(isWindowsPlatform(caps("linux"))).toBe(false);
    expect(isWindowsPlatform(caps("macos"))).toBe(false);
  });

  it("falls back to the user agent when capabilities are missing or unknown", () => {
    setUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    expect(isWindowsPlatform(null)).toBe(true);
    expect(isWindowsPlatform(caps(""))).toBe(true);

    setUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
    expect(isWindowsPlatform(null)).toBe(false);
  });

  it("matches the user agent case-insensitively", () => {
    setUserAgent("SOMETHING WINDOWS SOMETHING");
    expect(isWindowsPlatform(null)).toBe(true);
  });
});

describe("isLinuxPlatform", () => {
  it("is true only for a reported linux", () => {
    expect(isLinuxPlatform(caps("linux"))).toBe(true);
    expect(isLinuxPlatform(caps("windows"))).toBe(false);
    expect(isLinuxPlatform(caps("macos"))).toBe(false);
    expect(isLinuxPlatform(null)).toBe(false);
  });
});

describe("platformFeatures", () => {
  it("shows everything while the platform is not known yet or on Windows", () => {
    for (const input of [null, caps("windows"), caps("macos")]) {
      const f = platformFeatures(input);
      expect(Object.values(f).filter((v) => v === false)).toEqual(
        // `linuxSection` is the only thing that is off outside Linux.
        [false]
      );
      expect(f.linuxSection).toBe(false);
    }
  });

  it("treats a field the backend did not send as supported", () => {
    const partial = { os: "windows" } as unknown as PlatformCapabilities;
    const f = platformFeatures(partial);
    expect(f.liveView).toBe(true);
    expect(f.afkPage).toBe(true);
    expect(f.isolation).toBe(true);
  });

  it("hides every Windows-only feature on Linux", () => {
    const f = platformFeatures(linuxCaps());
    expect(f).toEqual({
      liveView: false,
      afkInput: false,
      afkPage: false,
      isolation: false,
      multiRobloxMutex: false,
      versions: false,
      optimization: false,
      clientSettings: false,
      externalClients: false,
      python: false,
      watcher: false,
      linuxSection: true,
    });
  });

  it("keeps the AFK page when Auto Rejoin works even without AFK input", () => {
    const f = platformFeatures({ ...linuxCaps(), supportsBotting: true });
    expect(f.afkInput).toBe(false);
    expect(f.afkPage).toBe(true);
  });
});
