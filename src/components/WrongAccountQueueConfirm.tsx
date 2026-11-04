import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { UserX } from "lucide-react";
import { useTr } from "../i18n/text";
import { useBackdropClose } from "../hooks/useBackdropClose";
import { useStore } from "../store";
import { accountLabel } from "../utils/accountName";

interface WrongAccountEvent {
  userId: number;
  loggedUserId: number;
  queueAsked?: boolean;
}

/**
 * "Continuar abrindo as outras contas?" depois que um cliente entra como outra
 * conta do Roblox (`roblox-client-wrong-account` com `queueAsked`, de
 * client_health.rs). Pedido do dono (31/10/2026): a fila parava sozinha e era
 * preciso pedir as contas de novo. A fila espera a resposta; "não perguntar de
 * novo" grava a escolha em `General.WrongAccountQueue` (Settings › Launch).
 * Esc ou o fundo param a fila, como antes. Nenhum cliente é fechado.
 */
export function WrongAccountQueueConfirm() {
  const t = useTr();
  const store = useStore();
  const [question, setQuestion] = useState<WrongAccountEvent | null>(null);
  const [remember, setRemember] = useState(false);

  useEffect(() => {
    const unlistenAsk = listen<WrongAccountEvent>("roblox-client-wrong-account", (event) => {
      if (event.payload.queueAsked !== true) return;
      // Já perguntando: a fila já está parada esperando, a pergunta fica a mesma.
      setQuestion((current) => current ?? event.payload);
    });
    // A fila acabou (Stop na Sessão, Close All): não há mais o que perguntar.
    const unlistenDone = listen("launch-complete", () => setQuestion(null));
    return () => {
      void unlistenAsk.then((fn) => fn());
      void unlistenDone.then((fn) => fn());
    };
  }, []);

  const answer = useCallback(
    (keepLaunching: boolean) => {
      setQuestion(null);
      setRemember(false);
      void invoke("answer_wrong_account_queue", { keepLaunching, remember }).catch(() => {});
    },
    [remember]
  );

  useEffect(() => {
    if (!question) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        answer(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [question, answer]);

  const backdropClose = useBackdropClose(() => answer(false));

  if (!question) return null;
  const account = store.accounts.find((a) => a.UserID === question.userId);
  const name = accountLabel(account, store, question.userId);

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
      {...backdropClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="wrong-account-queue-title"
        className="theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-xl shadow-2xl w-96 overflow-hidden animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 pt-4 pb-3">
          <p id="wrong-account-queue-title" className="flex items-center gap-2 text-[13px] font-medium text-zinc-100">
            <UserX size={14} className="text-amber-400 shrink-0" />
            {t("Keep launching the other accounts?")}
          </p>
          <p className="mt-2 text-[12px] leading-snug text-zinc-300">
            {t("{{name}} opened as a different Roblox account (user {{id}}).", {
              name,
              id: question.loggedUserId,
            })}
          </p>
          <p className="mt-2 text-[12px] leading-snug text-zinc-400">
            {t("That client stays open. The queue is waiting for your answer.")}
          </p>
          <label className="mt-3 flex items-center gap-2 text-[12px] text-zinc-300 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
              className="accent-sky-500"
            />
            {t("Don't ask again (change it in Settings › Launch)")}
          </label>
        </div>
        <div className="flex flex-col gap-1.5 px-4 pb-4 pt-1">
          <button
            autoFocus
            onClick={() => answer(true)}
            className="w-full px-3.5 py-[7px] rounded-lg text-[12px] text-white bg-sky-600 hover:bg-sky-500 transition-colors"
          >
            {t("Keep launching")}
          </button>
          <button
            onClick={() => answer(false)}
            className="w-full px-3.5 py-[6px] rounded-lg text-[12px] text-zinc-300 bg-zinc-800 hover:bg-zinc-700 transition-colors"
          >
            {t("Stop the queue")}
          </button>
        </div>
      </div>
    </div>
  );
}
