import { useCallback, useEffect, useState } from "react";
import { PYTHON_EVENTS, pythonApi, type PythonRunInfo } from "./pythonApi";
import { useTauriEvent } from "./useTauriEvent";

/** True while a run can still do something (not finished or failed). */
export function isRunActive(run: PythonRunInfo): boolean {
  return run.state !== "finished" && run.state !== "failed";
}

/** The runs the backend knows, newest first, kept fresh by `python-run-update`. */
export function usePythonRuns(): { runs: PythonRunInfo[]; refresh: () => Promise<void> } {
  const [runs, setRuns] = useState<PythonRunInfo[]>([]);

  const refresh = useCallback(async () => {
    try {
      const list = await pythonApi.listRuns();
      if (Array.isArray(list)) setRuns([...list].sort((a, b) => b.startedMs - a.startedMs));
    } catch {
      // The list is cosmetic: events keep it fresh.
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useTauriEvent<PythonRunInfo>(PYTHON_EVENTS.runUpdate, (run) => {
    setRuns((prev) => {
      const rest = prev.filter((item) => item.id !== run.id);
      return [run, ...rest].sort((a, b) => b.startedMs - a.startedMs);
    });
  });

  return { runs, refresh };
}
