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

/** O preset está pronto para salvar? Devolve a frase (inglês) do que falta. */
export function presetProblem(preset: LaunchPreset): string | null {
  if (!preset.name.trim()) return "Give the preset a name.";
  if (preset.userIds.length === 0) return "Pick at least one account.";
  if (!Number.isSafeInteger(preset.placeId) || preset.placeId <= 0) return "Pick a game (Place ID).";
  const s = preset.schedule;
  if (s?.openEnabled && s.days.length === 0) return "Pick at least one day to open.";
  return null;
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
