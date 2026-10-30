import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Copy, Lock, Plus, RotateCcw } from "lucide-react";
import { useConfirm, usePrompt } from "../../hooks/usePrompt";
import { copyText } from "./copyText";
import { errorText, pythonApi, type GlobalFunction, type GlobalModule } from "./pythonApi";
import { TREE_POLL_MS } from "./useWorkspaceTree";
import { baseName } from "./workspaceTree";

type Tone = "info" | "success" | "warn" | "error";

export function restoreConfirmText(name: string): string {
  return `Restore ${name} to the default? Your changes to this block will be lost. Your scripts and other blocks are not touched.`;
}

export function importLine(module: string, fn: GlobalFunction): string {
  return `from ${module} import ${fn.name}`;
}

const MODULE_NAME = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/;

/** A starter for `lib/<name>.py`: one function with the docstring this tab shows. */
export function newFunctionTemplate(name: string): string {
  return `from multialt import sleep


def ${name}(account, seconds=1.0):
    """Say in one line what ${name} does. This line shows up in Global functions."""
    with account.focused():
        account.press("W")
        sleep(seconds)
`;
}

interface GlobalFunctionsTabProps {
  visible: boolean;
  developerMode: boolean;
  /** Opens the file in the Code tab. */
  onOpenFile: (path: string) => void;
  onOpenDeveloper: () => void;
  onToast: (message: string, tone?: Tone) => void;
}

/** Global functions: the default blocks (locked) and the modules of `lib/`, each with a ready-to-paste import. */
export function GlobalFunctionsTab({ visible, developerMode, onOpenFile, onOpenDeveloper, onToast }: GlobalFunctionsTabProps) {
  const prompt = usePrompt();
  const confirm = useConfirm();
  const [modules, setModules] = useState<GlobalModule[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const signature = useRef("");

  const refresh = useCallback(async () => {
    try {
      const list = await pythonApi.listGlobalFunctions();
      if (Array.isArray(list)) {
        const sig = JSON.stringify(list);
        if (sig !== signature.current) {
          signature.current = sig;
          setModules(list);
        }
      }
    } catch (e) {
      onToast(errorText(e), "error");
    } finally {
      setLoaded(true);
    }
    // The toast is only for the first failure; the poll stays quiet afterwards.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!visible) return;
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "hidden") void refresh();
    }, TREE_POLL_MS);
    return () => window.clearInterval(timer);
  }, [visible, refresh]);

  const pinned = modules.filter((module) => module.pinned);
  const mine = modules.filter((module) => !module.pinned);

  async function copyImport(key: string, line: string) {
    if (await copyText(line)) {
      setCopied(key);
      window.setTimeout(() => setCopied((current) => (current === key ? null : current)), 1500);
    }
  }

  async function restore(module: GlobalModule) {
    const name = baseName(module.path);
    const ok = await confirm(restoreConfirmText(name), true, { confirmLabel: "Restore default" });
    if (!ok) return;
    try {
      await pythonApi.restoreDefaultBlock(name);
      await refresh();
      onToast(`${name} restored to the default.`, "success");
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  async function newFunction() {
    const raw = await prompt("Name of the new module in lib/ (letters, numbers and _). It holds your functions.", "", {
      maxLength: 64,
    });
    if (raw === null) return;
    const name = raw.trim().replace(/\.py$/i, "");
    if (!MODULE_NAME.test(name)) {
      onToast("Use letters, numbers and _ only, and do not start with a number.", "warn");
      return;
    }
    if (modules.some((module) => module.module === `lib.${name}`)) {
      onToast(`lib/${name}.py already exists.`, "warn");
      return;
    }
    const path = `lib/${name}.py`;
    try {
      await pythonApi.fsCreate(path, "file");
      await pythonApi.fsWrite(path, newFunctionTemplate(name));
      await refresh();
      onOpenFile(path);
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  function renderModule(module: GlobalModule) {
    return (
      <li key={module.path} className="rounded-lg border theme-border bg-[var(--panel-soft)]">
        <div className="flex flex-wrap items-center gap-2 border-b theme-border px-3 py-2">
          {module.pinned ? (
            <Lock size={12} strokeWidth={2} role="img" aria-label="Locked default block" className="shrink-0 text-zinc-400" />
          ) : null}
          <button
            type="button"
            onClick={() => onOpenFile(module.path)}
            title={`Open ${module.path} in the Code tab`}
            className="font-mono text-[12.5px] text-zinc-100 underline-offset-2 hover:underline"
          >
            {module.module}
          </button>
          <span className="text-[11.5px] text-zinc-500">{module.path}</span>
          {module.modified ? (
            <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[11px] font-medium text-amber-300">Modified</span>
          ) : null}
          {module.newDefaultAvailable ? (
            <span className="rounded bg-sky-500/15 px-1.5 py-0.5 text-[11px] font-medium text-sky-300">New default available</span>
          ) : null}
          {module.pinned ? (
            <button
              type="button"
              aria-label={`Restore default of ${baseName(module.path)}`}
              disabled={!developerMode}
              title={developerMode ? "Put the original code back" : "Turn on Developer mode to restore a block"}
              onClick={() => void restore(module)}
              className="theme-btn-ghost ml-auto inline-flex items-center gap-1 px-2 py-0.5 text-[11.5px] disabled:cursor-not-allowed disabled:opacity-40"
            >
              <RotateCcw size={10} strokeWidth={2} />
              Restore default
            </button>
          ) : null}
        </div>
        {module.functions.length === 0 ? (
          <div className="px-3 py-2.5 text-[12px] text-zinc-500">No top-level functions in this file yet.</div>
        ) : (
          <ul className="divide-y divide-zinc-800/60">
            {module.functions.map((fn) => {
              const line = importLine(module.module, fn);
              const key = `${module.path}:${fn.name}`;
              return (
                <li key={key} className="flex flex-wrap items-start gap-x-3 gap-y-1 px-3 py-2">
                  <button
                    type="button"
                    onClick={() => onOpenFile(module.path)}
                    title={`Open ${module.path} in the Code tab`}
                    className="min-w-0 flex-1 basis-[260px] text-left"
                  >
                    <span className="block break-words font-mono text-[12px] text-sky-200">{fn.signature}</span>
                    {fn.doc ? <span className="mt-0.5 block text-[12px] text-zinc-400">{fn.doc}</span> : null}
                  </button>
                  <div className="flex min-w-0 max-w-full items-center gap-1.5">
                    <code className="min-w-0 truncate rounded-md border theme-border bg-zinc-900/50 px-2 py-1 font-mono text-[11.5px] text-zinc-300">
                      {line}
                    </code>
                    <button
                      type="button"
                      aria-label={`Copy import of ${fn.name}`}
                      onClick={() => void copyImport(key, line)}
                      className="theme-btn-ghost inline-flex shrink-0 items-center gap-1 px-2 py-1 text-[11.5px]"
                    >
                      {copied === key ? <Check size={11} strokeWidth={2} /> : <Copy size={11} strokeWidth={2} />}
                      {copied === key ? "Copied" : "Copy"}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </li>
    );
  }

  return (
    <div className="max-w-[980px] space-y-5">
      <div className="flex flex-wrap items-start gap-3">
        <p className="min-w-0 flex-1 basis-[320px] text-[12px] leading-relaxed text-zinc-400">
          Functions any script can import. The default blocks come with MultiAlt; the modules in{" "}
          <span className="font-mono text-zinc-300">lib/</span> are yours. Copy the import line into a script, or click a
          function to open its file in the Code tab.
        </p>
        <button type="button" onClick={() => void newFunction()} className="theme-btn inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px]">
          <Plus size={12} strokeWidth={2} />
          New function
        </button>
      </div>

      {!loaded ? (
        <div className="text-[12px] text-zinc-500">Loading functions...</div>
      ) : (
        <>
          <section aria-label="Default blocks" className="space-y-2">
            <h3 className="flex flex-wrap items-center gap-2 text-[12.5px] font-medium text-zinc-100">
              <Lock size={12} strokeWidth={2} className="text-zinc-400" />
              Default blocks
            </h3>
            <p className="text-[12px] text-zinc-500">
              {developerMode ? (
                "Developer mode is on: you can edit these blocks in the Code tab and restore each one to its default."
              ) : (
                <>
                  Locked. Editing or restoring them needs Developer mode.{" "}
                  <button type="button" onClick={onOpenDeveloper} className="text-sky-300 underline-offset-2 hover:underline">
                    Open the Developer tab
                  </button>
                </>
              )}
            </p>
            {pinned.length === 0 ? (
              <div className="text-[12px] text-zinc-500">No default blocks found.</div>
            ) : (
              <ul className="space-y-2">{pinned.map(renderModule)}</ul>
            )}
          </section>

          <section aria-label="Your modules" className="space-y-2">
            <h3 className="text-[12.5px] font-medium text-zinc-100">Your modules (lib/)</h3>
            {mine.length === 0 ? (
              <div className="rounded-lg border border-dashed theme-border px-3 py-4 text-[12px] text-zinc-500">
                Nothing here yet. Press <span className="text-zinc-300">New function</span> to create a module you can
                import from any script.
              </div>
            ) : (
              <ul className="space-y-2">{mine.map(renderModule)}</ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}
