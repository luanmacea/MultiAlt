import { pythonApi, type FsNode } from "./pythonApi";
import { freeCopyName, joinPath } from "./workspaceTree";

/** Copies a read-only file (an example) into `scripts/` under a free name and returns the new path. */
export async function copyFileToScripts(tree: FsNode | null, path: string): Promise<string> {
  const target = joinPath("scripts", freeCopyName(tree, path));
  const text = await pythonApi.fsRead(path);
  await pythonApi.fsCreate(target, "file");
  await pythonApi.fsWrite(target, text);
  return target;
}
