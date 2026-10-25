import type { FsNode } from "./pythonApi";

/** Top-level folders the user can create files in. Same list as the backend. */
export const WRITABLE_ROOTS = ["scripts", "lib", "assets"] as const;

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp"]);

export function baseName(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? path : path.slice(at + 1);
}

export function parentPath(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

export function joinPath(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name;
}

export function extensionOf(path: string): string {
  const name = baseName(path);
  const at = name.lastIndexOf(".");
  return at <= 0 ? "" : name.slice(at + 1).toLowerCase();
}

export function isImagePath(path: string): boolean {
  return IMAGE_EXTENSIONS.has(extensionOf(path));
}

export function findNode(root: FsNode | null, path: string): FsNode | null {
  if (!root) return null;
  if (root.path === path) return root;
  for (const child of root.children ?? []) {
    if (path === child.path || path.startsWith(`${child.path}/`)) {
      const found = findNode(child, path);
      if (found) return found;
    }
  }
  return null;
}

/** Every file of the tree, depth first. */
export function listFiles(root: FsNode | null): FsNode[] {
  if (!root) return [];
  if (root.kind === "file") return [root];
  return (root.children ?? []).flatMap(listFiles);
}

/** A top-level folder (`scripts`, `lib`...): it can hold files but cannot be renamed or deleted. */
export function isTopLevelDir(node: FsNode): boolean {
  return node.kind === "dir" && node.path !== "" && !node.path.includes("/");
}

export function canRenameOrDelete(node: FsNode): boolean {
  return node.path !== "" && node.writable && !node.pinned && !isTopLevelDir(node);
}

/** The folder a new file or folder goes into for the current selection. */
export function targetDirFor(root: FsNode | null, selectedPath: string | null): string {
  const fallback = "scripts";
  if (!selectedPath) return fallback;
  const node = findNode(root, selectedPath);
  if (!node) return fallback;
  const dir = node.kind === "dir" ? node : findNode(root, parentPath(node.path));
  return dir && dir.writable && dir.path !== "" ? dir.path : fallback;
}

/** `farm` -> `farm.py` inside `scripts/` and `lib/`; other folders keep the name as typed. */
export function withDefaultExtension(dir: string, name: string): string {
  const top = dir.split("/")[0];
  const trimmed = name.trim();
  if ((top === "scripts" || top === "lib") && trimmed && !trimmed.includes(".")) return `${trimmed}.py`;
  return trimmed;
}

/** Same shape check as the backend for one path segment (the backend has the last word). */
export function isValidSegment(name: string): boolean {
  return /^[A-Za-z0-9 _\-.]{1,64}$/.test(name) && !name.startsWith(".");
}

export const SEGMENT_HELP = "Use letters, numbers, spaces, - _ and . (up to 64 characters, not starting with a dot).";

/** A name for `source` inside `scripts/` that does not exist yet: `farm.py`, `farm_copy.py`, `farm_copy2.py`... */
export function freeCopyName(root: FsNode | null, source: string): string {
  const taken = new Set((findNode(root, "scripts")?.children ?? []).map((node) => node.name.toLowerCase()));
  const name = baseName(source);
  if (!taken.has(name.toLowerCase())) return name;
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const ext = dot > 0 ? name.slice(dot) : "";
  for (let n = 1; n < 1000; n += 1) {
    const candidate = `${stem}_copy${n === 1 ? "" : n}${ext}`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
  return `${stem}_copy${Date.now()}${ext}`;
}

/** A cheap signature of the tree, to skip a render when the poll found nothing new. */
export function treeSignature(root: FsNode | null): string {
  if (!root) return "";
  const parts: string[] = [];
  const walk = (node: FsNode) => {
    parts.push(
      `${node.path}|${node.kind}|${node.size}|${node.modifiedMs}|${node.writable ? 1 : 0}${node.pinned ? 1 : 0}${node.runnable ? 1 : 0}`
    );
    for (const child of node.children ?? []) walk(child);
  };
  walk(root);
  return parts.join("\n");
}
