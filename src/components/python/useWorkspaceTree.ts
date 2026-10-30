import { useCallback, useEffect, useRef, useState } from "react";
import { errorText, pythonApi, type FsNode } from "./pythonApi";
import { treeSignature } from "./workspaceTree";

/** How often the workspace is re-read while the screen is visible (AI tools edit files from outside). */
export const TREE_POLL_MS = 2000;

/** The workspace tree, polled while `active` and the window is visible. */
export function useWorkspaceTree(active: boolean): {
  tree: FsNode | null;
  loaded: boolean;
  error: string | null;
  refresh: () => Promise<void>;
} {
  const [tree, setTree] = useState<FsNode | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const signature = useRef("");

  const refresh = useCallback(async () => {
    try {
      const next = await pythonApi.fsTree();
      if (next && typeof next === "object") {
        const sig = treeSignature(next);
        if (sig !== signature.current) {
          signature.current = sig;
          setTree(next);
        }
        setError(null);
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "hidden") void refresh();
    }, TREE_POLL_MS);
    return () => window.clearInterval(timer);
  }, [active, refresh]);

  return { tree, loaded, error, refresh };
}
