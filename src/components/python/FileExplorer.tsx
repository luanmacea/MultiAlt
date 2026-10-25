import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import {
  ChevronDown,
  ChevronRight,
  Eye,
  File,
  FileCode2,
  FileImage,
  FilePlus,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  Lock,
  Pencil,
  Trash2,
} from "lucide-react";
import { useConfirm, usePrompt } from "../../hooks/usePrompt";
import { copyText } from "./copyText";
import { errorText, pythonApi, type FsNode } from "./pythonApi";
import { copyFileToScripts } from "./workspaceActions";
import {
  SEGMENT_HELP,
  canRenameOrDelete,
  extensionOf,
  findNode,
  isImagePath,
  isValidSegment,
  joinPath,
  parentPath,
  targetDirFor,
  withDefaultExtension,
} from "./workspaceTree";

export const EXPLORER_OPEN_KEY = "ram_python_explorer_open";
const DEFAULT_OPEN = ["scripts", "lib"];

const STARTER_SCRIPT = `from multialt import sleep, stopped


def main(account):
    """Runs once per account."""
    while not stopped():
        with account.focused():
            account.press("Space")
        sleep(30)
`;

type Tone = "info" | "success" | "warn" | "error";

export function deleteConfirmText(name: string): string {
  return `Move ${name} to the workspace trash?`;
}

function loadOpen(): Set<string> {
  try {
    const raw = window.localStorage.getItem(EXPLORER_OPEN_KEY);
    if (raw) {
      const list: unknown = JSON.parse(raw);
      if (Array.isArray(list)) return new Set(list.filter((item): item is string => typeof item === "string"));
    }
  } catch {
    // No storage: the defaults are fine.
  }
  return new Set(DEFAULT_OPEN);
}

function saveOpen(open: Set<string>): void {
  try {
    window.localStorage.setItem(EXPLORER_OPEN_KEY, JSON.stringify([...open]));
  } catch {
    // Not remembered; the tree still works.
  }
}

function FileIcon({ node }: { node: FsNode }) {
  const ext = extensionOf(node.path);
  if (isImagePath(node.path)) return <FileImage size={13} strokeWidth={1.8} className="shrink-0 text-violet-300" />;
  if (ext === "py") return <FileCode2 size={13} strokeWidth={1.8} className="shrink-0 text-sky-300" />;
  if (ext === "md" || ext === "txt" || ext === "json") {
    return <FileText size={13} strokeWidth={1.8} className="shrink-0 text-zinc-400" />;
  }
  return <File size={13} strokeWidth={1.8} className="shrink-0 text-zinc-400" />;
}

interface MenuState {
  x: number;
  y: number;
  node: FsNode;
}

interface FileExplorerProps {
  tree: FsNode | null;
  loaded: boolean;
  activePath: string | null;
  onOpen: (path: string) => void;
  /** Re-reads the workspace right away (after a create, rename or delete). */
  onChanged: () => Promise<void>;
  onRenamed: (from: string, to: string) => void;
  onRemoved: (path: string) => void;
  onToast: (message: string, tone?: Tone) => void;
}

/** The Explorer side of the Code tab: the workspace tree with file actions. */
export function FileExplorer({
  tree,
  loaded,
  activePath,
  onOpen,
  onChanged,
  onRenamed,
  onRemoved,
  onToast,
}: FileExplorerProps) {
  const prompt = usePrompt();
  const confirm = useConfirm();
  const [open, setOpen] = useState<Set<string>>(loadOpen);
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    function onDown(event: globalThis.MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) setMenu(null);
    }
    function onKey(event: globalThis.KeyboardEvent) {
      if (event.key === "Escape") setMenu(null);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  function toggle(path: string) {
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      saveOpen(next);
      return next;
    });
  }

  function expand(path: string) {
    setOpen((prev) => {
      if (prev.has(path)) return prev;
      const next = new Set(prev).add(path);
      saveOpen(next);
      return next;
    });
  }

  async function guard(action: () => Promise<void>) {
    try {
      await action();
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  const effective = selected ?? activePath;

  async function newEntry(kind: "file" | "dir", dirOverride?: string) {
    setMenu(null);
    const dir = dirOverride ?? targetDirFor(tree, effective);
    const label = kind === "file" ? "file" : "folder";
    const raw = await prompt(
      `Name of the new ${label} in ${dir}/ (for example ${kind === "file" ? "farm.py" : "helpers"})`,
      "",
      { maxLength: 64 }
    );
    if (raw === null) return;
    const name = kind === "file" ? withDefaultExtension(dir, raw) : raw.trim();
    if (!isValidSegment(name)) {
      onToast(SEGMENT_HELP, "warn");
      return;
    }
    const path = joinPath(dir, name);
    await guard(async () => {
      await pythonApi.fsCreate(path, kind);
      if (kind === "file" && dir === "scripts" && extensionOf(name) === "py") {
        await pythonApi.fsWrite(path, STARTER_SCRIPT);
      }
      expand(dir);
      await onChanged();
      setSelected(path);
      if (kind === "file") onOpen(path);
    });
  }

  async function rename(node: FsNode) {
    setMenu(null);
    if (!canRenameOrDelete(node)) return;
    const raw = await prompt(`New name for ${node.name}`, node.name, { maxLength: 64 });
    if (raw === null) return;
    const name = raw.trim();
    if (!name || name === node.name) return;
    if (!isValidSegment(name)) {
      onToast(SEGMENT_HELP, "warn");
      return;
    }
    const to = joinPath(parentPath(node.path), name);
    await guard(async () => {
      await pythonApi.fsRename(node.path, to);
      onRenamed(node.path, to);
      setSelected(to);
      await onChanged();
    });
  }

  async function remove(node: FsNode) {
    setMenu(null);
    if (!canRenameOrDelete(node)) return;
    const ok = await confirm(deleteConfirmText(node.name), true, { confirmLabel: "Move to trash" });
    if (!ok) return;
    await guard(async () => {
      await pythonApi.fsDelete(node.path);
      onRemoved(node.path);
      if (selected === node.path) setSelected(null);
      await onChanged();
    });
  }

  async function copyToScripts(node: FsNode) {
    setMenu(null);
    await guard(async () => {
      const path = await copyFileToScripts(tree, node.path);
      await onChanged();
      setSelected(path);
      onOpen(path);
      onToast(`Copied to ${path}.`, "success");
    });
  }

  function openMenu(event: MouseEvent, node: FsNode) {
    event.preventDefault();
    setSelected(node.path);
    setMenu({ x: event.clientX, y: event.clientY, node });
  }

  function activate(node: FsNode) {
    setSelected(node.path);
    if (node.kind === "dir") toggle(node.path);
    else onOpen(node.path);
  }

  function onRowKey(event: KeyboardEvent, node: FsNode) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate(node);
    } else if (event.key === "F2") {
      event.preventDefault();
      void rename(node);
    } else if (event.key === "Delete") {
      event.preventDefault();
      void remove(node);
    }
  }

  function renderNode(node: FsNode, depth: number) {
    const isDir = node.kind === "dir";
    const isOpen = isDir && open.has(node.path);
    const isActive = !isDir && node.path === activePath;
    const isSelected = node.path === selected;
    const readOnly = !node.writable && !node.pinned && !isDir;
    return (
      <li key={node.path} role="none">
        <div
          role="treeitem"
          aria-level={depth + 1}
          aria-expanded={isDir ? isOpen : undefined}
          aria-selected={isActive || isSelected}
          tabIndex={0}
          title={node.path}
          onClick={() => activate(node)}
          onKeyDown={(event) => onRowKey(event, node)}
          onContextMenu={(event) => openMenu(event, node)}
          style={{ paddingLeft: 8 + depth * 14 }}
          className={`flex cursor-pointer items-center gap-1.5 py-[3px] pr-2 text-[12px] ${
            isActive
              ? "bg-sky-500/15 text-zinc-100"
              : isSelected
                ? "bg-zinc-800/70 text-zinc-100"
                : "text-zinc-300 hover:bg-zinc-800/40"
          }`}
        >
          {isDir ? (
            <>
              {isOpen ? (
                <ChevronDown size={12} strokeWidth={2} className="shrink-0 text-zinc-500" />
              ) : (
                <ChevronRight size={12} strokeWidth={2} className="shrink-0 text-zinc-500" />
              )}
              {isOpen ? (
                <FolderOpen size={13} strokeWidth={1.8} className="shrink-0 text-amber-300" />
              ) : (
                <Folder size={13} strokeWidth={1.8} className="shrink-0 text-amber-300" />
              )}
            </>
          ) : (
            <>
              <span className="w-3 shrink-0" />
              <FileIcon node={node} />
            </>
          )}
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
          {node.pinned ? (
            <Lock size={11} strokeWidth={2} role="img" aria-label="Locked default block" className="shrink-0 text-zinc-500" />
          ) : null}
          {readOnly ? (
            <Eye size={11} strokeWidth={2} role="img" aria-label="Read-only" className="shrink-0 text-zinc-500" />
          ) : null}
        </div>
        {isDir && isOpen && node.children?.length ? (
          <ul role="group">{node.children.map((child) => renderNode(child, depth + 1))}</ul>
        ) : null}
      </li>
    );
  }

  const selectedNode = effective ? findNode(tree, effective) : null;
  const canChange = selectedNode ? canRenameOrDelete(selectedNode) : false;
  const iconButton =
    "rounded p-1 text-zinc-400 transition hover:bg-zinc-700/60 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-35 disabled:hover:bg-transparent";
  const menuNode = menu?.node ?? null;

  return (
    <aside aria-label="Explorer" className="flex min-h-0 min-w-0 flex-col border-r theme-border bg-[var(--panel-soft)]">
      <div className="flex shrink-0 items-center gap-0.5 border-b theme-border px-2 py-1">
        <span className="mr-auto text-[11px] font-medium uppercase tracking-wide text-zinc-400">Explorer</span>
        <button type="button" aria-label="New file" title="New file" onClick={() => void newEntry("file")} className={iconButton}>
          <FilePlus size={13} strokeWidth={1.9} />
        </button>
        <button type="button" aria-label="New folder" title="New folder" onClick={() => void newEntry("dir")} className={iconButton}>
          <FolderPlus size={13} strokeWidth={1.9} />
        </button>
        <button
          type="button"
          aria-label="Rename"
          title="Rename (F2)"
          disabled={!canChange}
          onClick={() => selectedNode && void rename(selectedNode)}
          className={iconButton}
        >
          <Pencil size={13} strokeWidth={1.9} />
        </button>
        <button
          type="button"
          aria-label="Delete"
          title="Delete (moves to the trash)"
          disabled={!canChange}
          onClick={() => selectedNode && void remove(selectedNode)}
          className={iconButton}
        >
          <Trash2 size={13} strokeWidth={1.9} />
        </button>
        <button
          type="button"
          aria-label="Open workspace folder"
          title="Open the workspace folder"
          onClick={() => void guard(() => pythonApi.openWorkspaceFolder())}
          className={iconButton}
        >
          <FolderOpen size={13} strokeWidth={1.9} />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {!loaded ? (
          <div className="px-3 py-3 text-[12px] text-zinc-500">Loading files...</div>
        ) : !tree?.children?.length ? (
          <div className="px-3 py-4 text-[12px] leading-relaxed text-zinc-500">
            The workspace is empty. Use <span className="text-zinc-300">New file</span> to start.
          </div>
        ) : (
          <ul role="tree" aria-label="Workspace files">
            {tree.children.map((child) => renderNode(child, 0))}
          </ul>
        )}
      </div>

      {menu && menuNode ? (
        <div
          ref={menuRef}
          role="menu"
          aria-label={`Actions for ${menuNode.name}`}
          style={{ left: menu.x, top: menu.y }}
          className="theme-modal-scope theme-panel theme-border fixed z-50 w-[210px] overflow-hidden rounded-lg border py-1 shadow-2xl"
        >
          {menuNode.kind === "file" ? (
            <MenuItem
              label="Open"
              onClick={() => {
                setMenu(null);
                onOpen(menuNode.path);
              }}
            />
          ) : null}
          {menuNode.kind === "dir" && menuNode.writable && menuNode.path !== "" ? (
            <>
              <MenuItem label="New file here" onClick={() => void newEntry("file", menuNode.path)} />
              <MenuItem label="New folder here" onClick={() => void newEntry("dir", menuNode.path)} />
            </>
          ) : null}
          {menuNode.kind === "file" && !menuNode.writable && !menuNode.pinned && extensionOf(menuNode.path) === "py" ? (
            <MenuItem label="Copy to scripts" onClick={() => void copyToScripts(menuNode)} />
          ) : null}
          <MenuItem label="Rename" disabled={!canRenameOrDelete(menuNode)} onClick={() => void rename(menuNode)} />
          <MenuItem label="Delete" disabled={!canRenameOrDelete(menuNode)} onClick={() => void remove(menuNode)} />
          <MenuItem
            label="Copy path"
            onClick={() => {
              setMenu(null);
              void copyText(menuNode.path);
            }}
          />
        </div>
      ) : null}
    </aside>
  );
}

function MenuItem({ label, onClick, disabled }: { label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
      className="block w-full px-3 py-1.5 text-left text-[12px] text-zinc-200 hover:bg-zinc-800/70 disabled:cursor-not-allowed disabled:text-zinc-600 disabled:hover:bg-transparent"
    >
      {label}
    </button>
  );
}
