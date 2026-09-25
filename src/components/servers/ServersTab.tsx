import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Globe, Loader2, RefreshCw, Search, Server, Wifi } from "lucide-react";
import { MAX_SERVER_SCAN_PAGES, useStore } from "../../store";
import { useTr } from "../../i18n/text";
import type {
  ServerPreference,
  ServerRegion,
  ServerRegionProgress,
  ServerScanUpdate,
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
 * - A **ordem é pedida à API**, não refeita aqui: a resposta traz no máximo 100
 *   servidores e um jogo grande tem milhares, então reordenar a página local
 *   mostraria "o mais cheio entre os mais vazios".
 * - A região **não** vem da lista de servidores (a API não devolve isso). Cada
 *   região custa um `join-game-instance`, então ela é resolvida sob demanda, em
 *   lotes pequenos, e nunca automaticamente ao abrir a aba.
 * - Servidor sem vaga para o lote inteiro aparece, mas não é clicável: mandar
 *   8 contas para um servidor com 2 vagas espalharia 6 delas.
 *
 * Desenho: a informação que decide a escolha é a **ocupação** — quantas vagas
 * sobram e se o lote cabe. Por isso cada linha é uma barra de ocupação com as
 * vagas do lote desenhadas nela, e não uma célula de texto "3/13".
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

/** Vagas que sobram no servidor. */
function freeSeats(row: ServerRow): number {
  return Math.max(row.maxPlayers - row.playing, 0);
}

/**
 * Ordena a lista pelo encaixe com o lote **desta tela**.
 *
 * O backend já manda a lista ordenada, mas quem sabe com certeza quantas contas
 * estão selecionadas agora é a UI: o topo tem que ser sempre o melhor encaixe
 * para o número que aparece nos rótulos ("3 free · needs 6"). Ordenar aqui
 * também evita que uma página antiga da varredura, chegando fora de ordem,
 * deixe servidores piores no topo.
 *
 * Ordem:
 *
 * 1. cabe o lote **com uma vaga de folga**, do mais cheio para o mais vazio;
 * 2. cabe o lote sem folga (encheria o servidor), do mais cheio para o mais
 *    vazio;
 * 3. não cabe: quem leva **mais contas** primeiro e, entre iguais, o mais
 *    cheio — servidor vivo vale mais que servidor vazio do mesmo tamanho.
 */
export function rankRows(rows: ServerRow[], accounts: number): ServerRow[] {
  const needed = Math.max(accounts, 1);
  function tier(row: ServerRow): number {
    if (row.maxPlayers <= 0) return 3;
    const free = freeSeats(row);
    if (free >= needed + 1) return 0;
    if (free >= needed) return 1;
    return 2;
  }

  return [...rows].sort((a, b) => {
    const tierDiff = tier(a) - tier(b);
    if (tierDiff !== 0) return tierDiff;
    // Dentro de "não cabe", mais vagas primeiro: é quem leva mais contas.
    if (tier(a) === 2) {
      const freeDiff = freeSeats(b) - freeSeats(a);
      if (freeDiff !== 0) return freeDiff;
    }
    return b.playing - a.playing;
  });
}

const PREFERENCES: { id: ServerPreference; label: string }[] = [
  { id: "bestfit", label: "Best fit" },
  { id: "fullest", label: "Fullest" },
  { id: "emptiest", label: "Emptiest" },
  { id: "random", label: "Random" },
  { id: "none", label: "Let Roblox choose" },
];

/**
 * Barra de ocupação: jogadores presentes, as vagas que o lote vai ocupar e o
 * espaço que sobra. É o elemento que decide a escolha, então é o único com
 * peso visual na linha.
 */
function OccupancyBar({
  playing,
  maxPlayers,
  incoming,
  fits,
}: {
  playing: number;
  maxPlayers: number;
  incoming: number;
  fits: boolean;
}) {
  const total = Math.max(maxPlayers, 1);
  const takenPct = Math.min(100, (playing / total) * 100);
  const incomingPct = fits ? Math.min(100 - takenPct, (incoming / total) * 100) : 0;

  return (
    <div
      className="h-1.5 w-full rounded-full overflow-hidden flex bg-[var(--panel-muted)]"
      role="presentation"
    >
      <div
        className={fits ? "bg-[var(--panel-fg)]/45" : "bg-[var(--panel-fg)]/25"}
        style={{ width: `${takenPct}%` }}
      />
      <div
        className="bg-[var(--accent-color)]"
        style={{ width: `${incomingPct}%` }}
      />
    </div>
  );
}

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
  const [scan, setScan] = useState<
    { scanned: number; fitting: number; done: boolean; stoppedAtLimit: boolean } | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const [regions, setRegions] = useState<Map<string, ServerRegion>>(new Map());
  const [regionBusy, setRegionBusy] = useState(false);
  const [regionProgress, setRegionProgress] = useState<ServerRegionProgress | null>(null);
  const [joining, setJoining] = useState<string | null>(null);

  const accountForApi = userIds[0] ?? null;
  const preference = store.serverPreference;
  const regionFilter = store.serverRegionFilter;
  const scanPages = store.serverScanPages;
  const placeIdRef = useRef(placeId);
  placeIdRef.current = placeId;

  const scanIdRef = useRef<number | null>(null);

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

  /**
   * A varredura publica uma página por vez.
   *
   * O corte é pelo **maior** `scanId` já visto, e não pelo id que o
   * `start_server_scan` devolveu: duas varreduras seguidas resolvem o `invoke`
   * fora de ordem, e comparar com o id "atual" podia descartar justamente os
   * eventos da varredura nova, deixando na tela a primeira página da antiga.
   */
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    listen<ServerScanUpdate>("server-scan", (event) => {
      const update = event.payload;
      if (update.scanId < (scanIdRef.current ?? 0)) return;
      scanIdRef.current = update.scanId;
      setRows(update.servers || []);
      setScan({
        scanned: update.scanned,
        fitting: update.fitting,
        done: update.done,
        stoppedAtLimit: update.stoppedAtLimit,
      });
      setError(update.error ?? null);
      if (update.done) setLoading(false);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
      void invoke("stop_server_scan").catch(() => {});
    };
  }, []);

  /**
   * Começa a varredura do place. A lista chega por evento, página a página, já
   * ordenada pelo backend — num jogo grande as primeiras páginas podem não ter
   * nenhum servidor que caiba o lote, e esperar o fim deixaria a tela vazia.
   */
  const loadServers = useCallback(async () => {
    const place = parseInt(placeIdRef.current, 10);
    if (!place || place <= 0) {
      setError(t("Enter a valid Place ID"));
      return;
    }
    setLoading(true);
    setError(null);
    setScan(null);
    setRows(null);
    // Os Job IDs mudam a cada varredura; regiões antigas não valem mais.
    setRegions(new Map());
    try {
      const started = await invoke<number>("start_server_scan", {
        placeId: place,
        userId: accountForApi,
        preference,
        accounts: Math.max(userIds.length, 1),
        maxPages: scanPages,
      });
      scanIdRef.current = Math.max(scanIdRef.current ?? 0, started);
    } catch (e) {
      setError(String(e));
      setRows([]);
      setLoading(false);
    }
  }, [accountForApi, preference, scanPages, t, userIds.length]);

  // Recomeça ao abrir com um place escolhido e sempre que a ordem, o lote ou a
  // profundidade da varredura mudam.
  useEffect(() => {
    if (parseInt(placeId, 10) > 0) void loadServers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placeId, preference, userIds.length, scanPages]);

  const visible = useMemo(
    () =>
      rankRows(
        (rows || []).filter((row) => matchesRegion(row, regions, regionFilter)),
        Math.max(userIds.length, 1)
      ),
    [rows, regions, regionFilter, userIds.length]
  );

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
  const regionOptions = useMemo(() => {
    const codes = new Set<string>();
    for (const entry of regions.values()) {
      const code = entry.region?.countryCode;
      if (code) codes.add(code.toUpperCase());
    }
    if (regionFilter) codes.add(regionFilter);
    codes.add("BR");
    return [...codes].sort();
  }, [regions, regionFilter]);

  const batchSize = Math.max(userIds.length, 1);
  const pendingRegions = visible.filter((row) => !regions.has(row.id)).length;

  const fieldClass =
    "px-2.5 py-1.5 text-[12px] rounded-lg bg-[var(--panel-soft)] border theme-border text-[var(--panel-fg)] outline-none focus:border-[var(--accent-color)] transition-colors";
  const buttonClass =
    "flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-lg theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-40 transition-colors";

  return (
    <div className="h-full flex flex-col min-h-0 px-4 pt-3 pb-4 gap-3">
      {/* ── Controles ── */}
      <div className="shrink-0 flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1">
          <span className="text-[10px] theme-muted">{t("Place ID")}</span>
          <div className="relative">
            <Search
              size={12}
              strokeWidth={1.5}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 theme-muted pointer-events-none"
            />
            <input
              value={placeId}
              onChange={(e) => setPlaceId(e.target.value.replace(/[^0-9]/g, ""))}
              onKeyDown={(e) => {
                if (e.key === "Enter") void loadServers();
              }}
              placeholder="606849621"
              aria-label={t("Place ID")}
              className={`${fieldClass} w-[168px] pl-7 font-mono`}
            />
          </div>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-[10px] theme-muted">{t("Sort by")}</span>
          <select
            value={preference}
            onChange={(e) => store.setServerPreference(e.target.value as ServerPreference)}
            aria-label={t("Server preference")}
            className={fieldClass}
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
            className={fieldClass}
          >
            <option value="">{t("Any region")}</option>
            {regionOptions.map((code) => (
              <option key={code} value={code}>
                {code}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-[10px] theme-muted">{t("Pages to scan")}</span>
          <input
            type="number"
            min={1}
            max={MAX_SERVER_SCAN_PAGES}
            value={scanPages}
            onChange={(e) => store.setServerScanPages(Number(e.target.value))}
            aria-label={t("Pages to scan")}
            title={t("Each page is 100 servers. Raise it to keep looking in a big game.")}
            className={`${fieldClass} w-[86px] tabular-nums`}
          />
        </label>

        <button onClick={() => void loadRegions()} disabled={regionBusy || !rows?.length} className={buttonClass}>
          {regionBusy ? <Loader2 size={12} className="animate-spin" /> : <Globe size={12} strokeWidth={1.5} />}
          {regionBusy && regionProgress
            ? t("Loading regions ({{done}}/{{total}})", {
                done: regionProgress.done,
                total: regionProgress.total,
              })
            : t("Load regions")}
        </button>

        <button onClick={() => void loadServers()} disabled={loading} className={`${buttonClass} ml-auto`}>
          {loading ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} strokeWidth={1.5} />}
          {t("Refresh")}
        </button>
      </div>

      {/* ── Resumo do que está na tela ── */}
      {scan && (
        <p className="shrink-0 flex items-center gap-1.5 text-[11px] theme-muted">
          {!scan.done && <Loader2 size={11} className="animate-spin" />}
          {scan.fitting > 0
            ? t("{{fitting}} of {{scanned}} servers fit your {{accounts}} account(s).", {
                fitting: scan.fitting,
                scanned: scan.scanned,
                accounts: batchSize,
              })
            : t("None of the {{scanned}} servers fit all {{accounts}} accounts — the best partial fits are on top.", {
                scanned: scan.scanned,
                accounts: batchSize,
              })}{" "}
          {!scan.done && t("Still looking...")}
          {scan.done && !scan.stoppedAtLimit && pendingRegions > 0 &&
            t("{{pending}} still without a region — loading it costs one join request each.", {
              pending: pendingRegions,
            })}
          {scan.done && scan.stoppedAtLimit && (
            <>
              {t("Stopped after {{pages}} pages — this game has more servers.", { pages: scanPages })}{" "}
              <button
                onClick={() => store.setServerScanPages(Math.min(scanPages * 2, MAX_SERVER_SCAN_PAGES))}
                className="underline underline-offset-2 hover:text-[var(--panel-fg)]"
              >
                {t("Scan {{pages}} pages", {
                  pages: Math.min(scanPages * 2, MAX_SERVER_SCAN_PAGES),
                })}
              </button>
            </>
          )}
        </p>
      )}

      {/* ── Lista ── */}
      <div className="flex-1 min-h-0 overflow-y-auto rounded-xl border theme-border theme-surface">
        {error && <p className="text-[11px] text-red-400 p-4">{error}</p>}

        {!error && rows === null && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10 px-6 text-center">
            <Server size={22} strokeWidth={1.5} />
            <p className="text-[11px] max-w-[42ch]">
              {t("Enter a Place ID to list its servers, or open a game from the Games tab.")}
            </p>
          </div>
        )}

        {!error && rows !== null && visible.length === 0 && loading && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10">
            <Loader2 size={18} className="animate-spin" />
            <p className="text-[11px]">{t("Looking for servers...")}</p>
          </div>
        )}
        {!error && rows !== null && visible.length === 0 && !loading && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10 px-6 text-center">
            <Server size={22} strokeWidth={1.5} />
            <p className="text-[11px] max-w-[42ch]">
              {regionFilter
                ? t("No server matched {{region}}. Load more regions or clear the filter.", {
                    region: regionFilter,
                  })
                : t("No public server was found for this place.")}
            </p>
          </div>
        )}

        {visible.length > 0 && (
          <ul className="divide-y divide-[var(--panel-border)]">
            {visible.map((row) => {
              const room = hasRoomFor(row, batchSize);
              const region = regions.get(row.id);
              const free = Math.max(row.maxPlayers - row.playing, 0);
              return (
                <li
                  key={row.id}
                  className="flex items-center gap-4 px-3.5 py-2.5 hover:bg-[var(--panel-soft)] transition-colors"
                >
                  {/* Ocupação: o número que decide a escolha */}
                  <div className="w-[164px] shrink-0">
                    <div className="flex items-baseline gap-1.5">
                      <span className="text-[15px] tabular-nums text-[var(--panel-fg)]">
                        {row.playing}
                      </span>
                      <span className="text-[11px] tabular-nums theme-muted">
                        / {row.maxPlayers}
                      </span>
                      <span
                        className={`ml-auto text-[10px] tabular-nums ${
                          room ? "theme-muted" : "text-amber-400/80"
                        }`}
                      >
                        {room
                          ? t("{{free}} free", { free })
                          : t("{{free}} free · needs {{needed}}", { free, needed: batchSize })}
                      </span>
                    </div>
                    <div className="mt-1.5">
                      <OccupancyBar
                        playing={row.playing}
                        maxPlayers={row.maxPlayers}
                        incoming={batchSize}
                        fits={room}
                      />
                    </div>
                  </div>

                  {/* Região */}
                  <div className="w-[150px] shrink-0 text-[11px]">
                    {region ? (
                      region.label ? (
                        <span className="text-[var(--panel-fg)]">{region.label}</span>
                      ) : (
                        <span className="theme-muted">{region.error}</span>
                      )
                    ) : (
                      <span className="theme-muted">—</span>
                    )}
                  </div>

                  {/* Job ID */}
                  <code className="flex-1 min-w-0 truncate text-[11px] font-mono theme-muted">
                    {row.id}
                  </code>

                  {/* Ping */}
                  <div className="w-[64px] shrink-0 flex items-center justify-end gap-1 text-[11px] tabular-nums theme-muted">
                    <Wifi size={11} strokeWidth={1.5} />
                    {row.ping ?? "—"}
                  </div>

                  <button
                    onClick={() => void handleJoin(row)}
                    disabled={!room || joining !== null}
                    title={
                      room
                        ? undefined
                        : t("Not enough room for {{count}} accounts", { count: batchSize })
                    }
                    className="shrink-0 px-3 py-1 rounded-lg text-[11px] theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-30 transition-colors"
                  >
                    {joining === row.id ? t("Joining...") : t("Join")}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
