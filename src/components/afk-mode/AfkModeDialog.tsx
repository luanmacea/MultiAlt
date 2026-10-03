import { useRef } from "react";
import { useModalClose } from "../../hooks/useModalClose";
import { useTr } from "../../i18n/text";
import { useStore, type AfkModeDialogState } from "../../store";
import { AfkModeView } from "./AfkModeView";

/**
 * O Modo AFK como modal — aberto pela barra (aba de cliques), pelo Auto
 * Rejoin da barra de ações/Choose Game/lista de servidores (aba Auto Rejoin) e
 * pelo "Em jogo" do Painel de Sessão (Auto Rejoin com as contas em jogo).
 *
 * O que abriu fica em `store.afkModeDialog`; aqui só a moldura. A última
 * abertura fica guardada para o conteúdo continuar de pé durante a animação de
 * saída, quando a store já voltou a `null`.
 */
export function AfkModeDialog() {
  const t = useTr();
  const store = useStore();
  const state = store.afkModeDialog;
  const open = state !== null;
  const lastRef = useRef<AfkModeDialogState>({ tab: "clicks" });
  if (state) lastRef.current = state;
  const shown = lastRef.current;
  const { visible, closing, handleClose } = useModalClose(open, store.closeAfkMode);

  if (!visible) return null;

  // A chave remonta o conteúdo a cada abertura: cada uma traz a sua aba e as
  // suas contas, e o rascunho do ciclo é relido do INI.
  const openingKey = [
    shown.tab,
    shown.targetUserIds?.join(",") ?? "",
    shown.adoptRunning ? "adopt" : "",
    shown.placeId ?? "",
  ].join("|");

  // `mt-10` + `max-h` de 100vh-64: o topo do modal fica abaixo dos controles
  // da janela (`ModalWindowControls`, fixos no canto), que cobriam o X.
  return (
    <div
      className={`fixed inset-0 z-[70] flex items-center justify-center bg-black/60 backdrop-blur-sm ${
        closing ? "animate-fade-out" : "animate-fade-in"
      }`}
      onClick={handleClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("AFK Mode")}
        className={`theme-modal-scope theme-panel theme-border rounded-2xl border shadow-2xl flex flex-col overflow-hidden w-[1120px] h-[740px] max-w-[calc(100vw-24px)] max-h-[calc(100vh-64px)] mt-10 ${
          closing ? "animate-scale-out" : "animate-scale-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <AfkModeView
          key={openingKey}
          variant="modal"
          initialTab={shown.tab}
          targetUserIds={shown.targetUserIds}
          adoptRunning={shown.adoptRunning}
          initialPlaceId={shown.placeId ?? null}
          onClose={handleClose}
        />
      </div>
    </div>
  );
}
