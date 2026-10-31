import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle } from "lucide-react";
import { useTr } from "../i18n/text";
import { useBackdropClose } from "../hooks/useBackdropClose";

interface OutdatedVersionQuestion {
  versionId: string;
  label?: string | null;
  current: string;
}

type Choice = "switch" | "current" | "open-old";

/**
 * "Abrir versão antiga?" (`outdated-version-confirm`, launch_shared.rs). Relato
 * do Discord (31/10/2026): contas banidas por "using a disallowed version of
 * Roblox". Quando a versão escolhida pela tela Versions ficou para trás da
 * atual, o launch espera esta resposta. Pedido do dono: perguntar sempre e ter
 * um botão que já troca para a atual, para a pessoa não ficar procurando onde.
 * Esc, fundo ou 2 min sem resposta abrem a atual só desta vez.
 */
export function OutdatedVersionConfirm() {
  const t = useTr();
  const [question, setQuestion] = useState<OutdatedVersionQuestion | null>(null);

  useEffect(() => {
    const unlisten = listen<OutdatedVersionQuestion>("outdated-version-confirm", (event) => {
      setQuestion(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const answer = useCallback((choice: Choice) => {
    setQuestion(null);
    void invoke("answer_outdated_version_confirm", { choice }).catch(() => {});
  }, []);

  useEffect(() => {
    if (!question) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        answer("current");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [question, answer]);

  const backdropClose = useBackdropClose(() => answer("current"));

  if (!question) return null;
  const name = question.label ? `${question.label} (${question.versionId})` : question.versionId;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
      {...backdropClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="outdated-version-title"
        className="theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-xl shadow-2xl w-96 overflow-hidden animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 pt-4 pb-3">
          <p id="outdated-version-title" className="flex items-center gap-2 text-[13px] font-medium text-zinc-100">
            <AlertTriangle size={14} className="text-amber-400 shrink-0" />
            {t("This Roblox version is outdated")}
          </p>
          <p className="mt-2 text-[12px] leading-snug text-zinc-300">
            {t("The accounts are set to open {{name}}, but the current Roblox is {{current}}.", {
              name,
              current: question.current,
            })}
          </p>
          <p className="mt-2 text-[12px] leading-snug text-amber-300/90">
            {t("Roblox has banned accounts for using an old version (\"disallowed version of Roblox\").")}
          </p>
        </div>
        <div className="flex flex-col gap-1.5 px-4 pb-4 pt-1">
          <button
            autoFocus
            onClick={() => answer("switch")}
            className="w-full px-3.5 py-[7px] rounded-lg text-[12px] text-white bg-sky-600 hover:bg-sky-500 transition-colors"
          >
            {t("Switch to the current version")}
          </button>
          <button
            onClick={() => answer("current")}
            className="w-full px-3.5 py-[6px] rounded-lg text-[12px] text-zinc-300 bg-zinc-800 hover:bg-zinc-700 transition-colors"
          >
            {t("Use the current version this time")}
          </button>
          <button
            onClick={() => answer("open-old")}
            className="w-full px-3.5 py-[6px] rounded-lg text-[12px] text-red-300 hover:text-red-200 hover:bg-red-950/40 transition-colors"
          >
            {t("Open the old version anyway")}
          </button>
        </div>
      </div>
    </div>
  );
}
