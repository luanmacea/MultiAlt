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

  it("keeps the paid account generator off unless the build turns it on", async () => {
    let flags = await loadFlags({ VITE_ENABLE_ACCOUNT_GENERATOR: undefined });
    expect(flags.ENABLE_ACCOUNT_GENERATOR).toBe(false);
    flags = await loadFlags({ VITE_ENABLE_ACCOUNT_GENERATOR: "maybe" });
    expect(flags.ENABLE_ACCOUNT_GENERATOR).toBe(false);
    flags = await loadFlags({ VITE_ENABLE_ACCOUNT_GENERATOR: "true" });
    expect(flags.ENABLE_ACCOUNT_GENERATOR).toBe(true);
  });

  it("keeps avatar distribution on unless the build turns it off", async () => {
    let flags = await loadFlags({ VITE_ENABLE_AVATAR_BATCH: undefined });
    expect(flags.ENABLE_AVATAR_BATCH).toBe(true);
    flags = await loadFlags({ VITE_ENABLE_AVATAR_BATCH: "false" });
    expect(flags.ENABLE_AVATAR_BATCH).toBe(false);
    // As outras flags não mudam por causa desta.
    expect(flags.ENABLE_NEXUS).toBe(true);
    expect(flags.ENABLE_WEBSERVER).toBe(true);
  });

  it("keeps the sidebar Help button hidden unless the build turns it on", async () => {
    let flags = await loadFlags({ VITE_ENABLE_HELP_BUTTON: undefined });
    expect(flags.ENABLE_HELP_BUTTON).toBe(false);
    flags = await loadFlags({ VITE_ENABLE_HELP_BUTTON: "true" });
    expect(flags.ENABLE_HELP_BUTTON).toBe(true);
  });

  it("reads the two flags independently", async () => {
    const flags = await loadFlags({ VITE_ENABLE_NEXUS: "false", VITE_ENABLE_WEBSERVER: "true" });
    expect(flags.ENABLE_NEXUS).toBe(false);
    expect(flags.ENABLE_WEBSERVER).toBe(true);
  });
});
