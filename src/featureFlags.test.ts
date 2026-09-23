import { afterEach, describe, expect, it, vi } from "vitest";

async function loadFlags(env: Record<string, string | undefined>) {
  vi.resetModules();
  for (const [key, value] of Object.entries(env)) {
    vi.stubEnv(key, value as string);
  }
  return import("./featureFlags");
}

afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

describe("feature flags", () => {
  it("default to enabled when the env var is unset or empty", async () => {
    const flags = await loadFlags({ VITE_ENABLE_NEXUS: undefined, VITE_ENABLE_WEBSERVER: "" });
    expect(flags.ENABLE_NEXUS).toBe(true);
    expect(flags.ENABLE_WEBSERVER).toBe(true);
  });

  it("accept the truthy spellings", async () => {
    for (const value of ["1", "true", "TRUE", "yes", "on", "  True  "]) {
      const flags = await loadFlags({ VITE_ENABLE_NEXUS: value });
      expect(flags.ENABLE_NEXUS, value).toBe(true);
    }
  });

  it("accept the falsy spellings", async () => {
    for (const value of ["0", "false", "FALSE", "no", "off", " off "]) {
      const flags = await loadFlags({ VITE_ENABLE_WEBSERVER: value });
      expect(flags.ENABLE_WEBSERVER, value).toBe(false);
    }
  });

  it("fall back to the default for unrecognized values", async () => {
    const flags = await loadFlags({ VITE_ENABLE_NEXUS: "maybe", VITE_ENABLE_WEBSERVER: "2" });
    expect(flags.ENABLE_NEXUS).toBe(true);
    expect(flags.ENABLE_WEBSERVER).toBe(true);
  });

  it("reads the two flags independently", async () => {
    const flags = await loadFlags({ VITE_ENABLE_NEXUS: "false", VITE_ENABLE_WEBSERVER: "true" });
    expect(flags.ENABLE_NEXUS).toBe(false);
    expect(flags.ENABLE_WEBSERVER).toBe(true);
  });
});
