import { useCallback, useEffect, useRef, useState } from "react";
import { AlertTriangle, FileCode2, FileImage, Lock, Save, X } from "lucide-react";
import { useConfirm } from "../../hooks/usePrompt";
import { BottomPanel } from "./BottomPanel";
import { CodeEditor } from "./CodeEditor";
import { FileExplorer } from "./FileExplorer";
import { RunControls } from "./RunControls";
import { errorText, pythonApi, type FsNode, type PythonRunInfo } from "./pythonApi";
import { copyFileToScripts } from "./workspaceActions";
import { baseName, extensionOf, findNode, isImagePath } from "./workspaceTree";
import { useWorkspaceTree } from "./useWorkspaceTree";

type Tone = "info" | "success" | "warn" | "error";

interface OpenFile {
  path: string;
  kind: "text" | "image";
  text: string;
  saved: string;
  image: string | null;
  /** `modifiedMs` of the version we hold; a different value in the tree means the disk changed. */
  loadedMs: number;
  loading: boolean;
  error: string | null;
  notice: string | null;
  /** The disk version, offered when it changed while there are unsaved edits. */
  conflict: string | null;
  missing: boolean;
}

export interface OpenRequest {
  path: string;
  /** Changes on every request, so the same file can be requested twice. */
  seq: number;
}

interface CodeTabProps {
  /** False while another tab is showing: the tab stays mounted (open files keep their edits) but stops polling. */
  visible: boolean;
  openRequest: OpenRequest | null;
  runtimeInstalled: boolean;
  runs: PythonRunInfo[];
  developerMode: boolean;
  onOpenDeveloper: () => void;
  onToast: (message: string, tone?: Tone) => void;
}

export function discardConfirmText(name: string): string {
  return `${name} has unsaved changes. Close it and discard them?`;
}

function isDirty(file: OpenFile): boolean {
  return file.kind === "text" && file.text !== file.saved;
}

/** The Code tab: Explorer, open-file tabs, the editor and the Terminal/Running panel. */
export function CodeTab({ visible, openRequest, runtimeInstalled, runs, developerMode, onOpenDeveloper, onToast }: CodeTabProps) {
  const confirm = useConfirm();
  const { tree, loaded, refresh } = useWorkspaceTree(visible);
  const [files, setFiles] = useState<OpenFile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const filesRef = useRef(files);
  filesRef.current = files;
  const treeRef = useRef(tree);
  treeRef.current = tree;

  const patch = useCallback((path: string, change: Partial<OpenFile>) => {
    setFiles((prev) => prev.map((file) => (file.path === path ? { ...file, ...change } : file)));
  }, []);

  const openFile = useCallback(
    async (path: string) => {
      setActive(path);
      if (filesRef.current.some((file) => file.path === path)) return;
      const image = isImagePath(path);
      const loadedMs = findNode(treeRef.current, path)?.modifiedMs ?? 0;
      const placeholder: OpenFile = {
        path,
        kind: image ? "image" : "text",
        text: "",
        saved: "",
        image: null,
        loadedMs,
        loading: true,
        error: null,
        notice: null,
        conflict: null,
        missing: false,
      };
      // Keep the ref current right away: two quick requests must not open the same file twice.
      filesRef.current = [...filesRef.current, placeholder];
      setFiles(filesRef.current);
      try {
        if (image) {
          const url = await pythonApi.fsReadImage(path);
          patch(path, { image: typeof url === "string" ? url : null, loading: false });
        } else {
          const text = await pythonApi.fsRead(path);
          const content = typeof text === "string" ? text : "";
          patch(path, { text: content, saved: content, loading: false });
        }
      } catch (e) {
        // Binary or too large: nothing to edit, so do not leave an empty tab behind.
        setFiles((prev) => prev.filter((file) => file.path !== path));
        setActive((current) => (current === path ? (filesRef.current.find((file) => file.path !== path)?.path ?? null) : current));
        onToast(`${baseName(path)}: ${errorText(e)}`, "warn");
      }
    },
    [onToast, patch]
  );

  const requestSeq = openRequest?.seq;
  useEffect(() => {
    if (openRequest) void openFile(openRequest.path);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestSeq]);

  // Files touched from outside (an editor, an AI tool): reload clean ones, flag the ones with edits.
  useEffect(() => {
    if (!tree) return;
    for (const file of filesRef.current) {
      if (file.loading) continue;
      const node = findNode(tree, file.path);
      if (!node) {
        if (!file.missing) patch(file.path, { missing: true });
        continue;
      }
      if (file.missing) patch(file.path, { missing: false });
      if (node.modifiedMs === file.loadedMs) continue;
      // Claim this version first so the next poll does not read it again.
      patch(file.path, { loadedMs: node.modifiedMs });
      if (file.kind === "image") {
        pythonApi
          .fsReadImage(file.path)
          .then((url) => typeof url === "string" && patch(file.path, { image: url }))
          .catch(() => {});
        continue;
      }
      pythonApi
        .fsRead(file.path)
        .then((disk) => {
          const now = filesRef.current.find((item) => item.path === file.path);
          if (!now || typeof disk !== "string" || disk === now.saved) return;
          if (now.text === now.saved) patch(file.path, { text: disk, saved: disk, notice: "Changed on disk - reloaded" });
          else patch(file.path, { conflict: disk });
        })
        .catch(() => {});
    }
  }, [tree, patch]);

  const activeFile = files.find((file) => file.path === active) ?? null;
  const activeNode = activeFile ? findNode(tree, activeFile.path) : null;
  const readOnly = activeNode ? !activeNode.writable : false;

  async function save(file: OpenFile) {
    if (file.kind !== "text" || file.text === file.saved) return;
    const text = file.text;
    try {
      await pythonApi.fsWrite(file.path, text);
      patch(file.path, { saved: text, conflict: null, notice: null, error: null });
    } catch (e) {
      patch(file.path, { error: errorText(e) });
    }
  }

  const saveActive = () => {
    const file = filesRef.current.find((item) => item.path === active);
    if (file) void save(file);
  };
  const saveActiveRef = useRef(saveActive);
  saveActiveRef.current = saveActive;

  function closeNow(path: string) {
    const index = filesRef.current.findIndex((file) => file.path === path);
    const remaining = filesRef.current.filter((file) => file.path !== path);
    filesRef.current = remaining;
    setFiles(remaining);
    setActive((current) => (current === path ? (remaining[Math.min(index, remaining.length - 1)]?.path ?? null) : current));
  }

  async function close(file: OpenFile) {
    if (isDirty(file)) {
      const ok = await confirm(discardConfirmText(baseName(file.path)), true, { confirmLabel: "Discard changes" });
      if (!ok) return;
    }
    closeNow(file.path);
  }

  function onRenamed(from: string, to: string) {
    const moved = (path: string) => (path === from ? to : path.startsWith(`${from}/`) ? `${to}${path.slice(from.length)}` : path);
    setFiles((prev) => prev.map((file) => ({ ...file, path: moved(file.path) })));
    setActive((current) => (current ? moved(current) : current));
  }

  function onRemoved(path: string) {
    for (const file of filesRef.current) {
      if (file.path === path || file.path.startsWith(`${path}/`)) closeNow(file.path);
    }
  }

  async function copyActiveToScripts() {
    if (!activeFile) return;
    try {
      const target = await copyFileToScripts(tree, activeFile.path);
      await refresh();
      void openFile(target);
      onToast(`Copied to ${target}.`, "success");
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  function readOnlyBanner(file: OpenFile, node: FsNode | null) {
    if (!node || node.writable) return null;
    const text = node.pinned
      ? "This default block is locked. Turn on Developer mode in the Developer tab to edit it."
      : file.path.startsWith("examples/")
        ? "This example comes with MultiAlt and cannot be edited. Copy it to scripts/ to make your own version."
        : "This file is read-only.";
    return (
      <div role="note" className="flex flex-wrap items-center gap-2 border-b border-sky-500/30 bg-sky-500/10 px-3 py-1.5 text-[12px] text-sky-200">
        <Lock size={12} strokeWidth={2} className="shrink-0" />
        <span className="min-w-0 flex-1">{text}</span>
        {node.pinned ? (
          <button type="button" className="theme-btn-ghost px-2 py-0.5 text-[11.5px]" onClick={onOpenDeveloper}>
            Open the Developer tab
          </button>
        ) : extensionOf(file.path) === "py" ? (
          <button type="button" className="theme-btn-ghost px-2 py-0.5 text-[11.5px]" onClick={() => void copyActiveToScripts()}>
            Copy to scripts
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-[480px] flex-col overflow-hidden rounded-lg border theme-border">
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(170px,230px)_minmax(0,1fr)]">
        <FileExplorer
          tree={tree}
          loaded={loaded}
          activePath={active}
          onOpen={(path) => void openFile(path)}
          onChanged={refresh}
          onRenamed={onRenamed}
          onRemoved={onRemoved}
          onToast={onToast}
        />

        <section aria-label="Editor" className="flex min-h-0 min-w-0 flex-col">
          {files.length > 0 ? (
            <div role="tablist" aria-label="Open files" className="flex shrink-0 overflow-x-auto border-b theme-border bg-[var(--panel-soft)]">
              {files.map((file) => {
                const on = file.path === active;
                const dirty = isDirty(file);
                const name = baseName(file.path);
                return (
                  <div
                    key={file.path}
                    className={`group flex shrink-0 items-center gap-1.5 border-r theme-border py-1.5 pl-3 pr-1.5 text-[12px] ${
                      on ? "bg-[var(--panel-bg)] text-zinc-100" : "text-zinc-400 hover:bg-zinc-800/40"
                    }`}
                    onAuxClick={(event) => {
                      if (event.button === 1) void close(file);
                    }}
                  >
                    <button
                      type="button"
                      role="tab"
                      aria-selected={on}
                      title={file.path}
                      onClick={() => setActive(file.path)}
                      className="flex items-center gap-1.5"
                    >
                      {file.kind === "image" ? (
                        <FileImage size={12} strokeWidth={1.8} className="text-violet-300" />
                      ) : (
                        <FileCode2 size={12} strokeWidth={1.8} className="text-sky-300" />
                      )}
                      {name}
                    </button>
                    {dirty ? (
                      <span role="img" aria-label="Unsaved changes" className="h-2 w-2 rounded-full bg-amber-300" />
                    ) : null}
                    <button
                      type="button"
                      aria-label={`Close ${name}`}
                      onClick={() => void close(file)}
                      className="rounded p-0.5 text-zinc-500 hover:bg-zinc-700/60 hover:text-zinc-100"
                    >
                      <X size={12} strokeWidth={2} />
                    </button>
                  </div>
                );
              })}
            </div>
          ) : null}

          {activeFile ? (
            <>
              <div className="flex shrink-0 flex-wrap items-center gap-2 border-b theme-border px-3 py-1">
                <span className="min-w-0 truncate font-mono text-[11.5px] text-zinc-400" title={activeFile.path}>
                  {activeFile.path.replaceAll("/", " / ")}
                </span>
                {activeFile.notice ? <span className="text-[11.5px] text-sky-300">{activeFile.notice}</span> : null}
                <div className="ml-auto flex items-center gap-2">
                  {activeFile.kind === "text" && !readOnly ? (
                    <button
                      type="button"
                      onClick={saveActive}
                      disabled={!isDirty(activeFile)}
                      className="theme-btn-ghost inline-flex items-center gap-1.5 px-2 py-1 text-[12px] disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      <Save size={12} strokeWidth={2} />
                      Save
                      <span className="text-[11px] opacity-60">Ctrl+S</span>
                    </button>
                  ) : null}
                  {activeFile.kind === "text" ? (
                    <RunControls
                      script={baseName(activeFile.path)}
                      runnable={activeNode?.runnable === true}
                      runtimeInstalled={runtimeInstalled}
                      runs={runs}
                      onToast={onToast}
                    />
                  ) : null}
                </div>
              </div>

              {activeFile.kind === "text" ? readOnlyBanner(activeFile, activeNode) : null}

              {activeNode?.pinned && activeNode.writable && activeFile.kind === "text" && developerMode ? (
                <div role="note" className="border-b theme-border px-3 py-1.5 text-[12px] text-zinc-400">
                  Default block. Developer mode is on, so you can edit it. Use Restore default in Global functions to undo.
                </div>
              ) : null}

              {activeFile.missing ? (
                <div role="alert" className="border-b border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-[12px] text-amber-200">
                  This file was deleted or moved on disk. Saving writes it again.
                </div>
              ) : null}

              {activeFile.conflict !== null ? (
                <div role="alert" className="flex flex-wrap items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-[12px] text-amber-200">
                  <AlertTriangle size={13} strokeWidth={2} className="shrink-0" />
                  <span className="min-w-0 flex-1">This file changed on disk while you have unsaved edits here.</span>
                  <button
                    type="button"
                    className="theme-btn-ghost px-2 py-0.5 text-[11.5px]"
                    onClick={() => {
                      const disk = activeFile.conflict ?? "";
                      patch(activeFile.path, { text: disk, saved: disk, conflict: null, notice: "Reloaded from disk" });
                    }}
                  >
                    Load the disk version
                  </button>
                  <button
                    type="button"
                    className="theme-btn-ghost px-2 py-0.5 text-[11.5px]"
                    onClick={() => patch(activeFile.path, { conflict: null })}
                  >
                    Keep my edits
                  </button>
                </div>
              ) : null}

              {activeFile.error ? (
                <div role="alert" className="border-b border-red-500/30 bg-red-500/10 px-3 py-1.5 text-[12px] text-red-300">
                  {activeFile.error}
                </div>
              ) : null}

              <div className="min-h-0 flex-1">
                {activeFile.loading ? (
                  <div className="px-4 py-3 text-[12px] text-zinc-500">Loading...</div>
                ) : activeFile.kind === "image" ? (
                  <div className="flex h-full items-center justify-center overflow-auto p-4">
                    {activeFile.image ? (
                      <img
                        src={activeFile.image}
                        alt={baseName(activeFile.path)}
                        className="max-h-full max-w-full rounded border theme-border bg-zinc-900/60 [image-rendering:pixelated]"
                      />
                    ) : (
                      <span className="text-[12px] text-zinc-500">No preview available.</span>
                    )}
                  </div>
                ) : (
                  <CodeEditor
                    docKey={activeFile.path}
                    value={activeFile.text}
                    readOnly={readOnly}
                    ariaLabel={`Source of ${baseName(activeFile.path)}`}
                    onChange={(text) => patch(activeFile.path, { text })}
                    onSave={() => saveActiveRef.current()}
                  />
                )}
              </div>
            </>
          ) : (
            <div className="flex flex-1 items-center justify-center px-6 text-center text-[12.5px] text-zinc-500">
              Open a file from the Explorer to edit it. Scripts live in <span className="mx-1 text-zinc-300">scripts/</span>.
            </div>
          )}
        </section>
      </div>

      <BottomPanel runs={runs} onToast={onToast} />
    </div>
  );
}
