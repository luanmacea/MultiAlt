import { useEffect, useRef } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { ToastTone } from "../utils/toastTone";
import { consumeUpdateHandoff } from "../updateHandoff";

type Translate = (text: string, options?: Record<string, unknown>) => string;

/**
 * Ao abrir, confere se esta abertura é a volta de uma atualização (ver
 * `updateHandoff.ts`) e avisa: "Atualizado para vX ✨", ou que a instalação não
 * terminou. Roda uma vez, quando o app já passou da tela de senha.
 */
export function useUpdateHandoffToast(
  ready: boolean,
  addToast: (msg: string, tone?: ToastTone) => void,
  t: Translate
): void {
  const checkedRef = useRef(false);

  useEffect(() => {
    if (!ready || checkedRef.current) return;
    checkedRef.current = true;

    let cancelled = false;
    void getVersion()
      .then((current: unknown) => {
        if (cancelled || typeof current !== "string" || !current) return;
        const result = consumeUpdateHandoff(localStorage, current, Date.now());
        if (result?.kind === "updated") {
          addToast(t("Updated to v{{version}} ✨", { version: result.version }), "success");
        } else if (result?.kind === "failed") {
          addToast(
            t("The update to v{{version}} didn't finish. Check for updates to try again.", {
              version: result.version,
            }),
            "warn"
          );
        }
      })
      .catch(() => {
        // Sem versão não há o que comparar; a anotação fica para a próxima abertura.
      });

    return () => {
      cancelled = true;
    };
  }, [ready, addToast, t]);
}
