import { useCallback, useEffect, useState } from "react";
import { pythonApi, type PythonScriptFile } from "./pythonApi";

/** How often the script folder is re-read while the screen is visible. */
export const SCRIPT_POLL_MS = 2000;

/**
 * The `.py` files of the workspace. The folder is polled because the user (or
 * an AI tool) edits the files from outside the app.
 */
export function usePythonScripts(): {
  scripts: PythonScriptFile[];
  loaded: boolean;
  error: string | null;
  refresh: () => Promise<void>;
} {
  const [scripts, setScripts] = useState<PythonScriptFile[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await pythonApi.listScripts();
      if (Array.isArray(list)) {
        setScripts([...list].sort((a, b) => a.name.localeCompare(b.name)));
        setError(null);
      }
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : "Could not read the scripts folder");
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "hidden") void refresh();
    }, SCRIPT_POLL_MS);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return { scripts, loaded, error, refresh };
}
