import { describe, expect, it } from "vitest";
import {
  EMPTY_ACCOUNT_LAUNCH_OVERRIDES,
  readAccountLaunchOverrides,
  writeAccountLaunchOverrides,
} from "./types";

describe("readAccountLaunchOverrides", () => {
  it("reads nothing from an account without exceptions", () => {
    expect(readAccountLaunchOverrides({})).toEqual(EMPTY_ACCOUNT_LAUNCH_OVERRIDES);
    expect(readAccountLaunchOverrides(undefined)).toEqual(EMPTY_ACCOUNT_LAUNCH_OVERRIDES);
  });

  it("turns the stored volume fraction back into the on-screen scale", () => {
    expect(readAccountLaunchOverrides({ ClientOverrideVolume: "0.200" }).volume).toBe("2");
    expect(readAccountLaunchOverrides({ ClientOverrideVolume: "1.000" }).volume).toBe("10");
  });

  it("ignores a boolean field that is not a boolean", () => {
    const o = readAccountLaunchOverrides({
      ClientOverrideFullscreen: "talvez",
      ClientOverrideStartMinimized: "TRUE",
    });
    expect(o.fullscreen).toBe("");
    expect(o.startMinimized).toBe("true");
  });
});

describe("writeAccountLaunchOverrides", () => {
  it("deletes the key instead of storing an empty value", () => {
    const fields = writeAccountLaunchOverrides(
      { ClientOverrideMaxFPS: "240", RobloxVersion: "LIVE:abc" },
      { ...EMPTY_ACCOUNT_LAUNCH_OVERRIDES, enabled: true }
    );
    expect("ClientOverrideMaxFPS" in fields).toBe(false);
    // Campos de outra funcionalidade não são do nosso alcance.
    expect(fields.RobloxVersion).toBe("LIVE:abc");
    expect(fields.ClientOverridesEnabled).toBe("true");
  });

  it("stores the on-screen volume as the fraction the XML keeps", () => {
    const fields = writeAccountLaunchOverrides(
      {},
      { ...EMPTY_ACCOUNT_LAUNCH_OVERRIDES, enabled: true, volume: "2" }
    );
    expect(fields.ClientOverrideVolume).toBe("0.200");
  });

  it("clamps a volume above the scale", () => {
    const fields = writeAccountLaunchOverrides(
      {},
      { ...EMPTY_ACCOUNT_LAUNCH_OVERRIDES, enabled: true, volume: "80" }
    );
    expect(fields.ClientOverrideVolume).toBe("1.000");
  });

  it("round-trips a full exception", () => {
    const original = {
      enabled: true,
      maxFps: "240",
      volume: "2",
      graphics: "auto",
      fullscreen: "true",
      startMinimized: "false",
      windowWidth: "",
      windowHeight: "",
    };
    expect(readAccountLaunchOverrides(writeAccountLaunchOverrides({}, original))).toEqual(original);
  });

  it("turning the switch off leaves the switch key out", () => {
    const fields = writeAccountLaunchOverrides(
      { ClientOverridesEnabled: "true" },
      { ...EMPTY_ACCOUNT_LAUNCH_OVERRIDES, maxFps: "240" }
    );
    expect("ClientOverridesEnabled" in fields).toBe(false);
    // O valor continua guardado: desligar não é apagar a configuração.
    expect(fields.ClientOverrideMaxFPS).toBe("240");
  });
});
