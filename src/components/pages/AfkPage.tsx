import { useTr } from "../../i18n/text";
import { AfkModeView } from "../afk-mode/AfkModeView";
import { PageShell } from "./PageShell";

/**
 * Página AFK Mode: Auto Rejoin e cliques/teclas AFK juntos, no `AfkModeView`
 * (variante `page`). O cabeçalho é da página; o corpo é todo do view.
 */
export function AfkPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  if (!active) return null;
  return (
    <PageShell
      title={t("AFK Mode")}
      description={t(
        "Keep accounts in game: Auto Rejoin relaunches your alts in a place on a timer, and AFK presses a key or clicks in each window so the account keeps its spot without a rejoin."
      )}
      onLeave={onLeave}
      dataTour="afk-page"
      tour="afk"
      // O view rola as abas por dentro e prende Iniciar/Parar fora da rolagem.
      bodyClassName="flex flex-col"
    >
      <AfkModeView variant="page" />
    </PageShell>
  );
}
