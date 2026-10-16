import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../store";
import { useTr } from "../i18n/text";
import { useJoinOnlineWarning } from "./useJoinOnlineWarning";
import { presetAccountIds } from "../utils/presets";
import type { LaunchPresetView } from "../types";

/**
 * O submenu "Launch preset" da bandeja não lança no backend: ele traz a janela
 * à frente e emite `tray-launch-preset`, para o launch rodar o **fluxo normal
 * da tela** — com a pergunta de "conta online" (`useJoinOnlineWarning`), que
 * conhece a presença e evita derrubar a sessão de uma conta que já está num jogo
 * em outro aparelho (Fix 4). Com o app trancado por inatividade o backend recusa
 * e emite `tray-launch-blocked` (Fix 3).
 *
 * Mora num hook, e não na store, porque precisa do aviso de conta online, que é
 * um hook (usa o PromptProvider). Os ouvintes são registrados uma vez; um `ref`
 * com os valores do render atual evita closure velha.
 */
export function useTrayPresetLaunch() {
  const store = useStore();
  const t = useTr();
  const confirmJoinOnline = useJoinOnlineWarning();
  const ref = useRef({ store, t, confirmJoinOnline });
  ref.current = { store, t, confirmJoinOnline };

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const track = (p: Promise<() => void>) => {
      void p.then((fn) => {
        if (cancelled) fn();
        else unlisteners.push(fn);
      });
    };

    track(
      listen<{ presetId: string }>("tray-launch-preset", async (e) => {
        const { store, t, confirmJoinOnline } = ref.current;
        const presetId = e.payload?.presetId;
        if (!presetId) return;
        try {
          const presets = await invoke<LaunchPresetView[]>("get_launch_presets");
          const preset = presets.find((p) => p.id === presetId);
          if (!preset) {
            store.addToast(t("This preset no longer exists."), "warn");
            return;
          }
          const present = presetAccountIds(preset, store.accounts);
          if (present.length === 0) {
            store.addToast(t("This preset has no accounts to launch"), "warn");
            return;
          }
          if (!(await confirmJoinOnline(present))) return;
          const attempt = await store.launchPreset(preset);
          if (attempt === "started") {
            store.addToast(t("Preset {{name}} launched", { name: preset.name }), "success");
          }
        } catch (err) {
          store.addToast(String(err), "error");
        }
      })
    );

    track(
      listen("tray-launch-blocked", () => {
        ref.current.store.addToast(ref.current.t("Unlock MultiAlt first"), "warn");
      })
    );

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);
}
