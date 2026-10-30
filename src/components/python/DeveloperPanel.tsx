import { useEffect, useState } from "react";
import { Copy, FolderOpen } from "lucide-react";
import { useStore } from "../../store";
import { Toggle } from "../ui/Toggle";
import { copyText } from "./copyText";
import { errorText, pythonApi, type PythonWorkspaceInfo } from "./pythonApi";

/** Developer tab: the mode toggle and the terminal channel. The blocks themselves live in Code and Global functions. */
export function DeveloperPanel() {
  const store = useStore();
  const [enabled, setEnabled] = useState(() => store.settings?.Python?.DeveloperMode === "true");
  const [busy, setBusy] = useState(false);
  const [info, setInfo] = useState<PythonWorkspaceInfo | null>(null);
  const [copied, setCopied] = useState(false);

  const toast = store.addToast;

  useEffect(() => {
    if (!enabled) return;
    pythonApi
      .workspaceInfo()
      .then((value) => setInfo(value ?? null))
      .catch(() => {});
  }, [enabled]);

  async function toggle(next: boolean) {
    if (busy) return;
    setBusy(true);
    try {
      await pythonApi.setDeveloperMode(next);
      setEnabled(next);
      void store.reloadSettings();
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setBusy(false);
    }
  }

  async function copyCommand() {
    if (!info?.cliCommand) return;
    if (await copyText(info.cliCommand)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    }
  }

  return (
    <div className="max-w-[860px] space-y-4">
      <section className="rounded-lg border theme-border bg-[var(--panel-soft)] px-4 py-3">
        <Toggle label="Developer mode" checked={enabled} disabled={busy} onChange={(next) => void toggle(next)} />
        <div className="mt-1 space-y-2 pl-12 text-[12px] leading-relaxed text-zinc-400">
          <p>Developer mode is off by default. Turning it on unlocks two things:</p>
          <ul className="list-disc space-y-1 pl-4">
            <li>
              <span className="text-zinc-200">Editing the default blocks</span> (runner.py, focus.py, vision.py,
              actions.py, dashboard.py). They hold the code that switches between windows and calls your script. Open
              them in the Code tab; each one can be restored to its default from the Global functions tab.
            </li>
            <li>
              <span className="text-zinc-200">A terminal channel.</span> A command like{" "}
              <code className="font-mono text-zinc-300">python -m multialt run ...</code> connects to MultiAlt and runs a
              script as a normal run, so AI tools such as Claude can write a script, run it, read the error and fix it.
              It uses a private pipe that only your Windows user can open. Nothing listens on the network.
            </li>
          </ul>
          <p>Safety locks (focus queue, foreground check, tracked windows only) stay on whatever the blocks say.</p>
        </div>
      </section>

      {enabled ? (
        <section className="rounded-lg border theme-border bg-[var(--panel-soft)] px-4 py-3">
          <h3 className="text-[12.5px] font-medium text-zinc-100">Terminal channel</h3>
          <p className="mt-1 text-[12px] text-zinc-400">
            Run a script from a terminal (or let an AI tool do it). The run shows up in Running as &quot;terminal&quot;.
          </p>
          <div className="mt-2 flex items-start gap-2">
            <code
              data-testid="cli-command"
              className="min-w-0 flex-1 break-all rounded-md border theme-border bg-zinc-900/50 px-2.5 py-1.5 font-mono text-[11.5px] text-zinc-300"
            >
              {info?.cliCommand ?? "Loading..."}
            </code>
            <button
              type="button"
              onClick={() => void copyCommand()}
              disabled={!info?.cliCommand}
              className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[11.5px]"
            >
              <Copy size={11} strokeWidth={2} />
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <button
              type="button"
              onClick={() => void pythonApi.openWorkspaceFolder().catch((e) => toast(errorText(e), "error"))}
              className="theme-btn inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px]"
            >
              <FolderOpen size={12} strokeWidth={2} />
              Open workspace folder
            </button>
            <span className="text-[11.5px] text-zinc-500">
              Point your AI tool at this folder: it contains AGENTS.md and docs/API.md written for it.
            </span>
          </div>
        </section>
      ) : null}
    </div>
  );
}
