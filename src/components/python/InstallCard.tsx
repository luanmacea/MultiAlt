import { useState } from "react";
import { AlertCircle, CheckCircle2, Copy, Download, Loader2, RefreshCw } from "lucide-react";
import { useConfirm } from "../../hooks/usePrompt";
import { copyText } from "./copyText";
import type { PythonInstallProgress, PythonRuntimeStatus } from "./pythonApi";

const STAGE_LABEL: Record<PythonInstallProgress["stage"], string> = {
  python: "Downloading Python",
  pip: "Setting up pip",
  packages: "Installing numpy and OpenCV",
  ready: "Finishing",
  error: "Failed",
};

function formatMb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export interface InstallCardProps {
  /** `null` while the first status answer is still on its way. */
  status: PythonRuntimeStatus | null;
  progress: PythonInstallProgress | null;
  error: string | null;
  installing: boolean;
  onInstall: () => void;
  onReinstall: () => void;
}

/**
 * Install state of the portable Python. Nothing is downloaded until the user
 * presses the button: the card says exactly what will be fetched and from where.
 */
export function InstallCard({ status, progress, error, installing, onInstall, onReinstall }: InstallCardProps) {
  const confirm = useConfirm();
  const [copied, setCopied] = useState(false);

  if (installing) {
    const stage = progress?.stage ?? "python";
    const hasTotal = (progress?.total ?? 0) > 0;
    const percent = hasTotal ? Math.min(100, Math.round(((progress?.downloaded ?? 0) / (progress?.total ?? 1)) * 100)) : null;
    return (
      <div className="rounded-lg border theme-border bg-[var(--panel-soft)] px-4 py-3">
        <div className="flex items-center gap-2 text-[13px] font-medium text-zinc-100">
          <Loader2 size={14} strokeWidth={2} className="animate-spin text-sky-300" />
          Installing Python
          <span className="font-normal text-zinc-500">- {STAGE_LABEL[stage]}</span>
        </div>
        <div
          role="progressbar"
          aria-label="Install progress"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent ?? undefined}
          className="mt-2.5 h-1.5 overflow-hidden rounded-full bg-zinc-800"
        >
          <div
            className={`h-full rounded-full bg-sky-400 transition-[width] duration-300 ${percent === null ? "w-1/3 animate-pulse" : ""}`}
            style={percent === null ? undefined : { width: `${percent}%` }}
          />
        </div>
        <div className="mt-1.5 flex items-center justify-between gap-3 text-[11.5px] text-zinc-500">
          <span className="min-w-0 truncate">{progress?.message ?? ""}</span>
          {hasTotal ? (
            <span className="shrink-0">
              {formatMb(progress?.downloaded ?? 0)} / {formatMb(progress?.total ?? 0)}
            </span>
          ) : null}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div role="alert" className="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3">
        <div className="flex items-center gap-2 text-[13px] font-medium text-red-300">
          <AlertCircle size={14} strokeWidth={2} />
          Python could not be installed
        </div>
        <p className="mt-1 break-words text-[12px] text-red-200/80">{error}</p>
        <button type="button" onClick={onInstall} className="theme-btn mt-2.5 inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px]">
          <RefreshCw size={12} strokeWidth={2} />
          Try again
        </button>
      </div>
    );
  }

  if (!status) {
    return (
      <div className="rounded-lg border theme-border bg-[var(--panel-soft)] px-4 py-3 text-[12px] text-zinc-500">
        Checking for Python...
      </div>
    );
  }

  if (!status.installed) {
    return (
      <div className="rounded-lg border theme-border bg-[var(--panel-soft)] px-4 py-3.5">
        <div className="text-[13px] font-medium text-zinc-100">Python is not installed</div>
        <p className="mt-1 max-w-[70ch] text-[12px] leading-relaxed text-zinc-400">
          Scripts run on a private copy of Python that MultiAlt keeps for itself. It does not touch any Python you
          already have. Nothing is downloaded until you press the button.
        </p>
        <ul className="mt-2 list-disc space-y-0.5 pl-4 text-[11.5px] text-zinc-500">
          <li>Python 3.12 (embeddable build) from python.org</li>
          <li>pip, from PyPI (files.pythonhosted.org)</li>
          <li>numpy and opencv-python-headless (image search) from PyPI</li>
        </ul>
        <button type="button" onClick={onInstall} className="theme-btn mt-3 inline-flex items-center gap-1.5 px-3.5 py-1.5 text-[12.5px] font-medium">
          <Download size={13} strokeWidth={2} />
          Install Python (~65 MB)
        </button>
      </div>
    );
  }

  const pipCommand = `"${status.pythonExe ?? "python"}" -m pip install <package>`;
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border theme-border bg-[var(--panel-soft)] px-4 py-2.5">
      <span className="inline-flex items-center gap-1.5 text-[12.5px] text-zinc-200">
        <CheckCircle2 size={14} strokeWidth={2} className="text-emerald-400" />
        Python {status.version ?? ""} installed
      </span>
      {status.packages.length > 0 ? (
        <span className="min-w-0 truncate text-[11.5px] text-zinc-500">{status.packages.join(", ")}</span>
      ) : null}
      <span className="ml-auto flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={async () => {
            if (await copyText(pipCommand)) {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1500);
            }
          }}
          title={pipCommand}
          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[11.5px]"
        >
          <Copy size={11} strokeWidth={2} />
          {copied ? "Copied" : "Copy pip install command"}
        </button>
        <button
          type="button"
          onClick={async () => {
            const ok = await confirm(
              "Reinstall Python? The private copy of Python is downloaded again. Your scripts, assets and settings are not touched.",
              false,
              { confirmLabel: "Reinstall" }
            );
            if (ok) onReinstall();
          }}
          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[11.5px]"
        >
          <RefreshCw size={11} strokeWidth={2} />
          Reinstall
        </button>
      </span>
    </div>
  );
}
