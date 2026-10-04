import { useCallback, useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { REPO_URL } from "../../repo";
import {
  cachedReleaseHistory,
  compareVersions,
  fetchReleaseHistory,
  ReleaseHistoryError,
  type ReleaseEntry,
} from "../../releaseNotes";
import { ReleaseNotesMarkdown } from "../ReleaseNotesMarkdown";
import { PageShell } from "./PageShell";

/**
 * Página "What's new" (Novidades): o que cada atualização mudou, da mais nova
 * para a mais antiga, com a versão instalada marcada.
 *
 * As versões vêm das releases do GitHub (`fetchReleaseHistory`), **só quando a
 * página abre** — o GitHub dá 60 pedidos por hora sem login, e abrir o app não
 * pode gastar nenhum. A lista fica guardada na sessão; uma falha não fica, e o
 * "Tentar de novo" pede outra vez. Ver docs/features/ui-layout.md.
 */

type LoadState =
  | { kind: "loading" }
  | { kind: "ready"; entries: ReleaseEntry[] }
  | { kind: "error"; reason: ReleaseHistoryError["kind"] };

const RELEASES_URL = `${REPO_URL}/releases`;

/** Locale de data por idioma do app: "October 3, 2026", "3 de outubro de 2026". */
const DATE_LOCALES: Record<string, string> = { en: "en-US", pt: "pt-BR", es: "es-419", de: "de-DE" };

function formatDate(iso: string, language: string): string {
  const time = Date.parse(iso);
  if (!Number.isFinite(time)) return "";
  const locale = DATE_LOCALES[language.slice(0, 2)] ?? language;
  try {
    return new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(new Date(time));
  } catch {
    return new Date(time).toDateString();
  }
}

function initialState(): LoadState {
  const cached = cachedReleaseHistory();
  return cached ? { kind: "ready", entries: cached } : { kind: "loading" };
}

export function ChangelogPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  if (!active) return null;
  return <ChangelogPageBody onLeave={onLeave} />;
}

function ChangelogPageBody({ onLeave }: { onLeave: () => void }) {
  const t = useTr();
  const { i18n } = useTranslation();
  const store = useStore();
  const [state, setState] = useState<LoadState>(initialState);
  const [attempt, setAttempt] = useState(0);
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void getVersion()
      .then((version: unknown) => {
        if (!cancelled && typeof version === "string" && version) setCurrentVersion(version);
      })
      .catch(() => {
        // Sem versão, nada fica marcado — a lista continua útil.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    if (!cachedReleaseHistory()) setState({ kind: "loading" });
    fetchReleaseHistory().then(
      (entries) => {
        if (!cancelled) setState({ kind: "ready", entries });
      },
      (error: unknown) => {
        if (cancelled) return;
        setState({
          kind: "error",
          reason: error instanceof ReleaseHistoryError ? error.kind : "unavailable",
        });
      }
    );
    return () => {
      cancelled = true;
    };
  }, [attempt]);

  const retry = useCallback(() => setAttempt((n) => n + 1), []);
  const openOnGithub = useCallback(() => {
    window.open(RELEASES_URL, "_blank");
  }, []);

  const githubLink = (
    <button
      type="button"
      onClick={openOnGithub}
      className="text-[12px] text-[var(--panel-muted)] underline decoration-[var(--border-color)] underline-offset-[3px] hover:text-[var(--panel-fg)] hover:decoration-current transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] rounded-sm"
    >
      {t("See every version on GitHub")}
    </button>
  );

  return (
    <PageShell
      title={t("What's new")}
      description={t("What changed in each update of MultiAlt, newest first.")}
      onLeave={onLeave}
      dataTour="changelog-page"
      tour="changelog"
    >
      <div className="@container mx-auto w-full max-w-[880px]">
        {state.kind === "loading" ? <ChangelogSkeleton label={t("Loading the update history…")} /> : null}

        {state.kind === "error" ? (
          <div
            role="alert"
            className="max-w-[60ch] rounded-xl border theme-border bg-[var(--panel-soft)] px-4 py-3.5"
          >
            <p className="text-[13px] leading-relaxed text-[var(--panel-fg)]">
              {state.reason === "rate-limited"
                ? t("GitHub is limiting how often the update history can be loaded. Try again in a few minutes.")
                : t("Couldn't load the update history. Check your internet connection and try again.")}
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
              <button
                type="button"
                onClick={retry}
                className="theme-btn inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium"
              >
                <RefreshCw size={13} strokeWidth={1.8} aria-hidden="true" />
                {t("Try again")}
              </button>
              {githubLink}
            </div>
          </div>
        ) : null}

        {state.kind === "ready" && state.entries.length === 0 ? (
          <div className="max-w-[60ch]">
            <p className="text-[13px] text-[var(--panel-fg)]">{t("No updates to show yet.")}</p>
            <div className="mt-2">{githubLink}</div>
          </div>
        ) : null}

        {state.kind === "ready" && state.entries.length > 0 ? (
          <>
            <Timeline
              entries={state.entries}
              currentVersion={currentVersion}
              language={i18n.language}
              hasUpdateInfo={!!store.updateInfo}
              onOpenUpdate={() => store.setUpdateDialogOpen(true)}
              onCheckForUpdates={() => void store.checkForUpdates(true)}
            />
            <div className="mt-2 @xl:pl-[calc(10.5rem+1.5rem+2rem)] pl-[calc(1.25rem+0.75rem)]">{githubLink}</div>
          </>
        ) : null}
      </div>
    </PageShell>
  );
}

interface TimelineProps {
  entries: ReleaseEntry[];
  currentVersion: string | null;
  language: string;
  hasUpdateInfo: boolean;
  onOpenUpdate: () => void;
  onCheckForUpdates: () => void;
}

/**
 * Linha do tempo: à esquerda a versão e a data (alinhadas ao trilho), no meio
 * o trilho com um ponto por versão, à direita o que mudou. Em área estreita a
 * versão sobe para cima da lista e o trilho fica na beirada.
 *
 * A única cor forte da página é a da versão instalada: ponto cheio e "Sua
 * versão". Versão mais nova que a instalada tem ponto vazado e "Ainda não
 * instalada"; a mais nova delas leva ao fluxo de atualização que já existe.
 */
function Timeline({ entries, currentVersion, language, hasUpdateInfo, onOpenUpdate, onCheckForUpdates }: TimelineProps) {
  const t = useTr();
  const currentIndex = currentVersion ? entries.findIndex((e) => compareVersions(e.version, currentVersion) === 0) : -1;
  const isNewer = (entry: ReleaseEntry) => !!currentVersion && compareVersions(entry.version, currentVersion) > 0;
  // A lista vem da mais nova para a mais antiga: a primeira mais nova é a do topo.
  const updateIndex = entries.findIndex(isNewer);

  return (
    <ol aria-label={t("Versions")} data-tour="changelog-list" className="relative">
      {entries.map((entry, index) => {
        const isCurrent = index === currentIndex;
        const newer = isNewer(entry);
        const last = index === entries.length - 1;
        const date = formatDate(entry.publishedAt, language);
        return (
          <li
            key={entry.tag}
            aria-current={isCurrent ? "true" : undefined}
            data-tour={isCurrent ? "changelog-current" : undefined}
            className="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-x-3 @xl:grid-cols-[10.5rem_1.5rem_minmax(0,1fr)] @xl:gap-x-4"
          >
            {/* Trilho: a linha liga este ponto ao da próxima versão. */}
            <div aria-hidden="true" className="relative row-span-2 @xl:row-span-1 @xl:col-start-2 @xl:row-start-1">
              {last ? null : (
                <span className="absolute left-1/2 top-[0.95rem] bottom-0 w-px -translate-x-1/2 bg-[var(--border-color)]" />
              )}
              <span
                className={`absolute left-1/2 top-[0.3rem] -translate-x-1/2 rounded-full ${
                  isCurrent
                    ? "h-3 w-3 bg-[var(--accent-color)] shadow-[0_0_0_4px_var(--accent-soft)]"
                    : newer
                      ? "h-2.5 w-2.5 border-[1.5px] border-dashed border-[var(--panel-muted)] bg-[var(--panel-bg)] mt-px"
                      : "h-2 w-2 bg-[var(--border-color)] mt-[2px]"
                }`}
              />
            </div>

            {/* Os dois lados levam o respiro de baixo: a linha tem a altura do mais alto. */}
            <header
              className={`col-start-2 @xl:col-start-1 @xl:row-start-1 @xl:text-right min-w-0 ${last ? "@xl:pb-4" : "@xl:pb-8"}`}
            >
              <h2
                className={`text-[15px] leading-6 font-semibold tabular-nums tracking-tight ${
                  isCurrent ? "text-[var(--accent-color)]" : "text-[var(--panel-fg)]"
                }`}
              >
                v{entry.version}
              </h2>
              <div className="flex flex-wrap items-center gap-x-2 gap-y-1 @xl:justify-end">
                {date ? (
                  <time dateTime={entry.publishedAt} className="text-[12px] leading-5 text-[var(--panel-muted)]">
                    {date}
                  </time>
                ) : null}
                {isCurrent ? (
                  <span className="rounded-full bg-[var(--accent-soft)] px-2 py-px text-[11px] font-medium leading-[18px] text-[var(--accent-color)]">
                    {t("Your version")}
                  </span>
                ) : null}
                {newer ? (
                  <span className="text-[11.5px] leading-5 text-[var(--panel-muted)] italic">{t("Not installed yet")}</span>
                ) : null}
              </div>
              {index === updateIndex ? (
                <button
                  type="button"
                  onClick={hasUpdateInfo ? onOpenUpdate : onCheckForUpdates}
                  data-tour="changelog-update"
                  className="theme-btn mt-2 inline-flex items-center whitespace-nowrap px-2.5 py-1 text-[12px] font-medium"
                >
                  {hasUpdateInfo ? t("Update available") : t("Check for Updates")}
                </button>
              ) : null}
            </header>

            <div className={`col-start-2 @xl:col-start-3 @xl:row-start-1 min-w-0 ${last ? "pb-4" : "pb-8"} pt-2 @xl:pt-0.5`}>
              <ReleaseNotesMarkdown
                source={entry.notes}
                className={`max-w-[68ch] text-[13px] leading-[1.65] ${
                  isCurrent || newer ? "text-[var(--panel-fg)]" : "text-[var(--panel-fg)]/85"
                } [&_ul]:my-0 [&_ul]:space-y-1.5 [&_p]:my-1`}
              />
            </div>
          </li>
        );
      })}
    </ol>
  );
}

function ChangelogSkeleton({ label }: { label: string }) {
  return (
    <div role="status" aria-live="polite">
      <span className="sr-only">{label}</span>
      <div aria-hidden="true" className="space-y-9 animate-pulse motion-reduce:animate-none">
        {[3, 2, 4].map((lines, row) => (
          <div
            key={row}
            className="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-x-3 @xl:grid-cols-[10.5rem_1.5rem_minmax(0,1fr)] @xl:gap-x-4"
          >
            <div className="relative row-span-2 @xl:row-span-1 @xl:col-start-2 @xl:row-start-1">
              <span className="absolute left-1/2 top-[0.4rem] h-2 w-2 -translate-x-1/2 rounded-full bg-[var(--border-color)]" />
            </div>
            <div className="col-start-2 @xl:col-start-1 @xl:row-start-1 flex flex-col gap-2 @xl:items-end">
              <span className="h-4 w-16 rounded bg-[var(--panel-soft)]" />
              <span className="h-3 w-24 rounded bg-[var(--panel-soft)]" />
            </div>
            <div className="col-start-2 @xl:col-start-3 @xl:row-start-1 mt-3 @xl:mt-0.5 space-y-2.5">
              {Array.from({ length: lines }, (_, i) => (
                <span
                  key={i}
                  className="block h-3 rounded bg-[var(--panel-soft)]"
                  style={{ width: `${[78, 64, 70, 52][i % 4]}%` }}
                />
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
