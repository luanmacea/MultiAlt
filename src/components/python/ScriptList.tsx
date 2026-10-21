import { useEffect, useRef, useState } from "react";
import { ChevronDown, ExternalLink, FileCode2, FolderOpen, Plus, Trash2 } from "lucide-react";
import { useConfirm, usePrompt } from "../../hooks/usePrompt";
import {
  errorText,
  isValidScriptName,
  pythonApi,
  type PythonScriptFile,
  type PythonScriptTemplate,
} from "./pythonApi";

const TEMPLATES: { id: PythonScriptTemplate; label: string; hint: string }[] = [
  { id: "blank", label: "Blank", hint: "An empty script" },
  { id: "per_account", label: "Per account", hint: "main(account) runs once per account" },
  { id: "controller", label: "Controller", hint: "main_all(accounts) runs once for all" },
  { id: "dashboard", label: "Dashboard", hint: "Shows a report in the Dashboard tab" },
];

interface ScriptListProps {
  scripts: PythonScriptFile[];
  loaded: boolean;
  selected: string | null;
  onSelect: (name: string | null) => void;
  /** Re-reads the folder right away (after a create or delete). */
  onChanged: () => Promise<void>;
  onToast: (message: string, tone?: "info" | "success" | "warn" | "error") => void;
}

/** Accepts `farm` and `farm.py`; returns the file name or `null` when it is not valid. */
export function normalizeScriptName(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const name = /\.py$/i.test(trimmed) ? trimmed.replace(/\.py$/i, ".py") : `${trimmed}.py`;
  return isValidScriptName(name) ? name : null;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
}

export function ScriptList({ scripts, loaded, selected, onSelect, onChanged, onToast }: ScriptListProps) {
  const prompt = usePrompt();
  const confirm = useConfirm();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    function onDown(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) setMenuOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen]);

  async function createFrom(template: PythonScriptTemplate) {
    setMenuOpen(false);
    const raw = await prompt("Name of the new script (letters, numbers, - and _)", "", { maxLength: 64 });
    if (raw === null) return;
    const name = normalizeScriptName(raw);
    if (!name) {
      onToast("Use only letters, numbers, - and _ (up to 64 characters).", "warn");
      return;
    }
    if (scripts.some((script) => script.name.toLowerCase() === name.toLowerCase())) {
      onToast(`A script named ${name} already exists.`, "warn");
      return;
    }
    try {
      await pythonApi.createScript(name, template);
      await onChanged();
      onSelect(name);
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  async function remove(name: string) {
    const ok = await confirm(`Delete ${name}? This cannot be undone.`, true, { confirmLabel: "Delete" });
    if (!ok) return;
    try {
      await pythonApi.deleteScript(name);
      if (selected === name) onSelect(null);
      await onChanged();
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  async function run(action: () => Promise<void>) {
    try {
      await action();
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  return (
    <aside aria-label="Scripts" className="flex min-h-0 flex-col rounded-lg border theme-border bg-[var(--panel-soft)]">
      <div className="flex flex-wrap items-center gap-1.5 border-b theme-border px-2.5 py-2">
        <div ref={menuRef} className="relative">
          <button
            type="button"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((open) => !open)}
            className="theme-btn inline-flex items-center gap-1.5 px-2.5 py-1 text-[12px]"
          >
            <Plus size={12} strokeWidth={2} />
            New
            <ChevronDown size={11} strokeWidth={2} className={`transition-transform ${menuOpen ? "rotate-180" : ""}`} />
          </button>
          {menuOpen && (
            <div
              role="menu"
              className="theme-modal-scope theme-panel theme-border absolute left-0 top-full z-30 mt-1.5 w-[260px] overflow-hidden rounded-xl border shadow-2xl"
            >
              {TEMPLATES.map((template) => (
                <button
                  key={template.id}
                  type="button"
                  role="menuitem"
                  onClick={() => void createFrom(template.id)}
                  className="block w-full px-3 py-2 text-left hover:bg-zinc-800/70"
                >
                  <span className="block text-[12px] text-zinc-200">{template.label}</span>
                  <span className="block text-[11px] text-zinc-500">{template.hint}</span>
                </button>
              ))}
            </div>
          )}
        </div>
        <button
          type="button"
          onClick={() => void run(() => pythonApi.openWorkspaceFolder())}
          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[11.5px]"
        >
          <FolderOpen size={11} strokeWidth={2} />
          Open folder
        </button>
        <span className="ml-auto text-[11px] text-zinc-600">{scripts.length} scripts</span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {!loaded ? (
          <div className="px-3 py-4 text-[12px] text-zinc-500">Loading scripts...</div>
        ) : scripts.length === 0 ? (
          <div className="px-3 py-5 text-[12px] leading-relaxed text-zinc-500">
            No scripts yet. Press <span className="text-zinc-300">New</span> to start from a template, or drop a .py file
            in the scripts folder.
          </div>
        ) : (
          scripts.map((script) => {
            const isSelected = script.name === selected;
            return (
              <div
                key={script.name}
                className={`group flex items-center gap-2 border-b border-zinc-800/50 px-3 py-2 ${
                  isSelected ? "bg-zinc-800/70" : "hover:bg-zinc-800/35"
                }`}
              >
                <button
                  type="button"
                  onClick={() => onSelect(script.name)}
                  aria-current={isSelected ? "true" : undefined}
                  className="flex min-w-0 flex-1 items-center gap-2 text-left"
                >
                  <FileCode2 size={13} strokeWidth={1.8} className="shrink-0 text-sky-300" />
                  <span className="min-w-0">
                    <span className="block truncate text-[12px] font-medium text-zinc-200">{script.name}</span>
                    <span className="block text-[11px] text-zinc-600">{formatSize(script.size)}</span>
                  </span>
                </button>
                <button
                  type="button"
                  aria-label={`Open ${script.name} in your editor`}
                  title="Open in your editor"
                  onClick={() => void run(() => pythonApi.openInEditor(script.name))}
                  className="rounded p-1 text-zinc-500 opacity-60 transition hover:bg-zinc-700/60 hover:text-zinc-200 group-hover:opacity-100 focus-visible:opacity-100"
                >
                  <ExternalLink size={12} strokeWidth={1.9} />
                </button>
                <button
                  type="button"
                  aria-label={`Delete ${script.name}`}
                  title="Delete"
                  onClick={() => void remove(script.name)}
                  className="rounded p-1 text-zinc-500 opacity-60 transition hover:bg-red-500/15 hover:text-red-400 group-hover:opacity-100 focus-visible:opacity-100"
                >
                  <Trash2 size={12} strokeWidth={1.9} />
                </button>
              </div>
            );
          })
        )}
      </div>
    </aside>
  );
}
