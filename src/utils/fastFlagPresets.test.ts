import { describe, expect, it } from "vitest";
import { WINDOWS_FASTFLAG_ALLOWLIST } from "../components/settings/OptimizationTab";
import {
  FAST_FLAG_CONTROLS,
  FAST_FLAG_INFO,
  FAST_FLAG_PRESETS,
  addPresetToJson,
  presetIsApplied,
  readControl,
  unlistedFastFlags,
  writeControl,
} from "./fastFlagPresets";

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

  it("simple controls only write flags Roblox still reads", () => {
    for (const control of FAST_FLAG_CONTROLS) {
      for (const key of control.keys) {
        expect(WINDOWS_FASTFLAG_ALLOWLIST, `${control.id}: ${key}`).toContain(key);
      }
      expect(control.options[0].id, control.id).toBe("auto");
    }
  });

  /** Como o Bloxstrap e o Fishstrap: um menu por coisa, sem digitar JSON (28/10/2026). */
  it("a simple control writes its keys, reads them back and Automatic removes them", () => {
    const textures = FAST_FLAG_CONTROLS.find((c) => c.id === "textures")!;
    let json = writeControl('{"FFlagDebugSkyGray":"True"}', textures, "lowest");
    expect(JSON.parse(json)).toEqual({
      FFlagDebugSkyGray: "True",
      DFFlagTextureQualityOverrideEnabled: "True",
      DFIntTextureQualityOverride: "0",
    });
    expect(readControl(json, textures)).toBe("lowest");
    json = writeControl(json, textures, "auto");
    expect(JSON.parse(json)).toEqual({ FFlagDebugSkyGray: "True" });
    expect(readControl(json, textures)).toBe("auto");
    // Valor digitado à mão que não é de nenhuma opção.
    expect(readControl('{"DFIntTextureQualityOverride":"7"}', textures)).toBe("custom");
  });

  it("the graphics API keeps only one of Direct3D, Vulkan and OpenGL", () => {
    const api = FAST_FLAG_CONTROLS.find((c) => c.id === "api")!;
    let json = writeControl("", api, "vulkan");
    json = writeControl(json, api, "opengl");
    expect(JSON.parse(json)).toEqual({ FFlagDebugGraphicsPreferOpenGL: "True" });
  });

  it("the Nuke rendering profile reads as the lowest option on every control it touches", () => {
    const nuke = FAST_FLAG_PRESETS.find((p) => p.id === "nuke")!;
    const json = addPresetToJson("", nuke);
    const read = (id: string) => readControl(json, FAST_FLAG_CONTROLS.find((c) => c.id === id)!);
    expect(read("textures")).toBe("lowest");
    expect(read("msaa")).toBe("off");
    expect(read("quality")).toBe("1");
    expect(read("models")).toBe("lowest");
    expect(read("grass")).toBe("off");
    expect(read("lighting")).toBe("paused");
    expect(read("sky")).toBe("gray");
    expect(read("api")).toBe("auto");
  });

  it("lists the keys Roblox would ignore without blocking the rest", () => {
    expect(unlistedFastFlags('{"FFlagDebugSkyGray":"True","DFIntTaskSchedulerTargetFps":"999"}')).toEqual([
      "DFIntTaskSchedulerTargetFps",
    ]);
    expect(unlistedFastFlags("{oops")).toEqual([]);
  });
});
