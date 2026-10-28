import { describe, expect, it } from "vitest";
import { WINDOWS_FASTFLAG_ALLOWLIST } from "../components/settings/OptimizationTab";
import { FAST_FLAG_INFO, FAST_FLAG_PRESETS, addPresetToJson, presetIsApplied, unlistedFastFlags } from "./fastFlagPresets";

describe("fast flag presets", () => {
  it("only use flags Roblox still reads", () => {
    for (const preset of FAST_FLAG_PRESETS) {
      for (const key of Object.keys(preset.flags)) {
        expect(WINDOWS_FASTFLAG_ALLOWLIST, `${preset.id}: ${key}`).toContain(key);
      }
    }
  });

  it("explain every flag on the list in plain words", () => {
    for (const key of WINDOWS_FASTFLAG_ALLOWLIST) {
      expect(FAST_FLAG_INFO[key]?.trim(), key).toBeTruthy();
    }
  });

  it("nuke rendering is the lightest and turns everything down", () => {
    const nuke = FAST_FLAG_PRESETS.find((p) => p.id === "nuke")!;
    expect(nuke.flags.DFIntTextureQualityOverride).toBe("0");
    expect(nuke.flags.FIntDebugForceMSAASamples).toBe("0");
    expect(nuke.flags.DFIntDebugFRMQualityLevelOverride).toBe("1");
    expect(nuke.flags.FFlagDebugSkyGray).toBe("True");
    for (const preset of FAST_FLAG_PRESETS) {
      expect(Object.keys(nuke.flags).length).toBeGreaterThanOrEqual(Object.keys(preset.flags).length);
    }
  });

  it("the + adds the preset on top of what is there, keeping the other keys", () => {
    const more = FAST_FLAG_PRESETS.find((p) => p.id === "fps")!;
    const out = addPresetToJson('{ "FFlagHandleAltEnterFullscreenManually": "False", "FIntDebugForceMSAASamples": "4" }', more);
    const parsed = JSON.parse(out);
    expect(parsed.FFlagHandleAltEnterFullscreenManually).toBe("False");
    expect(parsed.FIntDebugForceMSAASamples).toBe("0");
    expect(presetIsApplied(out, more)).toBe(true);
    // Vazio ou quebrado: começa do zero com o padrão.
    expect(JSON.parse(addPresetToJson("", more))).toEqual(more.flags);
    expect(JSON.parse(addPresetToJson("{oops", more))).toEqual(more.flags);
    expect(presetIsApplied("", more)).toBe(false);
  });

  it("lists the keys Roblox would ignore without blocking the rest", () => {
    expect(unlistedFastFlags('{"FFlagDebugSkyGray":"True","DFIntTaskSchedulerTargetFps":"999"}')).toEqual([
      "DFIntTaskSchedulerTargetFps",
    ]);
    expect(unlistedFastFlags("{oops")).toEqual([]);
  });
});
