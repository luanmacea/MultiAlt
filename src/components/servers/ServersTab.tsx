import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Copy, Globe, Loader2, RefreshCw, Search, Server, Wifi } from "lucide-react";
import { MAX_SERVER_SCAN_PAGES, useStore } from "../../store";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import { GameBadge } from "../ui/GameBadge";
import { useTr } from "../../i18n/text";
import type {
  ServerPreference,
  ServerRegion,
  ServerRegionProgress,
  ServerScanUpdate,
} from "../../types";
import { looksLikeJoinLink, parsePlaceIdInput } from "../server-list/types";

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

/** Quantos servidores têm a região (e a permissão) verificada por clique em "Check servers". */
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

/** Folga ideal depois que o lote entra. Espelha `FREE_SEAT_BUFFER` no backend. */
const IDEAL_SPARE_SEATS = 1;

/**
 * Nota de encaixe de um servidor para um lote — **menor é melhor**.
 *
 * A folga ideal depois das contas entrarem é **uma vaga**: o lote joga com
 * gente, e ainda sobra lugar para quem cair e voltar. A nota piora conforme se
 * afasta disso para qualquer lado — encher o servidor é ruim, e um servidor
 * quase vazio também. Empate vai para o mais cheio.
 *
 * Quem não cabe vem depois de todo mundo que cabe, ordenado por **quantas
 * contas leva**.
 */
export function fitScore(row: ServerRow, accounts: number): [number, number, number] {
  if (row.maxPlayers <= 0) return [2, 0, 0];
  const slack = freeSeats(row) - Math.max(accounts, 1);
  if (slack >= 0) return [0, Math.abs(slack - IDEAL_SPARE_SEATS), slack];
  return [1, -freeSeats(row), -row.playing];
}

/**
 * Ordena a lista pelo encaixe com o lote **desta tela**.
 *
 * O backend já manda ordenado, mas quem sabe com certeza quantas contas estão
 * selecionadas agora é a UI — é ela que escreve os rótulos ("3 free · needs
 * 6"), e as duas coisas têm que concordar. Ordenar aqui também impede que uma
 * página da varredura chegando fora de ordem deixe servidores piores no topo.
 */
/**
 * Tira servidores repetidos, mantendo o primeiro.
 *
 * A lista do Roblox se mexe entre uma página e outra, então o mesmo Job ID
 * volta em páginas diferentes (em dados reais do jogo do relato: 50 repetidos
 * em 400). Cada repetido virava uma **chave de lista duplicada** no React, que
 * então parava de reordenar a tabela — a lista "tentava" subir os servidores
 * bons e travava no meio do caminho.
 */
export function dedupeRows(rows: ServerRow[]): ServerRow[] {
  const seen = new Set<string>();
  return rows.filter((row) => {
    if (!row.id || seen.has(row.id)) return false;
    seen.add(row.id);
    return true;
  });
}

export function rankRows(rows: ServerRow[], accounts: number): ServerRow[] {
  return dedupeRows(rows).sort((a, b) => {
    const scoreA = fitScore(a, accounts);
    const scoreB = fitScore(b, accounts);
    for (let i = 0; i < scoreA.length; i++) {
      if (scoreA[i] !== scoreB[i]) return scoreA[i] - scoreB[i];
    }
    return 0;
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

/** Menuzinho de contexto do Job ID — uma ação só: copiar. */
function JobIdContextMenu({
  x,
  y,
  onCopy,
  onClose,
}: {
  x: number;
  y: number;
  onCopy: () => void;
  onClose: () => void;
}) {
  const t = useTr();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    }
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [onClose]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const pad = 8;
    const width = el.offsetWidth || 160;
    const height = el.offsetHeight || 40;
    const left = Math.max(pad, Math.min(x, window.innerWidth - width - pad));
    const top = Math.max(pad, Math.min(y, window.innerHeight - height - pad));
    setPos({ left, top });
  }, [x, y]);

  return createPortal(
    <div
      ref={ref}
      className="theme-modal-scope theme-panel theme-border fixed z-[60] rounded-xl shadow-2xl py-1 w-44 animate-scale-in"
      style={{ top: pos.top, left: pos.left }}
    >
      <button
        onClick={() => {
          onCopy();
          onClose();
        }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
      >
        <Copy size={12} strokeWidth={1.5} className="theme-muted" />
        {t("Copy Job ID")}
      </button>
    </div>,
    document.body
  );
}

/**
 * Célula do Job ID: coluna fixa (não some no meio da região vazia que
 * sobrava com `flex-1`) e copiável — clique copia direto, botão direito abre
 * o menu com a mesma ação. Antes era um `<code>` sem `onClick`, `title` nem
 * menu de contexto: o único jeito de levar o Job ID para outro lugar era
 * selecionar o texto na mão.
 */
function JobIdCell({ jobId, onCopy }: { jobId: string; onCopy: (jobId: string) => void }) {
  const t = useTr();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  return (
    <>
      <button
        type="button"
        onClick={() => onCopy(jobId)}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
        title={t("{{jobId}} — click to copy", { jobId })}
        aria-label={t("Copy Job ID")}
        // 264px é o Job ID inteiro medido na tela (259px na fonte mono de
        // 12px) com folga: a 250px de antes ele passou a ser cortado quando a
        // fonte do app subiu de 11 para 12, e um Job ID pela metade não serve
        // para nada. A barra de ocupação ao lado é flexível e cede o espaço.
        className="w-[264px] shrink-0 truncate text-left text-[12px] font-mono theme-muted hover:text-[var(--panel-fg)] transition-colors"
      >
        {jobId}
      </button>
      {menu && (
        <JobIdContextMenu
          x={menu.x}
          y={menu.y}
          onCopy={() => onCopy(jobId)}
          onClose={() => setMenu(null)}
        />
      )}
    </>
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
  /** Que jogo é o place digitado — um número de 10 dígitos não diz nada. */
  const game = useGameIdentity(placeId, accountForApi);
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

  /**
   * O que entra no campo "Place ID".
   *
   * Digitar dígitos continua igual. Colar a URL do jogo preenche o place da
   * URL — antes o campo só apagava os não-dígitos e **juntava** o resto, então
   * `.../games/606849621/Jailbreak?privateServerLinkCode=98765` virava o place
   * inventado `60684962198765`. O que não tem place (link de convite,
   * `share?code=`) é recusado com o motivo, em vez de virar número.
   */
  function handlePlaceIdInput(raw: string) {
    const text = raw.trim();
    if (/^\d*$/.test(text)) {
      setPlaceId(text);
      setError(null);
      return;
    }
    const parsed = parsePlaceIdInput(text);
    if (parsed !== null) {
      setPlaceId(String(parsed));
      setError(null);
      return;
    }
    setError(
      looksLikeJoinLink(text)
        ? t("That link has no Place ID. Use the Follow tab to join invite and private server links.")
        : t("Could not find a Place ID in that text. Paste a game link or type the ID.")
    );
  }

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

  async function handleCopyJobId(jobId: string) {
    try {
      await navigator.clipboard.writeText(jobId);
      store.addToast(t("Copied Job ID"));
    } catch {
      store.addToast(t("Failed to copy"));
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
  /**
   * Quantos dos servidores **na tela** cabem o lote. Quando o backend diz que
   * existem mas nenhum deles chegou (lista cortada), a tela avisa em vez de
   * deixar o usuário achar que o topo serve.
   */
  const shownFitting = visible.filter((row) => hasRoomFor(row, batchSize)).length;

  const fieldClass =
    "px-2.5 py-1.5 text-[12px] rounded-lg bg-[var(--panel-soft)] border theme-border text-[var(--panel-fg)] outline-none focus:border-[var(--accent-color)] transition-colors";
  const buttonClass =
    "flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-lg theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-40 transition-colors";

  return (
    <div className="h-full flex flex-col min-h-0 px-4 pt-3 pb-4 gap-3">
      {/* ── Controles ── */}
      <div className="shrink-0 flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1">
          <span className="text-[11px] theme-muted">{t("Place ID")}</span>
          <div className="relative">
            <Search
              size={12}
              strokeWidth={1.5}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 theme-muted pointer-events-none"
            />
            <input
              value={placeId}
              onChange={(e) => handlePlaceIdInput(e.target.value)}
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
          <span className="text-[11px] theme-muted">{t("Sort by")}</span>
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
          <span className="text-[11px] theme-muted">{t("Region")}</span>
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
          <span className="text-[11px] theme-muted">{t("Pages to scan")}</span>
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

        {/* A consulta da região é um pedido de join àquele servidor, e a
            resposta já diz se a conta pode entrar — por isso o botão verifica
            as duas coisas. */}
        <button
          onClick={() => void loadRegions()}
          disabled={regionBusy || !rows?.length}
          title={t("Loads each server's region and whether the first selected account can join it — one check per server.")}
          className={buttonClass}
        >
          {regionBusy ? <Loader2 size={12} className="animate-spin" /> : <Globe size={12} strokeWidth={1.5} />}
          {regionBusy && regionProgress
            ? t("Checking servers ({{done}}/{{total}})", {
                done: regionProgress.done,
                total: regionProgress.total,
              })
            : t("Check servers")}
        </button>

        <button onClick={() => void loadServers()} disabled={loading} className={`${buttonClass} ml-auto`}>
          {loading ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} strokeWidth={1.5} />}
          {t("Refresh")}
        </button>
      </div>

      {/* ── Que jogo é este place ── */}
      {game && (game.name || game.iconUrl) && (
        <div className="shrink-0 flex items-center gap-2 -mt-1">
          <GameBadge
            name={game.name}
            iconUrl={game.iconUrl}
            placeId={game.placeId}
            className="max-w-[320px]"
          />
        </div>
      )}

      {/* ── Resumo do que está na tela ── */}
      {scan && (
        <p className="shrink-0 flex items-center gap-1.5 text-[12px] theme-muted">
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
          {scan.fitting > 0 && shownFitting === 0 && (
            <span className="text-amber-400/90">
              {t("The ones that fit are not in this page yet — refresh to fetch them.")}{" "}
            </span>
          )}
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
        {error && <p className="text-[12px] text-red-400 p-4">{error}</p>}

        {!error && rows === null && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10 px-6 text-center">
            <Server size={22} strokeWidth={1.5} />
            <p className="text-[12px] max-w-[42ch]">
              {t("Enter a Place ID to list its servers, or open a game from the Games tab.")}
            </p>
          </div>
        )}

        {!error && rows !== null && visible.length === 0 && loading && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10">
            <Loader2 size={18} className="animate-spin" />
            <p className="text-[12px]">{t("Looking for servers...")}</p>
          </div>
        )}
        {!error && rows !== null && visible.length === 0 && !loading && (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10 px-6 text-center">
            <Server size={22} strokeWidth={1.5} />
            <p className="text-[12px] max-w-[42ch]">
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
                      <span className="text-[12px] tabular-nums theme-muted">
                        / {row.maxPlayers}
                      </span>
                      <span
                        className={`ml-auto text-[11px] tabular-nums ${
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

                  {/* Região — flex-1: era a coluna de tamanho fixo (150px) que
                      truncava nomes de cidade enquanto o Job ID sobrava vazio
                      ao lado. Agora quem sobra é ela. */}
                  <div className="flex-1 min-w-0 truncate text-[12px]">
                    {region?.denied ? (
                      // Recusa do Roblox para a conta da consulta (erro 524): é
                      // o que o usuário só descobria tentando entrar.
                      <span
                        title={region.error ?? undefined}
                        className="inline-flex items-center rounded-md border border-red-500/30 bg-red-500/10 px-1.5 py-0.5 text-[11px] font-medium text-red-300"
                      >
                        {t("No permission")}
                      </span>
                    ) : region ? (
                      region.label ? (
                        <span className="text-[var(--panel-fg)]">{region.label}</span>
                      ) : (
                        <span className="theme-muted">{region.error}</span>
                      )
                    ) : (
                      <span className="theme-muted">—</span>
                    )}
                  </div>

                  {/* Job ID — coluna fixa e copiável (era `flex-1`, um
                      `<code>` sem onClick/title/menu, com ~290px de sobra
                      vazia num Job ID real de ~238px). */}
                  <JobIdCell jobId={row.id} onCopy={handleCopyJobId} />

                  {/* Ping */}
                  <div className="w-[64px] shrink-0 flex items-center justify-end gap-1 text-[12px] tabular-nums theme-muted">
                    <Wifi size={11} strokeWidth={1.5} />
                    {row.ping ?? "—"}
                  </div>

                  <button
                    onClick={() => void handleJoin(row)}
                    disabled={!room || joining !== null || region?.denied === true}
                    title={
                      region?.denied
                        ? t("The first selected account does not have permission to join this server")
                        : room
                          ? undefined
                          : t("Not enough room for {{count}} accounts", { count: batchSize })
                    }
                    className="shrink-0 px-3 py-1 rounded-lg text-[12px] theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-30 transition-colors"
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
