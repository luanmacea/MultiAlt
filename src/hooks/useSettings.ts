import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

export type UseSettingsReturn = ReturnType<typeof useSettings>;

export function useSettings() {
  const [settings, setSettings] = useState<Record<string, Record<string, string>>>({});
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const saveTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);
  const flushInFlight = useRef(false);
  const pending = useRef<Map<string, { section: string; key: string; value: string }>>(new Map());

  const flushPending = useCallback(async () => {
    if (flushInFlight.current) return;
    flushInFlight.current = true;
    let savedAny = false;
    try {
      while (pending.current.size > 0) {
        const batch = [...pending.current.values()];
        pending.current.clear();
        await Promise.all(
          batch.map(async ({ section, key, value }) => {
            await invoke("update_setting", { section, key, value });
          })
        );
        savedAny = true;
      }
    } catch {
    } finally {
      flushInFlight.current = false;
      if (pending.current.size > 0) {
        if (saveTimeout.current) clearTimeout(saveTimeout.current);
        saveTimeout.current = setTimeout(() => {
          void flushPending();
        }, 120);
        return;
      }
      setSaving(false);
      if (savedAny) {
        window.dispatchEvent(
          new CustomEvent("ram-action-status", {
            detail: {
              message: "Settings saved",
              tone: "success",
              timeoutMs: 2200,
            },
          })
        );
      }
    }
  }, []);

  const load = useCallback(async () => {
    try {
      const result = await invoke<Record<string, Record<string, string>>>("get_all_settings");
      setSettings(result);
      setLoaded(true);
    } catch {}
  }, []);

  const get = useCallback(
    (section: string, key: string, fallback = ""): string => {
      return settings[section]?.[key] ?? fallback;
    },
    [settings]
  );

  const getBool = useCallback(
    (section: string, key: string): boolean => {
      return get(section, key) === "true";
    },
    [get]
  );

  const getNumber = useCallback(
    (section: string, key: string, fallback = 0): number => {
      const val = get(section, key);
      const n = parseFloat(val);
      return isNaN(n) ? fallback : n;
    },
    [get]
  );

  const set = useCallback(
    async (section: string, key: string, value: string) => {
      setSettings((prev) => ({
        ...prev,
        [section]: { ...prev[section], [key]: value },
      }));

      pending.current.set(`${section}::${key}`, { section, key, value });
      setSaving(true);

      if (saveTimeout.current) clearTimeout(saveTimeout.current);
      saveTimeout.current = setTimeout(() => {
        void flushPending();
      }, 160);
    },
    [flushPending]
  );

  const setBool = useCallback(
    (section: string, key: string, value: boolean) => {
      set(section, key, value ? "true" : "false");
    },
    [set]
  );

  const setNumber = useCallback(
    (section: string, key: string, value: number) => {
      set(section, key, value.toString());
    },
    [set]
  );

  useEffect(() => {
    return () => {
      if (saveTimeout.current) clearTimeout(saveTimeout.current);
      if (pending.current.size > 0) {
        void flushPending();
      }
    };
  }, [flushPending]);

  return { settings, loaded, saving, load, get, getBool, getNumber, set, setBool, setNumber };
}
