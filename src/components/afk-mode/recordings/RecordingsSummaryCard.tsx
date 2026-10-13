import { Clapperboard } from "lucide-react";
import { useStore } from "../../../store";
import { useTr } from "../../../i18n/text";
import { useRecordings } from "./useRecordings";
import { recordingTriggers } from "./triggers";
import { RecordingTriggerLines } from "./RecordingTriggerLines";

/**
 * Cartão "Recordings" do resumo da página Session: uma linha por vaga ("After
 * auto-reconnect" e "On anti-AFK") com a gravação global dela e quando toca,
 * quantas contas têm escolha própria e o atalho para a aba Recordings do Modo
 * AFK. Só leitura —
 * tudo se muda na aba. Sem nenhuma gravação o cartão não aparece: não haveria
 * o que dizer além de "nenhuma".
 */
export function RecordingsSummaryCard() {
  const t = useTr();
  const store = useStore();
  const { payload } = useRecordings();
  if (!payload || payload.recordings.length === 0) return null;
  const triggers = recordingTriggers(payload, store.settings);
  return (
    <section className="rounded-xl border theme-border px-4 py-3" aria-label={t("Recordings")} data-testid="session-recordings">
      <h2 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("Recordings")}</h2>
      <RecordingTriggerLines triggers={triggers} className="mt-1.5" />
      {triggers.ownCount > 0 ? (
        <p className="mt-1 text-[11.5px] leading-snug text-[var(--panel-muted)]">
          {t("{{count}} account(s) with their own choice.", { count: triggers.ownCount })}
        </p>
      ) : null}
      <button
        type="button"
        onClick={() => store.openAfkMode({ tab: "recordings" })}
        className="theme-btn mt-3 inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium"
      >
        <Clapperboard size={13} strokeWidth={1.8} aria-hidden="true" />
        {t("Open Recordings")}
      </button>
    </section>
  );
}
