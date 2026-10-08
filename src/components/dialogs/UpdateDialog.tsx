import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Sparkles, X } from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useTr } from "../../i18n/text";
import { getUpdaterSkipVersionKey } from "../../updaterChannels";
import { REPO_API_URL, REPO_URL } from "../../repo";
import { UPDATE_HANDOFF_KEY, writeUpdateHandoff } from "../../updateHandoff";
import { notesForUpdateDialog, releaseKindOf, type ReleaseKind } from "../../releaseNotes";
import { renderReleaseNotes } from "../ReleaseNotesMarkdown";
import { ReleaseKindBadge } from "../ReleaseKindBadge";

// Mora em releaseNotes.ts (dividida com a página de novidades); o export fica
// para quem já importava daqui.
export { notesForUpdateDialog };

type Phase = "available" | "downloading" | "ready" | "installing" | "error";

/**
 * Quanto a tela "Instalando" fica à vista antes de o app fechar. A instalação
 * roda sem janela (`installMode: quiet`), então este é o último quadro que o
 * usuário vê da versão antiga — curto, mas legível.
 */
export const INSTALL_HANDOFF_DELAY_MS = 1500;

interface DownloadProgress {
  downloaded: number;
  total: number | null;
  speed: number;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function toTagName(version: string): string {
  const trimmed = version.trim();
  return trimmed.startsWith("v") ? trimmed : `v${trimmed}`;
}

function buildCommitMessagesSection(
  currentVersion: string,
  nextVersion: string,
  commits: Array<{ sha: string; message: string }>
): string {
  const lines = commits
    .map((commit) => {
      const sha = String(commit.sha || "").trim();
      const shortSha = sha.slice(0, 7);
      const subject = String(commit.message || "").split(/\r?\n/, 1)[0].trim();
      if (!sha || !subject) return "";
      return `- ${subject} ([${shortSha}](${REPO_URL}/commit/${sha}))`;
    })
    .filter(Boolean);

  if (lines.length === 0) return "";

  return [
    "## All Commit Messages",
    `Range: ${currentVersion}...${nextVersion}`,
    "",
    ...lines,
  ].join("\n");
}

export function UpdateDialog() {
  const t = useTr();
  const store = useStore();
  const open = store.updateDialogOpen;
  const info = store.updateInfo;
  const { visible, closing, handleClose } = useModalClose(open, () =>
    store.setUpdateDialogOpen(false)
  );

  const [phase, setPhase] = useState<Phase>("available");
  const [progress, setProgress] = useState<DownloadProgress>({ downloaded: 0, total: null, speed: 0 });
  const [errorMsg, setErrorMsg] = useState("");
  const [releaseNotes, setReleaseNotes] = useState<string | null>(null);
  // Tipo da release (correção, novidades, geral), pela marca do texto dela.
  const [releaseKind, setReleaseKind] = useState<ReleaseKind | null>(null);
  const renderedNotes = useMemo(() => (releaseNotes ? renderReleaseNotes(releaseNotes) : null), [releaseNotes]);

  useEffect(() => {
    if (!open) {
      setPhase("available");
      setProgress({ downloaded: 0, total: null, speed: 0 });
      setErrorMsg("");
      setReleaseNotes(null);
      setReleaseKind(null);
      return;
    }

    const nextNotes = info?.body?.trim() ? notesForUpdateDialog(info.body) : null;
    setReleaseNotes(nextNotes);
    const manifestKind = info?.body ? releaseKindOf(info.body) : null;
    setReleaseKind(manifestKind);

    if (!info?.version || !info?.currentVersion) return;

    const releaseTag = toTagName(info.version);
    const currentTag = toTagName(info.currentVersion);

    const hasDetailedSections = !!nextNotes
      && /(^|\n)##\s+What's Changed\b/i.test(nextNotes)
      && /(^|\n)##\s+Contributors\b/i.test(nextNotes);
    const hasAllCommitMessages = !!nextNotes
      && /(^|\n)##\s+All Commit Messages\b/i.test(nextNotes);

    const controller = new AbortController();

    (async () => {
      let resolvedNotes = nextNotes || "";

      if (!hasDetailedSections) {
        try {
          const releaseResponse = await fetch(
            `${REPO_API_URL}/releases/tags/${releaseTag}`,
            {
              signal: controller.signal,
              headers: {
                Accept: "application/vnd.github+json",
              },
            }
          );

          if (releaseResponse.ok) {
            const releaseData = await releaseResponse.json();
            const remoteBody = typeof releaseData?.body === "string" ? releaseData.body.trim() : "";
            if (remoteBody) {
              resolvedNotes = remoteBody;
              // O texto final da release (passo "Finalize release notes") vence
              // o do manifesto, que sai antes dele — menos quando só o do
              // manifesto traz a marca (aí o palpite pela lista não a desfaz).
              const remoteKind =
                manifestKind && !/<!--\s*release-kind:/i.test(remoteBody)
                  ? manifestKind
                  : (releaseKindOf(remoteBody) ?? manifestKind);
              if (!controller.signal.aborted) setReleaseKind(remoteKind);
            }
          }
        } catch {}
      }

      if (!hasAllCommitMessages && currentTag !== releaseTag) {
        try {
          const compareResponse = await fetch(
            `${REPO_API_URL}/compare/${encodeURIComponent(currentTag)}...${encodeURIComponent(releaseTag)}`,
            {
              signal: controller.signal,
              headers: {
                Accept: "application/vnd.github+json",
              },
            }
          );

          if (compareResponse.ok) {
            const compareData = await compareResponse.json();
            const commits = Array.isArray(compareData?.commits)
              ? compareData.commits.map((entry: any) => ({
                  sha: String(entry?.sha || ""),
                  message: String(entry?.commit?.message || ""),
                }))
              : [];

            const commitSection = buildCommitMessagesSection(currentTag, releaseTag, commits);
            if (commitSection) {
              resolvedNotes = resolvedNotes.trim()
                ? `${resolvedNotes.trim()}\n\n${commitSection}`
                : commitSection;
            }
          }
        } catch {}
      }

      if (!controller.signal.aborted) {
        const finalNotes = notesForUpdateDialog(resolvedNotes).trim();
        if (finalNotes) {
          setReleaseNotes(finalNotes);
        }
      }
    })();

    return () => {
      controller.abort();
    };
  }, [open, info?.body, info?.currentVersion, info?.version]);

  // O backend avisa o download aos poucos (`update-download-progress`). A
  // velocidade sai da diferença entre dois avisos.
  const lastSampleRef = useRef<{ at: number; downloaded: number } | null>(null);
  useEffect(() => {
    if (phase !== "downloading") return;
    lastSampleRef.current = null;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen<{ downloaded: number; total: number | null }>("update-download-progress", (event) => {
      const { downloaded, total } = event.payload;
      const now = performance.now();
      const last = lastSampleRef.current;
      const elapsed = last ? (now - last.at) / 1000 : 0;
      setProgress((prev) => ({
        downloaded,
        total: total ?? null,
        speed: last && elapsed > 0 ? Math.max(0, (downloaded - last.downloaded) / elapsed) : prev.speed,
      }));
      lastSampleRef.current = { at: now, downloaded };
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [phase]);

  const startDownload = useCallback(async () => {
    setPhase("downloading");
    setProgress({ downloaded: 0, total: null, speed: 0 });

    try {
      await invoke("download_selected_update");
      setProgress((prev) => {
        const size = prev.total ?? prev.downloaded;
        return { downloaded: size, total: size > 0 ? size : null, speed: 0 };
      });
      setPhase("ready");
    } catch (e) {
      setPhase("error");
      setErrorMsg(String(e));
    }
  }, []);

  const installAndRestart = useCallback(async () => {
    if (!info) return;
    setPhase("installing");
    writeUpdateHandoff(localStorage, info.currentVersion, info.version, Date.now());
    await new Promise((resolve) => setTimeout(resolve, INSTALL_HANDOFF_DELAY_MS));
    try {
      // Daqui o app fecha: o instalador roda sem janela e abre a versão nova.
      await invoke("install_selected_update");
    } catch (e) {
      try {
        localStorage.removeItem(UPDATE_HANDOFF_KEY);
      } catch {
        // sem armazenamento, sem anotação para desfazer
      }
      setPhase("error");
      setErrorMsg(String(e));
    }
  }, [info]);

  // Troca de edição pedida pela pessoa ("Get the complete edition"): o diálogo
  // baixa e instala sozinho, com o progresso aqui — nada fora do app. A
  // assinatura é conferida pelo updater como em qualquer atualização.
  const autoInstall = open && info?.autoInstall === true;
  const autoStartedRef = useRef(false);
  useEffect(() => {
    if (!open) {
      autoStartedRef.current = false;
      return;
    }
    if (autoInstall && phase === "available" && !autoStartedRef.current) {
      autoStartedRef.current = true;
      void startDownload();
    }
  }, [open, autoInstall, phase, startDownload]);
  useEffect(() => {
    if (autoInstall && phase === "ready") void installAndRestart();
  }, [autoInstall, phase, installAndRestart]);

  const skipVersion = useCallback(() => {
    if (info) {
      localStorage.setItem(
        getUpdaterSkipVersionKey(info.releaseChannel, info.featureChannel),
        info.version
      );
    }
    handleClose();
  }, [info, handleClose]);

  // Baixando ou instalando, o fundo não fecha o diálogo.
  const backdropClose = useBackdropClose(
    phase === "downloading" || phase === "installing" ? undefined : handleClose
  );

  if (!visible || !info) return null;

  const pct = progress.total ? Math.round((progress.downloaded / progress.total) * 100) : 0;

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      {...backdropClose}
    >
      <div
        className={`theme-panel theme-border rounded-xl w-full max-w-lg mx-4 shadow-2xl flex flex-col max-h-[calc(100vh-24px)] overflow-y-auto ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3 px-5 pt-5 pb-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-[var(--panel-fg)]">
              {phase === "installing"
                ? t("Updating MultiAlt")
                : info.autoInstall && info.featureChannel === "nexus-ws"
                  ? t("Getting the complete edition")
                  : t("Update Available")}
            </h2>
            {phase !== "installing" && (
              <div className="mt-1 flex flex-wrap items-center gap-2">
                <span className="text-xs font-medium tabular-nums text-[var(--panel-fg)]/80">v{info.version}</span>
                <ReleaseKindBadge kind={releaseKind} />
              </div>
            )}
          </div>
          {phase !== "downloading" && phase !== "installing" && (
            <button onClick={handleClose} className="theme-muted hover:opacity-100 transition-opacity">
              <X size={16} strokeWidth={2} />
            </button>
          )}
        </div>

        {phase === "installing" ? (
          <div className="px-5 pt-2 pb-6 flex flex-col items-center text-center animate-fade-in" role="status">
            <div className="relative mb-4">
              <div className="absolute inset-0 rounded-2xl bg-emerald-500/25 blur-xl animate-pulse" />
              <div className="relative w-14 h-14 rounded-2xl theme-accent-bg theme-accent-border border flex items-center justify-center">
                <Sparkles size={24} strokeWidth={1.8} className="text-[var(--panel-fg)]" />
              </div>
            </div>
            <div className="text-[15px] font-semibold text-[var(--panel-fg)]">
              {t("Installing v{{version}}", { version: info.version })}
            </div>
            <div className="mt-1 text-xs theme-muted max-w-xs leading-relaxed">
              {t("MultiAlt will close and open again by itself in a few seconds.")}
            </div>
            <div className="mt-5 w-full max-w-xs h-1.5 rounded-full bg-zinc-700/50 overflow-hidden">
              <div className="h-full w-2/5 rounded-full bg-emerald-500 animate-update-indeterminate" />
            </div>
            <div className="mt-3 text-[11px] theme-muted">
              {t("Your accounts and settings stay where they are.")}
            </div>
          </div>
        ) : (
          <div className="px-5 pb-3">
            <div className="text-xs font-medium theme-muted mb-1.5">{t("Release Notes")}</div>
            <div className="theme-input rounded-lg px-3.5 py-3 max-h-56 overflow-y-auto text-[12px] text-[var(--panel-fg)] leading-[1.55]">
              {releaseNotes
                ? renderedNotes
                : t("Could not load release notes")}
            </div>
          </div>
        )}

        {(phase === "downloading" || phase === "ready") && (
          <div className="px-5 pb-3">
            <div className="w-full h-2 rounded-full bg-zinc-700/50 overflow-hidden">
              <div
                className="h-full rounded-full bg-sky-500 transition-all duration-300"
                style={{ width: `${progress.total ? pct : 0}%` }}
              />
            </div>
            <div className="flex justify-between mt-1.5 text-xs theme-muted">
              <span>
                {progress.total
                  ? `${pct}% — ${formatBytes(progress.downloaded)} / ${formatBytes(progress.total)}`
                  : formatBytes(progress.downloaded)}
              </span>
              {phase === "downloading" && progress.speed > 0 && (
                <span>{formatBytes(progress.speed)}/s</span>
              )}
            </div>
          </div>
        )}

        {phase === "error" && (
          <div className="px-5 pb-3">
            <div className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">
              {errorMsg}
            </div>
          </div>
        )}

        {phase !== "installing" && (
        <div className="flex items-center justify-between px-5 pb-5 pt-2 border-t border-[var(--border-color)]">
          <div className="flex gap-2">
            {phase === "available" && (
              <>
                <button
                  onClick={skipVersion}
                  className="px-3 py-1.5 rounded-md text-xs text-zinc-400 hover:text-zinc-200 hover:bg-zinc-700/50 transition-colors"
                >
                  {t("Skip This Version")}
                </button>
                <button
                  onClick={handleClose}
                  className="px-3 py-1.5 rounded-md text-xs text-zinc-400 hover:text-zinc-200 hover:bg-zinc-700/50 transition-colors"
                >
                  {t("Remind Me Later")}
                </button>
              </>
            )}
          </div>
          <div>
            {phase === "available" && (
              <button
                onClick={startDownload}
                className="px-4 py-1.5 rounded-md text-xs font-medium bg-sky-600 hover:bg-sky-500 text-white transition-colors"
              >
                {t("Download Update")}
              </button>
            )}
            {phase === "downloading" && (
              <button
                disabled
                className="px-4 py-1.5 rounded-md text-xs font-medium bg-sky-600/50 text-white/60 cursor-not-allowed"
              >
                {t("Downloading update...")}
              </button>
            )}
            {phase === "ready" && (
              <button
                onClick={installAndRestart}
                className="px-4 py-1.5 rounded-md text-xs font-medium bg-emerald-600 hover:bg-emerald-500 text-white transition-colors"
              >
                {t("Install & Restart")}
              </button>
            )}
            {phase === "error" && (
              <button
                onClick={startDownload}
                className="px-4 py-1.5 rounded-md text-xs font-medium bg-sky-600 hover:bg-sky-500 text-white transition-colors"
              >
                {t("Download Update")}
              </button>
            )}
          </div>
        </div>
        )}
      </div>
    </div>
  );
}
