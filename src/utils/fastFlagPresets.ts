/**
 * Espelho de `WINDOWS_FASTFLAG_ALLOWLIST`
 * (`src-tauri/src/platform/windows/optimization.rs`): a lista oficial do
 * Roblox (DevForum, 29/09/2025), na mesma ordem.
 *
 * Por que espelhar em vez de expor por comando Tauri: a conferência acontece
 * enquanto o usuário digita, e a tela de Settings monta sem depender de ida ao
 * backend. O preço do espelho é pago por `fastFlagAllowlist.test.ts`, que lê o
 * próprio .rs e quebra a suíte se as duas listas divergirem.
 */
export const WINDOWS_FASTFLAG_ALLOWLIST = [
  "DFIntCSGLevelOfDetailSwitchingDistance",
  "DFIntCSGLevelOfDetailSwitchingDistanceL12",
  "DFIntCSGLevelOfDetailSwitchingDistanceL23",
  "DFIntCSGLevelOfDetailSwitchingDistanceL34",
  "FFlagHandleAltEnterFullscreenManually",
  "DFFlagTextureQualityOverrideEnabled",
  "DFIntTextureQualityOverride",
  "FIntDebugForceMSAASamples",
  "DFFlagDisableDPIScale",
  "FFlagDebugGraphicsPreferD3D11",
  "FFlagDebugSkyGray",
  "DFFlagDebugPauseVoxelizer",
  "DFIntDebugFRMQualityLevelOverride",
  "FIntFRMMaxGrassDistance",
  "FIntFRMMinGrassDistance",
  "FFlagDebugGraphicsPreferVulkan",
  "FFlagDebugGraphicsPreferOpenGL",
  "FIntGrassMovementReducedMotionFactor",
] as const;

/**
 * Padrões prontos de FastFlags para Settings › Optimization (pedido do dono,
 * 28/10/2026: "quem entende pouco clica no + e tem mais desempenho"). Só usam
 * as chaves da lista oficial do Roblox (DevForum, 29/09/2025) — qualquer outra
 * ele ignora. Os valores vão como texto ("True", "0"), como o Bloxstrap grava.
 * Ver docs/features/performance.md ("FastFlags").
 *
 * FPS não está aqui: o Roblox recusa a FastFlag de FPS (o log do cliente diz
 * "Denied local configuration for: DFIntTaskSchedulerTargetFps"); o limite de
 * FPS do app vai pelo arquivo de configurações do Roblox e continua valendo.
 */

export interface FastFlagPreset {
  id: "nuke" | "fps" | "memory";
  /** Chave de tradução (inglês). */
  name: string;
  description: string;
  flags: Record<string, string>;
}

/** O que cada chave da lista faz, em palavras simples (tooltip das chaves). */
export const FAST_FLAG_INFO: Record<string, string> = {
  DFIntCSGLevelOfDetailSwitchingDistance: "Distance where parts switch to a simpler model. 0 = simplest model always.",
  DFIntCSGLevelOfDetailSwitchingDistanceL12: "Same, for the second detail level. 0 = simplest always.",
  DFIntCSGLevelOfDetailSwitchingDistanceL23: "Same, for the third detail level. 0 = simplest always.",
  DFIntCSGLevelOfDetailSwitchingDistanceL34: "Same, for the last detail level. 0 = simplest always.",
  FFlagHandleAltEnterFullscreenManually: "Alt+Enter toggles fullscreen the old way. Not about performance.",
  DFFlagTextureQualityOverrideEnabled: "Lets the texture quality below be forced. Needs to be True.",
  DFIntTextureQualityOverride: "Texture quality: 0 = lowest (blurry, least video memory), 3 = highest.",
  FIntDebugForceMSAASamples: "Anti-aliasing (smooth edges). 0 = off, the lightest.",
  DFFlagDisableDPIScale: "Ignores Windows display scaling. Not about performance.",
  FFlagDebugGraphicsPreferD3D11: "Uses Direct3D 11. Roblox said it may remove this one.",
  FFlagDebugSkyGray: "Plain gray sky instead of the skybox.",
  DFFlagDebugPauseVoxelizer: "Stops recalculating light and shadows. Big FPS gain; lighting may look frozen.",
  DFIntDebugFRMQualityLevelOverride: "Forces the graphics quality level. 1 = lowest.",
  FIntFRMMaxGrassDistance: "How far grass is drawn. 0 = no grass.",
  FIntFRMMinGrassDistance: "Where grass starts. 0 = no grass.",
  FFlagDebugGraphicsPreferVulkan: "Uses Vulkan. Can help or crash depending on the graphics card.",
  FFlagDebugGraphicsPreferOpenGL: "Uses OpenGL. Usually slower on Windows.",
  FIntGrassMovementReducedMotionFactor: "Grass movement. 0 = still grass.",
};

export const FAST_FLAG_PRESETS: FastFlagPreset[] = [
  {
    id: "nuke",
    name: "Nuke rendering",
    description:
      "Everything as low as Roblox allows: lowest textures and quality, no anti-aliasing, no grass, simplest models, frozen lighting and a gray sky. The game looks bad and runs as light as it gets. Best for alts nobody is looking at.",
    flags: {
      DFFlagTextureQualityOverrideEnabled: "True",
      DFIntTextureQualityOverride: "0",
      DFIntDebugFRMQualityLevelOverride: "1",
      FIntDebugForceMSAASamples: "0",
      DFFlagDebugPauseVoxelizer: "True",
      DFIntCSGLevelOfDetailSwitchingDistance: "0",
      DFIntCSGLevelOfDetailSwitchingDistanceL12: "0",
      DFIntCSGLevelOfDetailSwitchingDistanceL23: "0",
      DFIntCSGLevelOfDetailSwitchingDistanceL34: "0",
      FIntFRMMinGrassDistance: "0",
      FIntFRMMaxGrassDistance: "0",
      FIntGrassMovementReducedMotionFactor: "0",
      FFlagDebugSkyGray: "True",
    },
  },
  {
    id: "fps",
    name: "More FPS",
    description:
      "Cuts what costs the most and you notice the least: anti-aliasing, grass and light recalculation. Textures and models stay as they are, so the game still looks normal.",
    flags: {
      FIntDebugForceMSAASamples: "0",
      DFFlagDebugPauseVoxelizer: "True",
      FIntFRMMinGrassDistance: "0",
      FIntFRMMaxGrassDistance: "0",
      FIntGrassMovementReducedMotionFactor: "0",
    },
  },
  {
    id: "memory",
    name: "Less video memory",
    description:
      "Lowest textures and no anti-aliasing: each client uses less video memory, which helps when many accounts run on one graphics card.",
    flags: {
      DFFlagTextureQualityOverrideEnabled: "True",
      DFIntTextureQualityOverride: "0",
      FIntDebugForceMSAASamples: "0",
    },
  },
];

function parseObject(json: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(json);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? (parsed as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

/**
 * O "+": junta o padrão ao JSON que já existe (o valor do padrão vence; as
 * outras chaves ficam). JSON vazio ou quebrado vira só o padrão.
 */
export function addPresetToJson(json: string, preset: FastFlagPreset): string {
  const current = parseObject(json.trim()) ?? {};
  return JSON.stringify({ ...current, ...preset.flags }, null, 2);
}

/** Todas as chaves do padrão já estão no JSON com o valor dele. */
export function presetIsApplied(json: string, preset: FastFlagPreset): boolean {
  const current = parseObject(json.trim());
  if (!current) return false;
  return Object.entries(preset.flags).every(([key, value]) => String(current[key]) === value);
}

/** As chaves que o Roblox ignoraria (o launch as descarta, sem travar o resto). */
export function unlistedFastFlags(json: string): string[] {
  const current = parseObject(json.trim());
  if (!current) return [];
  return Object.keys(current).filter((key) => !(WINDOWS_FASTFLAG_ALLOWLIST as readonly string[]).includes(key));
}
