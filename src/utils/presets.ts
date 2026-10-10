/**
 * Regras puras dos presets de launch (ideia 13): o alvo que um favorito vira,
 * o rascunho de um preset novo e o texto do próximo horário. Ver
 * docs/features/presets.md.
 */
import type { LaunchPreset, PresetSchedule } from "../types";
import type { FavoriteGame } from "../components/server-list/types";
import { parsePrivateServerCode } from "./privateServerCode";

export const EMPTY_SCHEDULE: PresetSchedule = {
  openEnabled: false,
  openAt: "08:00",
  days: [0, 1, 2, 3, 4, 5, 6],
  closeEnabled: false,
  closeAt: "18:00",
};

export function newPresetDraft(userIds: number[], placeId = 0, jobId = ""): LaunchPreset {
  return {
    id: "",
    name: "",
    userIds: [...new Set(userIds)],
    placeId,
    jobId: jobId.trim(),
    gameName: null,
    vipName: null,
    arrangeGrid: false,
    schedule: null,
  };
}

/**
 * O alvo que o servidor VIP de um favorito vira: `vip:<código>`, a forma que o
 * backend (`resolve_launch_job`) entende numa fila de várias contas. Link
 * colado vira código aqui, como o `launchMultiple` do store faz.
 */
export function vipJobFromLink(link: string): string {
  const code = parsePrivateServerCode(link);
  return code ? `vip:${code}` : link.trim();
}

/** Uma opção de destino tirada dos favoritos. */
export interface PresetTargetOption {
  key: string;
  placeId: number;
  jobId: string;
  gameName: string;
  vipName: string | null;
}

/** Cada favorito vira "servidor público" + um item por VIP salvo. */
export function favoriteTargets(favorites: FavoriteGame[]): PresetTargetOption[] {
  const out: PresetTargetOption[] = [];
  for (const fav of favorites) {
    out.push({ key: `${fav.placeId}:public`, placeId: fav.placeId, jobId: "", gameName: fav.name, vipName: null });
    for (const vip of fav.vipServers ?? []) {
      if (!vip.link?.trim()) continue;
      out.push({
        key: `${fav.placeId}:${vip.id}`,
        placeId: fav.placeId,
        jobId: vipJobFromLink(vip.link),
        gameName: fav.name,
        vipName: vip.name?.trim() || null,
      });
    }
  }
  return out;
}

/** A opção dos favoritos que bate com o preset (mesmo place e mesmo servidor). */
export function matchingTarget(options: PresetTargetOption[], placeId: number, jobId: string): PresetTargetOption | null {
  return options.find((o) => o.placeId === placeId && o.jobId === jobId.trim()) ?? null;
}

/** O campo do editor em que o problema aparece. */
export type PresetField = "name" | "accounts" | "game" | "schedule";

/** "HH:MM" que o backend aceita (`parse_hhmm`): hora de 1–2 dígitos, minuto de 2. */
export function isPresetTime(text: string): boolean {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  return !!m && Number(m[1]) <= 23 && Number(m[2]) <= 59;
}

/**
 * O preset está pronto para salvar? Devolve o campo e a frase (inglês) do que
 * falta, na ordem em que o formulário se lê — o editor mostra a frase ao lado
 * do campo. As mesmas regras do `normalize_preset` do backend, e mais duas
 * que só a tela sabe: o Place ID digitado que não é número (`placeText`) e as
 * contas que já saíram do app (`knownUserIds`).
 */
export function presetFieldProblem(
  preset: LaunchPreset,
  context: { placeText?: string; knownUserIds?: Iterable<number> } = {}
): { field: PresetField; message: string } | null {
  if (!preset.name.trim()) return { field: "name", message: "Give the preset a name." };
  if (preset.userIds.length === 0) return { field: "accounts", message: "Pick at least one account." };
  if (context.knownUserIds) {
    const known = new Set(context.knownUserIds);
    if (!preset.userIds.some((id) => known.has(id))) {
      return { field: "accounts", message: "None of this preset's accounts are in the app anymore." };
    }
  }
  if (!Number.isSafeInteger(preset.placeId) || preset.placeId <= 0) {
    const typed = context.placeText?.trim();
    return { field: "game", message: typed ? "That Place ID is not valid." : "Pick a game (Place ID)." };
  }
  const s = preset.schedule;
  if (s?.openEnabled && !isPresetTime(s.openAt)) return { field: "schedule", message: "Pick a time to open." };
  if (s?.openEnabled && s.days.length === 0) return { field: "schedule", message: "Pick at least one day to open." };
  if (s?.closeEnabled && !isPresetTime(s.closeAt)) return { field: "schedule", message: "Pick a time to close." };
  return null;
}

/** O preset está pronto para salvar? Devolve a frase (inglês) do que falta. */
export function presetProblem(preset: LaunchPreset): string | null {
  return presetFieldProblem(preset)?.message ?? null;
}

/**
 * "Today 08:00" / "Tomorrow 08:00" / "Mon 08:00". `weekdayName` recebe o dia
 * no formato do JS (0 = domingo) e devolve o nome curto no idioma do app.
 */
export function formatNextRun(
  at: number,
  now: number,
  t: (text: string, options?: Record<string, unknown>) => string,
  weekdayName: (jsDay: number) => string
): string {
  const when = new Date(at);
  const today = new Date(now);
  const time = `${String(when.getHours()).padStart(2, "0")}:${String(when.getMinutes()).padStart(2, "0")}`;
  const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOfDay(when) - startOfDay(today)) / 86_400_000);
  if (days <= 0) return t("Today {{time}}", { time });
  if (days === 1) return t("Tomorrow {{time}}", { time });
  return `${weekdayName(when.getDay())} ${time}`;
}

/** Ordem de exibição dos dias (segunda primeiro) → dia no formato do JS. */
export const PRESET_DAY_ORDER = [0, 1, 2, 3, 4, 5, 6] as const;
export function presetDayToJsDay(day: number): number {
  return (day + 1) % 7;
}
