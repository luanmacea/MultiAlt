import { useCallback, useEffect, useState } from "react";
import {
  PYTHON_EVENTS,
  errorText,
  pythonApi,
  type PythonInstallProgress,
  type PythonRuntimeStatus,
} from "./pythonApi";
import { useTauriEvent } from "./useTauriEvent";

export interface PythonRuntimeState {
  /** `null` until the first answer from the backend. */
  status: PythonRuntimeStatus | null;
  progress: PythonInstallProgress | null;
  error: string | null;
  installing: boolean;
  install: () => Promise<void>;
  reinstall: () => Promise<void>;
  refresh: () => Promise<void>;
}

/** Runtime status plus the install progress events. Nothing downloads without `install()`. */
export function usePythonRuntime(): PythonRuntimeState {
  const [status, setStatus] = useState<PythonRuntimeStatus | null>(null);
  const [progress, setProgress] = useState<PythonInstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const next = await pythonApi.runtimeStatus();
      if (next) setStatus(next);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useTauriEvent<PythonInstallProgress>(PYTHON_EVENTS.installProgress, (next) => {
    setProgress(next);
    if (next.stage === "error") {
      setError(next.message);
      setBusy(false);
    } else if (next.stage === "ready") {
      setError(null);
      setBusy(false);
      void refresh();
    }
  });

  const run = useCallback(
    async (action: () => Promise<void>) => {
      setError(null);
      setBusy(true);
      setProgress({ stage: "python", message: "Starting...", downloaded: 0, total: 0 });
      try {
        await action();
      } catch (e) {
        setError(errorText(e));
        setBusy(false);
        return;
      }
      setBusy(false);
      await refresh();
    },
    [refresh]
  );

  const install = useCallback(() => run(() => pythonApi.installRuntime()), [run]);
  const reinstall = useCallback(() => run(() => pythonApi.reinstallRuntime()), [run]);

  return {
    status,
    progress,
    error,
    installing: busy || status?.installing === true,
    install,
    reinstall,
    refresh,
  };
}
