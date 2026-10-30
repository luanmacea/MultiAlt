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

/**
 * Controles simples (como o Bloxstrap e o Fishstrap): um menu por coisa, que
 * grava as chaves dele no mesmo JSON do perfil. "Automatic" tira as chaves (o
 * Roblox decide). Um valor digitado à mão que não é de nenhuma opção aparece
 * como "Custom" e só muda se a pessoa escolher outra opção.
 */
export interface FastFlagControlOption {
  id: string;
  /** Chave de tradução (inglês). */
  label: string;
  flags: Record<string, string>;
}

export interface FastFlagControl {
  id: "textures" | "msaa" | "quality" | "models" | "grass" | "lighting" | "sky" | "api";
  label: string;
  description: string;
  /**
   * Para que lado fica mais leve, em poucas palavras (pedido do dono,
   * 28/10/2026: "quanto menor, mais rápido"). Aparece ao lado do nome.
   */
  hint: string;
  /** As chaves que este controle é dono (as que "Automatic" tira). */
  keys: string[];
  options: FastFlagControlOption[];
}

const CSG_KEYS = [
  "DFIntCSGLevelOfDetailSwitchingDistance",
  "DFIntCSGLevelOfDetailSwitchingDistanceL12",
  "DFIntCSGLevelOfDetailSwitchingDistanceL23",
  "DFIntCSGLevelOfDetailSwitchingDistanceL34",
];

const csg = (values: number[]): Record<string, string> =>
  Object.fromEntries(CSG_KEYS.map((key, i) => [key, String(values[i])]));

export const FAST_FLAG_CONTROLS: FastFlagControl[] = [
  {
    id: "textures",
    hint: "Lower = lighter",
    label: "Texture quality",
    description: "Lower textures look blurry but use much less video memory.",
    keys: ["DFFlagTextureQualityOverrideEnabled", "DFIntTextureQualityOverride"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      ...[
        ["lowest", "Lowest", "0"],
        ["low", "Low", "1"],
        ["medium", "Medium", "2"],
        ["high", "High", "3"],
      ].map(([id, label, level]) => ({
        id,
        label,
        flags: { DFFlagTextureQualityOverrideEnabled: "True", DFIntTextureQualityOverride: level },
      })),
    ],
  },
  {
    id: "msaa",
    hint: "Off = lightest",
    label: "Anti-aliasing",
    description: "Smooths the edges of objects. Off is the lightest.",
    keys: ["FIntDebugForceMSAASamples"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      { id: "off", label: "Off", flags: { FIntDebugForceMSAASamples: "0" } },
      { id: "1", label: "1x", flags: { FIntDebugForceMSAASamples: "1" } },
      { id: "2", label: "2x", flags: { FIntDebugForceMSAASamples: "2" } },
      { id: "4", label: "4x", flags: { FIntDebugForceMSAASamples: "4" } },
    ],
  },
  {
    id: "quality",
    hint: "Lower = faster",
    label: "Graphics quality",
    description: "Forces the graphics level, from 1 (lowest) to 21 (highest), whatever the in-game slider says.",
    keys: ["DFIntDebugFRMQualityLevelOverride"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      ...["1", "3", "5", "10", "15", "21"].map((level) => ({
        id: level,
        label: level === "1" ? "1 (lowest)" : level === "21" ? "21 (highest)" : level,
        flags: { DFIntDebugFRMQualityLevelOverride: level },
      })),
    ],
  },
  {
    id: "models",
    hint: "Lower = faster",
    label: "Model detail",
    description: "How soon parts switch to a simpler model. Lowest uses the simplest model everywhere.",
    keys: CSG_KEYS,
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      { id: "lowest", label: "Lowest", flags: csg([0, 0, 0, 0]) },
      // Os níveis do Fishstrap: (clamp(q - i, 0, 3) + 1) * 250.
      { id: "low", label: "Low", flags: csg([250, 250, 250, 250]) },
      { id: "medium", label: "Medium", flags: csg([750, 500, 250, 250]) },
      { id: "high", label: "High", flags: csg([1000, 750, 500, 250]) },
    ],
  },
  {
    id: "grass",
    hint: "Off = faster",
    label: "Grass",
    description: "Turning grass off saves a lot in open maps.",
    keys: ["FIntFRMMinGrassDistance", "FIntFRMMaxGrassDistance", "FIntGrassMovementReducedMotionFactor"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      {
        id: "off",
        label: "Off",
        flags: { FIntFRMMinGrassDistance: "0", FIntFRMMaxGrassDistance: "0", FIntGrassMovementReducedMotionFactor: "0" },
      },
      { id: "still", label: "Still (no movement)", flags: { FIntGrassMovementReducedMotionFactor: "0" } },
    ],
  },
  {
    id: "lighting",
    hint: "Paused = faster",
    label: "Lighting updates",
    description: "Paused stops recalculating light and shadows: big FPS gain, but lighting may look frozen.",
    keys: ["DFFlagDebugPauseVoxelizer"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      { id: "paused", label: "Paused", flags: { DFFlagDebugPauseVoxelizer: "True" } },
    ],
  },
  {
    id: "sky",
    hint: "Gray = a bit lighter",
    label: "Sky",
    description: "A plain gray sky instead of the game's skybox.",
    keys: ["FFlagDebugSkyGray"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      { id: "gray", label: "Gray", flags: { FFlagDebugSkyGray: "True" } },
    ],
  },
  {
    id: "api",
    hint: "Depends on your PC",
    label: "Graphics API",
    description: "Leave it on Automatic unless Roblox runs badly. Vulkan can help or close the game depending on the graphics card.",
    keys: ["FFlagDebugGraphicsPreferD3D11", "FFlagDebugGraphicsPreferVulkan", "FFlagDebugGraphicsPreferOpenGL"],
    options: [
      { id: "auto", label: "Automatic", flags: {} },
      { id: "d3d11", label: "Direct3D 11", flags: { FFlagDebugGraphicsPreferD3D11: "True" } },
      { id: "vulkan", label: "Vulkan", flags: { FFlagDebugGraphicsPreferVulkan: "True" } },
      { id: "opengl", label: "OpenGL", flags: { FFlagDebugGraphicsPreferOpenGL: "True" } },
    ],
  },
];

/** Qual opção do controle está no JSON: "auto" sem nenhuma chave dele, "custom" se não bate com nenhuma. */
export function readControl(json: string, control: FastFlagControl): string {
  const current = parseObject(json.trim()) ?? {};
  const present = control.keys.filter((key) => key in current);
  if (present.length === 0) return "auto";
  const match = control.options.find(
    (option) =>
      Object.keys(option.flags).length === present.length &&
      Object.entries(option.flags).every(([key, value]) => key in current && String(current[key]) === value)
  );
  return match?.id ?? "custom";
}

/** Grava a opção: tira as chaves do controle e põe as da opção; as outras chaves do JSON ficam. */
export function writeControl(json: string, control: FastFlagControl, optionId: string): string {
  const current = parseObject(json.trim()) ?? {};
  const option = control.options.find((o) => o.id === optionId);
  if (!option) return JSON.stringify(current, null, 2);
  const next: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(current)) {
    if (!control.keys.includes(key)) next[key] = value;
  }
  Object.assign(next, option.flags);
  return JSON.stringify(next, null, 2);
}
