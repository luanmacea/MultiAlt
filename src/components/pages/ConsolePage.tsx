import { useTr } from "../../i18n/text";
import { LaunchConsole } from "../console/LaunchConsole";
import { PageShell } from "./PageShell";

/**
 * Página Console: o histórico do que o app fez — launch, Auto Rejoin, Watcher
 * e erros. Era a aba Console da Choose Game, que só aparecia com contas
 * selecionadas; agora fica na barra lateral, logo abaixo de Session. As listas
 * da sessão (Entrando, Em jogo) não se repetem aqui: moram só na página
 * Session.
 */
export function ConsolePage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  if (!active) return null;
  return (
    <PageShell
      title={t("Console")}
      description={t("What the app did: launch, Auto Rejoin, Watcher and errors, with where each line came from.")}
      onLeave={onLeave}
      dataTour="console-page"
      bodyClassName="flex flex-col gap-4 px-6 py-5 overflow-y-auto"
    >
      <LaunchConsole className="flex-1" />
    </PageShell>
  );
}
