import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useConfirm } from "../hooks/usePrompt";
import { useTr } from "../i18n/text";

interface LaunchAnywayQuestion {
  kind: "admin" | "program" | string;
  programs: string;
}

/**
 * "Abrir mesmo assim?" (`multi-roblox-confirm`, launch_shared.rs). Quando abrir
 * mais uma conta fecharia uma janela que o app não consegue destravar, o launch
 * espera esta resposta em vez de falhar com erro. Pedido do dono (23/10/2026):
 * confirmação é o padrão; quem decide se a janela pode fechar é a pessoa.
 */
export function MultiRobloxConfirm() {
  const confirm = useConfirm();
  const t = useTr();

  useEffect(() => {
    const unlisten = listen<LaunchAnywayQuestion>("multi-roblox-confirm", async (event) => {
      const { kind, programs } = event.payload;
      const message =
        kind === "admin"
          ? t(
              "A Roblox window is running as administrator, so MultiAlt cannot unlock it. Opening another account will close that window."
            )
          : t("{{programs}} looks like a Roblox window that MultiAlt cannot unlock. Opening another account may close it.", {
              programs,
            });
      const allow = await confirm(message, true, {
        confirmLabel: t("Open anyway"),
        note: t("Cancel keeps every open window as it is."),
      });
      await invoke("answer_multi_roblox_confirm", { allow }).catch(() => {});
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [confirm, t]);

  return null;
}
