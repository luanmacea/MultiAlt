import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ShieldCheck, ShieldAlert, Loader2, CheckCircle2, Info } from "lucide-react";
import { useTr } from "../i18n/text";

interface IsolationProgress {
  stage: string;
  message: string;
  pathsCleaned: number;
  bytesFreed: number;
  finished: boolean;
}

const ELEVATION_STAGES = new Set(["requesting-elevation"]);
const ERROR_STAGES = new Set(["spoof-failed"]);
/**
 * Isolamento pulado porque há cliente aberto: é a regra (não fechar as outras
 * contas), não um erro. Antes aparecia em vermelho como "Isolation issue" a
 * cada launch de quem joga com várias contas ou com a main aberta num
 * executor; agora é informação, mostrada uma vez até um isolamento rodar.
 */
const SKIPPED_STAGE = "skipped";
const HIDE_AFTER_MS = 2500;

let skipNoticeShown = false;

/** Só para os testes: esquece que o aviso de "pulado" já foi mostrado. */
export function resetIsolationSkipNotice(): void {
  skipNoticeShown = false;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "—";
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${mb.toFixed(0)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function IsolationProgressOverlay() {
  const t = useTr();
  const [progress, setProgress] = useState<IsolationProgress | null>(null);
  const [visible, setVisible] = useState(false);
  const hideTimerRef = useRef<number | null>(null);

  useEffect(() => {
    const unlisten = listen<IsolationProgress>("isolation-progress", (e) => {
      const next = e.payload;
      if (next.stage === SKIPPED_STAGE) {
        if (skipNoticeShown) {
          // Já avisado: some com o que estiver na tela (um "starting" antigo
          // ficaria parado, já que ele não termina sozinho).
          setVisible(false);
          return;
        }
        skipNoticeShown = true;
      } else if (next.finished) {
        skipNoticeShown = false;
      }
      setProgress(next);
      setVisible(true);
      if (hideTimerRef.current !== null) {
        window.clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
      if (next.finished) {
        hideTimerRef.current = window.setTimeout(() => {
          setVisible(false);
          hideTimerRef.current = null;
        }, HIDE_AFTER_MS);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
      if (hideTimerRef.current !== null) {
        window.clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
    };
  }, []);

  if (!visible || !progress) return null;

  const isError = ERROR_STAGES.has(progress.stage);
  const isSkipped = progress.stage === SKIPPED_STAGE;
  const isElevation = ELEVATION_STAGES.has(progress.stage);
  const isFinished = progress.finished && !isError && !isSkipped;

  const Icon = isError
    ? ShieldAlert
    : isSkipped
    ? Info
    : isFinished
    ? CheckCircle2
    : isElevation
    ? ShieldAlert
    : ShieldCheck;

  const iconClass = isError
    ? "text-rose-400"
    : isSkipped
    ? "text-zinc-400"
    : isFinished
    ? "text-emerald-400"
    : isElevation
    ? "text-amber-400 animate-pulse"
    : "text-sky-400";

  return (
    <div className="pointer-events-none fixed top-12 left-1/2 -translate-x-1/2 z-[80] animate-fade-in">
      <div className="theme-modal-scope theme-panel theme-border pointer-events-auto rounded-2xl border border-zinc-800/80 bg-zinc-900/95 backdrop-blur shadow-2xl px-4 py-3 min-w-[320px] max-w-[440px]">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 shrink-0">
            {isFinished || isError || isSkipped ? (
              <Icon size={18} strokeWidth={1.75} className={iconClass} />
            ) : (
              <div className="relative">
                <Icon size={18} strokeWidth={1.75} className={iconClass} />
                {!isElevation && (
                  <Loader2
                    size={18}
                    strokeWidth={1.5}
                    className="absolute inset-0 text-sky-400/40 animate-spin"
                  />
                )}
              </div>
            )}
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-[12px] font-medium text-zinc-100">
              {isFinished
                ? t("Isolation complete")
                : isError
                ? t("Isolation issue")
                : isSkipped
                ? t("Isolation skipped")
                : t("Pre-launch isolation")}
            </div>
            <div className="mt-0.5 text-[12px] text-zinc-400 leading-snug break-words">
              {progress.message}
            </div>
            {(progress.pathsCleaned > 0 || progress.bytesFreed > 0) && (
              <div className="mt-1.5 flex items-center gap-3 text-[11px] text-zinc-500 font-mono">
                <span>
                  {t("{{count}} paths", { count: progress.pathsCleaned })}
                </span>
                <span>{formatBytes(progress.bytesFreed)}</span>
              </div>
            )}
            {isElevation && (
              <div className="mt-2 inline-flex items-center gap-1.5 rounded-md bg-amber-500/10 border border-amber-500/20 px-2 py-1 text-[11px] text-amber-300">
                {t("Accept the UAC prompt to continue")}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
