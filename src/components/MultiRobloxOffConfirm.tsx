import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Layers } from "lucide-react";
import { useTr } from "../i18n/text";
import { useBackdropClose } from "../hooks/useBackdropClose";
import { useStore } from "../store";

/**
 * "Multi Roblox está desligado, ligar?" (`multi-roblox-off-confirm`,
 * launch_shared.rs). Pedido do dono (04/11/2026): vários usuários perguntam no
 * Discord por que só uma conta fica no jogo — o Multi Roblox estava desligado.
 * O launch espera a resposta (2 min no máximo); a escolha é gravada aqui, nas
 * settings, e `answer_multi_roblox_off` só solta o launch. "Não perguntar de
 * novo" grava `General.AskEnableMultiRbx` (Settings › Launch). Esc ou o fundo
 * abrem com ele desligado, como antes. Nenhum cliente é fechado.
 */
export function MultiRobloxOffConfirm() {
  const t = useTr();
  const store = useStore();
  const [open, setOpen] = useState(false);
  const [dontAsk, setDontAsk] = useState(false);

  useEffect(() => {
    const unlisten = listen("multi-roblox-off-confirm", () => setOpen(true));
    // A fila acabou (Stop, Close All): não há mais o que perguntar.
    const unlistenDone = listen("launch-complete", () => setOpen(false));
    return () => {
      void unlisten.then((fn) => fn());
      void unlistenDone.then((fn) => fn());
    };
  }, []);

  const answer = useCallback(
    async (turnOn: boolean) => {
      setOpen(false);
      setDontAsk(false);
      try {
        if (turnOn) await store.updateSetting("General", "EnableMultiRbx", "true");
        else if (dontAsk) await store.updateSetting("General", "AskEnableMultiRbx", "false");
      } finally {
        void invoke("answer_multi_roblox_off").catch(() => {});
      }
    },
    [dontAsk, store]
  );

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void answer(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, answer]);

  const backdropClose = useBackdropClose(() => void answer(false));

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
      {...backdropClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="multi-roblox-off-title"
        className="theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-xl shadow-2xl w-96 overflow-hidden animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 pt-4 pb-3">
          <p id="multi-roblox-off-title" className="flex items-center gap-2 text-[13px] font-medium text-zinc-100">
            <Layers size={14} className="text-sky-400 shrink-0" />
            {t("Multi Roblox is off. Turn it on?")}
          </p>
          <p className="mt-2 text-[12px] leading-snug text-zinc-300">
            {t("Without it, Roblox keeps only one account open at a time, so opening more closes the others.")}
          </p>
          <label className="mt-3 flex items-center gap-2 text-[12px] text-zinc-300 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={dontAsk}
              onChange={(e) => setDontAsk(e.target.checked)}
              className="accent-sky-500"
            />
            {t("Don't ask again (change it in Settings › Launch)")}
          </label>
        </div>
        <div className="flex flex-col gap-1.5 px-4 pb-4 pt-1">
          <button
            autoFocus
            onClick={() => void answer(true)}
            className="w-full px-3.5 py-[7px] rounded-lg text-[12px] text-white bg-sky-600 hover:bg-sky-500 transition-colors"
          >
            {t("Turn on Multi Roblox")}
          </button>
          <button
            onClick={() => void answer(false)}
            className="w-full px-3.5 py-[6px] rounded-lg text-[12px] text-zinc-300 bg-zinc-800 hover:bg-zinc-700 transition-colors"
          >
            {t("Keep it off")}
          </button>
        </div>
      </div>
    </div>
  );
}
