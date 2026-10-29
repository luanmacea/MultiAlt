import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Snowflake } from "lucide-react";
import { useTr } from "../../i18n/text";
import { ChipMenu, type ChipMenuOption } from "./ChipMenu";

/**
 * Freeze (docs/features/freeze.md): congela o cliente das contas marcadas — o
 * jogo para de desenhar e de gastar CPU/GPU e volta de onde estava. Quem
 * congela é o backend (commands/freeze.rs); aqui ficam o botão ao lado do
 * "AFK Mode", o selo da linha e o relógio de quem tem tempo.
 */

export interface FrozenClient {
  userId: number;
  sinceMs: number;
  untilMs: number | null;
}

interface FreezeReport {
  frozen: number[];
  failed: [number, string][];
}

/** "Até soltar" e os tempos do menu, em segundos (0 = até soltar). */
export const FREEZE_DURATIONS = [0, 10, 30, 60, 300] as const;

export function freezeDurationLabel(seconds: number, t: ReturnType<typeof useTr>): string {
  if (seconds === 0) return t("Until I unfreeze");
  if (seconds < 60) return t("{{count}} seconds", { count: seconds });
  return t("{{count}} minutes", { count: seconds / 60 });
}

/** Quanto falta, "12s" / "4m 05s"; vazio quando é até soltar. */
export function freezeRemaining(untilMs: number | null, nowMs: number): string {
  if (untilMs == null) return "";
  const left = Math.max(0, Math.ceil((untilMs - nowMs) / 1000));
  if (left < 60) return `${left}s`;
  const m = Math.floor(left / 60);
  const s = left % 60;
  return `${m}m ${String(s).padStart(2, "0")}s`;
}

/** As contas congeladas, sempre em dia com o backend (evento `freeze-changed`). */
export function useFrozenClients() {
  const [frozen, setFrozen] = useState<Map<number, FrozenClient>>(() => new Map());
  const [now, setNow] = useState(() => Date.now());

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<FrozenClient[]>("frozen_clients");
      setFrozen(new Map(list.map((f) => [f.userId, f])));
    } catch {
      // Sem backend (teste) ou app fechando: fica como estava.
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("freeze-changed", () => void refresh());
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  // O relógio só anda enquanto alguém tem tempo para acabar.
  const timed = [...frozen.values()].some((f) => f.untilMs != null);
  useEffect(() => {
    if (!timed) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [timed]);

  const freeze = useCallback(
    async (userIds: number[], seconds: number) => {
      const report = await invoke<FreezeReport>("freeze_clients", {
        userIds,
        seconds: seconds > 0 ? seconds : null,
      });
      await refresh();
      return report;
    },
    [refresh]
  );

  const unfreeze = useCallback(
    async (userIds: number[]) => {
      await invoke<number>("unfreeze_clients", { userIds });
      await refresh();
    },
    [refresh]
  );

  return { frozen, now, freeze, unfreeze };
}

/**
 * O botão do cabeçalho "Em jogo": congela as contas-alvo (as marcadas, ou
 * todas) pelo tempo escolhido no menu; com todas já congeladas, vira
 * "Unfreeze" e solta com um clique.
 */
export function FreezeButton({
  targets,
  frozen,
  onFreeze,
  onUnfreeze,
  disabled = false,
}: {
  targets: number[];
  frozen: Map<number, FrozenClient>;
  onFreeze: (seconds: number) => void;
  onUnfreeze: () => void;
  disabled?: boolean;
}) {
  const t = useTr();
  const allFrozen = targets.length > 0 && targets.every((id) => frozen.has(id));
  if (allFrozen) {
    return (
      <button
        onClick={onUnfreeze}
        disabled={disabled}
        title={t("Lets these clients run again")}
        data-testid="freeze-button"
        className="sidebar-btn-sm flex items-center gap-1.5 shrink-0 !text-sky-400 disabled:opacity-40"
      >
        <Snowflake size={13} strokeWidth={1.5} aria-hidden="true" />
        {t("Unfreeze ({{count}})", { count: targets.length })}
      </button>
    );
  }
  const options: ChipMenuOption[] = FREEZE_DURATIONS.map((seconds, i) => ({
    value: String(seconds),
    label: freezeDurationLabel(seconds, t),
    separated: i === 1,
  }));
  return (
    <ChipMenu
      ariaLabel={t("Freeze")}
      title={t(
        "Freezes the game of these accounts: it stops drawing and using CPU and GPU, stays connected, and goes on from where it was when unfrozen."
      )}
      menuLabel={t("Freeze for how long")}
      options={options}
      onSelect={(raw) => onFreeze(Number(raw))}
      disabled={disabled || targets.length === 0}
      testId="freeze-button"
      className="!h-[26px] !px-2 !text-[12px]"
    >
      <Snowflake size={13} strokeWidth={1.5} aria-hidden="true" className="text-sky-400" />
      {t("Freeze ({{count}})", { count: targets.length })}
    </ChipMenu>
  );
}

/** O selo na linha da conta congelada: clique solta. */
export function FrozenChip({
  name,
  entry,
  now,
  onUnfreeze,
}: {
  name: string;
  entry: FrozenClient;
  now: number;
  onUnfreeze: () => void;
}) {
  const t = useTr();
  const left = freezeRemaining(entry.untilMs, now);
  return (
    <button
      onClick={onUnfreeze}
      title={t("{{name}} is frozen. Click to unfreeze.", { name })}
      data-testid="frozen-chip"
      className="session-row-btn !text-sky-400"
    >
      <Snowflake size={12} strokeWidth={1.5} aria-hidden="true" />
      {left ? t("Frozen · {{left}}", { left }) : t("Frozen")}
    </button>
  );
}
