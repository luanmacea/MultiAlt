import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { isWindowsPlatform } from "../../utils/platform";
import { LaunchConsole } from "../console/LaunchConsole";
import { WindowGridCard } from "../console/WindowGridCard";
import { PageShell } from "./PageShell";

/**
 * Página Console: o histórico do que o app fez — launch, Auto Rejoin, Watcher
 * e erros. Era a aba Console da Choose Game, que só aparecia com contas
 * selecionadas; agora fica na barra lateral, logo abaixo de Session. As listas
 * da sessão (Entrando, Em jogo) não se repetem aqui: moram só na página
 * Session. No topo, compacta, a grade de janelas (era a aba Windows da Choose
 * Game).
 */
export function ConsolePage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const store = useStore();
  if (!active) return null;
  // A grade de janelas é só do Windows (platform/windows/windowing.rs).
  const isWindows = isWindowsPlatform(store.platformCapabilities);
  return (
    <PageShell
      title={t("Console")}
      description={t("What the app did: launch, Auto Rejoin, Watcher and errors, with where each line came from.")}
      onLeave={onLeave}
      dataTour="console-page"
      tour="console"
      bodyClassName="flex flex-col gap-4 px-6 py-5 overflow-y-auto"
    >
      {/* Grade de janelas no topo, compacta (era a aba Windows da Choose Game). */}
      {isWindows && <WindowGridCard />}
      <LaunchConsole className="flex-1" />
    </PageShell>
  );
}
