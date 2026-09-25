import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Globe, Loader2, RefreshCw, Server, Users } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import type {
  ServerPreference,
  ServerRegion,
  ServerRegionProgress,
} from "../../types";

/**
 * Aba "Servers" da Choose Game.
 *
 * Mostra os servidores públicos do place e deixa o usuário **escolher em qual
 * entrar** — todas as contas selecionadas vão para o servidor clicado.
 *
 * Regras de produto que não podem se perder:
 *
 * - O launch passa SEMPRE por `launchAll` (`launch_multiple`), nunca por um
 *   laço de `launch_roblox`: o piso anti-captcha de 8 s entre contas é do
 *   backend.
 * - A região **não** vem da lista de servidores (a API não devolve isso). Cada
 *   região custa um `join-game-instance`, então ela é resolvida sob demanda, em
 *   lotes pequenos, e nunca automaticamente ao abrir a aba.
 * - Servidor sem vaga para o lote inteiro aparece, mas não é clicável: mandar
 *   8 contas para um servidor com 2 vagas espalharia 6 delas.
 */

/** Quantos servidores têm a região resolvida por clique em "Load regions". */
const REGION_BATCH = 10;

export type LaunchAllFn = (
  userIds: number[],
  placeId: number,
  jobId?: string,
  onStarted?: () => void
) => Promise<{ ok: boolean; error?: string }>;

/** Um servidor como a API devolve (`games/v1/games/{place}/servers/Public`). */
export interface ServerRow {
  id: string;
  playing: number;
  maxPlayers: number;
  fps?: number;
  ping?: number | null;
}

interface ServersResponse {
  data: ServerRow[];
  nextPageCursor: string | null;
}

/**
 * Ordena a lista conforme a preferência, espelhando `rank_servers` do backend.
 *
 * `default` mantém a ordem que a API devolveu (menos jogadores primeiro), para
 * a aba não mentir sobre o que o launch faria.
 */
export function sortServers(rows: ServerRow[], preference: ServerPreference): ServerRow[] {
  const sorted = [...rows];
  if (preference === "fullest") sorted.sort((a, b) => b.playing - a.playing);
  else if (preference === "emptiest") sorted.sort((a, b) => a.playing - b.playing);
  return sorted;
}

/** O servidor cabe o lote inteiro? É a mesma regra do backend. */
export function hasRoomFor(row: ServerRow, accounts: number): boolean {
  if (row.maxPlayers <= 0) return false;
  return row.playing + Math.max(accounts, 1) <= row.maxPlayers;
}

/**
 * Filtra por país. Servidor sem região resolvida **fica na lista** enquanto o
 * filtro está ligado, marcado como desconhecido — escondê-lo daria a impressão
 * falsa de que não existe servidor naquele país.
 */
export function matchesRegion(
  row: ServerRow,
  regions: Map<string, ServerRegion>,
  countryCode: string
): boolean {
  const wanted = countryCode.trim().toUpperCase();
  if (!wanted) return true;
  const resolved = regions.get(row.id);
  if (!resolved || !resolved.region) return true;
  return resolved.region.countryCode.toUpperCase() === wanted;
}

const PREFERENCES: { id: ServerPreference; label: string }[] = [
  { id: "default", label: "Default" },
  { id: "random", label: "Random" },
  { id: "emptiest", label: "Emptiest" },
  { id: "fullest", label: "Fullest" },
];

export interface ServersTabProps {
  /** Contas selecionadas — todas entram no servidor clicado. */
  userIds: number[];
  placeId: string;
  setPlaceId: (placeId: string) => void;
  launchAll: LaunchAllFn;
  onGoToConsole?: () => void;
}

export function ServersTab({
  userIds,
  placeId,
  setPlaceId,
  launchAll,
  onGoToConsole,
}: ServersTabProps) {
  const t = useTr();
  const store = useStore();

  const [rows, setRows] = useState<ServerRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [regions, setRegions] = useState<Map<string, ServerRegion>>(new Map());
  const [regionBusy, setRegionBusy] = useState(false);
  const [regionProgress, setRegionProgress] = useState<ServerRegionProgress | null>(null);
  const [joining, setJoining] = useState<string | null>(null);

  const accountForApi = userIds[0] ?? null;
  const preference = store.serverPreference;
  const regionFilter = store.serverRegionFilter;
  const placeIdRef = useRef(placeId);
  placeIdRef.current = placeId;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    listen<ServerRegionProgress>("server-region-progress", (event) => {
      setRegionProgress(event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const loadServers = useCallback(async () => {
    const place = parseInt(placeIdRef.current, 10);
    if (!place || place <= 0) {
      setError(t("Enter a valid Place ID"));
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const response = await invoke<ServersResponse>("get_servers", {
        placeId: place,
        serverType: "Public",
        cursor: null,
        userId: accountForApi,
      });
      setRows(response.data || []);
      // Os Job IDs mudam a cada refresh; regiões antigas não valem mais.
      setRegions(new Map());
    } catch (e) {
      setError(String(e));
      setRows([]);
    } finally {
      setLoading(false);
    }
  }, [accountForApi, t]);

  // Carrega sozinho quando a aba abre já com um place escolhido.
  useEffect(() => {
    if (rows === null && parseInt(placeId, 10) > 0) void loadServers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placeId]);

  const visible = useMemo(() => {
    const list = sortServers(rows || [], preference);
    return list.filter((row) => matchesRegion(row, regions, regionFilter));
  }, [rows, preference, regions, regionFilter]);

  /**
   * Resolve a região dos próximos servidores ainda sem região. Em lote pequeno
   * porque cada um custa uma chamada de join.
   */
  const loadRegions = useCallback(async () => {
    const place = parseInt(placeIdRef.current, 10);
    if (!place || !accountForApi) {
      store.addToast(t("Select an account to load regions"));
      return;
    }
    const pending = visible.filter((row) => !regions.has(row.id)).slice(0, REGION_BATCH);
    if (pending.length === 0) {
      store.addToast(t("All listed servers already have a region"));
      return;
    }

    setRegionBusy(true);
    setRegionProgress({ done: 0, total: pending.length });
    try {
      const resolved = await invoke<ServerRegion[]>("get_server_regions", {
        userId: accountForApi,
        placeId: place,
        jobIds: pending.map((row) => row.id),
      });
      setRegions((prev) => {
        const next = new Map(prev);
        for (const entry of resolved) next.set(entry.jobId, entry);
        return next;
      });
    } catch (e) {
      store.addToast(t("Failed to load regions: {{error}}", { error: String(e) }));
    } finally {
      setRegionBusy(false);
      setRegionProgress(null);
    }
  }, [accountForApi, regions, store, t, visible]);

  async function handleJoin(row: ServerRow) {
    const place = parseInt(placeIdRef.current, 10);
    if (!place) return;
    setJoining(row.id);
    try {
      await launchAll(userIds, place, row.id, onGoToConsole);
    } finally {
      setJoining(null);
    }
  }

  /** Países já resolvidos, para o filtro oferecer só o que existe na lista. */
  const availableCountries = useMemo(() => {
    const codes = new Set<string>();
    for (const entry of regions.values()) {
      const code = entry.region?.countryCode;
      if (code) codes.add(code.toUpperCase());
    }
    return [...codes].sort();
  }, [regions]);

  const regionOptions = useMemo(() => {
    const codes = new Set(availableCountries);
    if (regionFilter) codes.add(regionFilter);
    return [...codes].sort();
  }, [availableCountries, regionFilter]);

  return (
    <div className="h-full flex flex-col min-h-0 gap-3">
      {/* ── Controles ── */}
      <div className="shrink-0 flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1">
          <span className="text-[10px] theme-muted">{t("Place ID")}</span>
          <input
            value={placeId}
            onChange={(e) => setPlaceId(e.target.value.replace(/[^0-9]/g, ""))}
            onKeyDown={(e) => {
              if (e.key === "Enter") void loadServers();
            }}
            placeholder="606849621"
            aria-label={t("Place ID")}
            className="w-[150px] px-2.5 py-1.5 text-[12px] rounded-md bg-[var(--panel-soft)] border theme-border text-[var(--panel-fg)] outline-none focus:border-[var(--accent-color)]"
          />
        </label>

        <button
          onClick={() => void loadServers()}
          disabled={loading}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] rounded-md theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-50"
        >
          {loading ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
          {t("Refresh")}
        </button>

        <label className="flex flex-col gap-1">
          <span className="text-[10px] theme-muted">{t("Server preference")}</span>
          <select
            value={preference}
            onChange={(e) => store.setServerPreference(e.target.value as ServerPreference)}
            aria-label={t("Server preference")}
            className="px-2.5 py-1.5 text-[12px] rounded-md bg-[var(--panel-soft)] border theme-border text-[var(--panel-fg)] outline-none"
          >
            {PREFERENCES.map((option) => (
              <option key={option.id} value={option.id}>
                {t(option.label)}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-[10px] theme-muted">{t("Region")}</span>
          <select
            value={regionFilter}
            onChange={(e) => store.setServerRegionFilter(e.target.value)}
            aria-label={t("Region")}
            className="px-2.5 py-1.5 text-[12px] rounded-md bg-[var(--panel-soft)] border theme-border text-[var(--panel-fg)] outline-none"
          >
            <option value="">{t("Any region")}</option>
            {regionOptions.map((code) => (
              <option key={code} value={code}>
                {code}
              </option>
            ))}
            {!regionOptions.includes("BR") && <option value="BR">BR</option>}
          </select>
        </label>

        <button
          onClick={() => void loadRegions()}
          disabled={regionBusy || !rows?.length}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] rounded-md theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-50"
        >
          {regionBusy ? <Loader2 size={12} className="animate-spin" /> : <Globe size={12} />}
          {regionBusy && regionProgress
            ? t("Loading regions ({{done}}/{{total}})", {
                done: regionProgress.done,
                total: regionProgress.total,
              })
            : t("Load regions")}
        </button>
      </div>

      <p className="shrink-0 text-[10px] theme-muted">
        {t(
          "Region needs one join request per server, so it is loaded in small batches — {{count}} at a time.",
          { count: REGION_BATCH }
        )}
      </p>

      {/* ── Lista ── */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {error && (
          <p className="text-[11px] text-red-400 py-2">{error}</p>
        )}
        {!error && rows === null && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10">
            <Server size={22} strokeWidth={1.5} />
            <p className="text-[11px]">{t("Enter a Place ID to list its servers.")}</p>
          </div>
        )}
        {!error && rows !== null && visible.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10">
            <Server size={22} strokeWidth={1.5} />
            <p className="text-[11px]">
              {regionFilter
                ? t("No server matched {{region}}. Load more regions or clear the filter.", {
                    region: regionFilter,
                  })
                : t("No public server was found for this place.")}
            </p>
          </div>
        )}

        {visible.length > 0 && (
          <table className="w-full text-[11px]">
            <thead className="theme-muted">
              <tr className="text-left">
                <th className="py-1.5 px-2 font-medium">{t("Players")}</th>
                <th className="py-1.5 px-2 font-medium">{t("Region")}</th>
                <th className="py-1.5 px-2 font-medium">{t("Job ID")}</th>
                <th className="py-1.5 px-2 font-medium text-right">{t("Ping")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {visible.map((row) => {
                const room = hasRoomFor(row, userIds.length);
                const region = regions.get(row.id);
                return (
                  <tr
                    key={row.id}
                    className={`border-t theme-border ${room ? "" : "opacity-50"}`}
                  >
                    <td className="py-1.5 px-2 tabular-nums text-[var(--panel-fg)]">
                      <span className="inline-flex items-center gap-1">
                        <Users size={11} strokeWidth={1.5} className="theme-muted" />
                        {row.playing}/{row.maxPlayers}
                      </span>
                    </td>
                    <td className="py-1.5 px-2 theme-muted">
                      {region ? region.label || region.error || "—" : "—"}
                    </td>
                    <td className="py-1.5 px-2 theme-muted font-mono truncate max-w-[220px]">
                      {row.id}
                    </td>
                    <td className="py-1.5 px-2 text-right tabular-nums theme-muted">
                      {row.ping ?? "—"}
                    </td>
                    <td className="py-1.5 px-2 text-right">
                      <button
                        onClick={() => void handleJoin(row)}
                        disabled={!room || joining !== null}
                        title={
                          room
                            ? undefined
                            : t("Not enough room for {{count}} accounts", { count: userIds.length })
                        }
                        className="px-2 py-1 rounded-md text-[11px] theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-40"
                      >
                        {joining === row.id ? t("Joining...") : t("Join")}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
