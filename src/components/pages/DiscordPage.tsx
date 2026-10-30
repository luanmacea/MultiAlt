import { useEffect } from "react";
import { useSettings } from "../../hooks/useSettings";
import { useTr } from "../../i18n/text";
import { DiscordSettings } from "../discord/DiscordSettings";
import { PageShell } from "./PageShell";

/**
 * Página Discord alerts: avisos, status ao vivo e resumo diário no webhook do
 * próprio usuário (`DiscordSettings`). Recarrega as settings ao entrar, como a
 * Settings.
 */
export function DiscordPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const s = useSettings();

  useEffect(() => {
    if (active) void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  if (!active) return null;
  return (
    <PageShell
      title={t("Discord alerts")}
      description={t(
        "Know what your accounts are doing without looking at the PC: alerts, a live status message and a daily summary in your own Discord channel."
      )}
      onLeave={onLeave}
    >
      <div className="max-w-[760px] 2xl:max-w-[920px]">{s.loaded ? <DiscordSettings s={s} /> : null}</div>
    </PageShell>
  );
}
