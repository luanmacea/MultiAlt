import { useCallback, useEffect, useState, type KeyboardEvent } from "react";
import { Copy, FolderOpen, RotateCcw, Save, X } from "lucide-react";
import { useStore } from "../../store";
import { useConfirm } from "../../hooks/usePrompt";
import { Toggle } from "../ui/Toggle";
import { copyText } from "./copyText";
import { errorText, pythonApi, type DefaultBlockInfo, type PythonWorkspaceInfo } from "./pythonApi";

export function restoreConfirmText(name: string): string {
  return `Restore ${name} to the default? Your changes to this block will be lost. Your scripts and other blocks are not touched.`;
}

function BlockEditor({ name, onSaved, onClose }: { name: string; onSaved: () => void; onClose: () => void }) {
  const [source, setSource] = useState("");
  const [saved, setSaved] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    pythonApi
      .readDefaultBlock(name)
      .then((text) => {
        if (cancelled) return;
        const content = typeof text === "string" ? text : "";
        setSource(content);
        setSaved(content);
      })
      .catch((e) => {
        if (!cancelled) setError(errorText(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  const dirty = source !== saved;

  async function save() {
    setError(null);
    try {
      await pythonApi.writeDefaultBlock(name, source);
      setSaved(source);
      onSaved();
    } catch (e) {
      setError(errorText(e));
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      void save();
    }
  }

  return (
    <div className="mt-2 rounded-lg border theme-border">
      <div className="flex items-center gap-2 border-b theme-border px-3 py-1.5">
        <span className="font-mono text-[12px] text-zinc-200">{name}</span>
        {dirty ? <span className="text-[11px] text-amber-300">Unsaved changes</span> : null}
        <button
          type="button"
          onClick={() => void save()}
          disabled={!dirty}
          className="theme-btn ml-auto inline-flex items-center gap-1.5 px-2.5 py-1 text-[12px] disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Save size={12} strokeWidth={2} />
          Save
        </button>
        <button type="button" aria-label={`Close ${name}`} onClick={onClose} className="rounded p-1 text-zinc-500 hover:bg-zinc-700/60 hover:text-zinc-200">
          <X size={13} strokeWidth={2} />
        </button>
      </div>
      {error ? (
        <div role="alert" className="border-b border-red-500/30 bg-red-500/10 px-3 py-1.5 text-[12px] text-red-300">
          {error}
        </div>
      ) : null}
      <textarea
        aria-label={`Source of block ${name}`}
        value={source}
        disabled={loading}
        onChange={(e) => setSource(e.target.value)}
        onKeyDown={onKeyDown}
        spellCheck={false}
        wrap="off"
        className="block h-[300px] w-full resize-y bg-transparent p-3 font-mono text-[12px] leading-[1.55] text-zinc-200 focus:outline-none"
      />
    </div>
  );
}

/** Developer tab: the mode toggle, the default blocks, and the terminal channel. */
export function DeveloperPanel() {
  const store = useStore();
  const confirm = useConfirm();
  const [enabled, setEnabled] = useState(() => store.settings?.Python?.DeveloperMode === "true");
  const [busy, setBusy] = useState(false);
  const [blocks, setBlocks] = useState<DefaultBlockInfo[]>([]);
  const [info, setInfo] = useState<PythonWorkspaceInfo | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [editorRevision, setEditorRevision] = useState(0);
  const [copied, setCopied] = useState(false);

  const toast = store.addToast;

  const refreshBlocks = useCallback(async () => {
    try {
      const list = await pythonApi.listDefaultBlocks();
      if (Array.isArray(list)) setBlocks(list);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }, [toast]);

  useEffect(() => {
    if (!enabled) {
      setOpen(null);
      return;
    }
    void refreshBlocks();
    pythonApi
      .workspaceInfo()
      .then((value) => setInfo(value ?? null))
      .catch(() => {});
  }, [enabled, refreshBlocks]);

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

  async function restore(name: string) {
    const ok = await confirm(restoreConfirmText(name), true, { confirmLabel: "Restore default" });
    if (!ok) return;
    try {
      await pythonApi.restoreDefaultBlock(name);
      setEditorRevision((n) => n + 1);
      await refreshBlocks();
      toast(`${name} restored to the default.`, "success");
    } catch (e) {
      toast(errorText(e), "error");
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
              actions.py, dashboard.py). They hold the code that switches between windows and calls your script. Each one
              can be restored to its default on its own.
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
        <>
          <section className="rounded-lg border theme-border bg-[var(--panel-soft)]">
            <header className="border-b theme-border px-4 py-2">
              <h3 className="text-[12.5px] font-medium text-zinc-100">Default blocks</h3>
            </header>
            {blocks.length === 0 ? (
              <div className="px-4 py-3 text-[12px] text-zinc-500">No blocks found.</div>
            ) : (
              <ul className="divide-y divide-zinc-800/60">
                {blocks.map((block) => (
                  <li key={block.name} className="px-4 py-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-[12.5px] text-zinc-200">{block.name}</span>
                      {block.modified ? (
                        <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[10.5px] font-medium text-amber-300">Modified</span>
                      ) : null}
                      {block.newDefaultAvailable ? (
                        <span className="rounded bg-sky-500/15 px-1.5 py-0.5 text-[10.5px] font-medium text-sky-300">
                          New default available
                        </span>
                      ) : null}
                      <span className="ml-auto flex gap-1.5">
                        <button
                          type="button"
                          aria-label={`Open ${block.name}`}
                          onClick={() => setOpen(open === block.name ? null : block.name)}
                          className="theme-btn-ghost px-2 py-0.5 text-[11.5px]"
                        >
                          {open === block.name ? "Hide" : "Open"}
                        </button>
                        <button
                          type="button"
                          aria-label={`Restore default of ${block.name}`}
                          onClick={() => void restore(block.name)}
                          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-0.5 text-[11.5px]"
                        >
                          <RotateCcw size={10} strokeWidth={2} />
                          Restore default
                        </button>
                      </span>
                    </div>
                    {open === block.name ? (
                      <BlockEditor
                        key={`${block.name}-${editorRevision}`}
                        name={block.name}
                        onSaved={() => void refreshBlocks()}
                        onClose={() => setOpen(null)}
                      />
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>

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
        </>
      ) : null}
    </div>
  );
}
