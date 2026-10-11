import { useTr } from "../../../i18n/text";
import { formatAfkIntervalSeconds, type RecordingTriggers } from "./triggers";

/**
 * Uma linha por vaga: "After auto-reconnect" (quanto tempo depois de voltar ao
 * jogo, e a gravação global dela) e "On anti-AFK" (de quanto em quanto tempo no
 * Modo AFK, e a global dela). A mesma frase na aba Recordings e no resumo da
 * página Session.
 */
export function RecordingTriggerLines({ triggers, className = "" }: { triggers: RecordingTriggers; className?: string }) {
  const t = useTr();
  const reconnectName = triggers.globalNames.afterReconnect;
  const afkName = triggers.globalNames.antiAfk;
  const interval = formatAfkIntervalSeconds(triggers.intervalSeconds);
  return (
    <ul className={`space-y-0.5 text-[11.5px] leading-snug text-[var(--panel-muted)] ${className}`} data-testid="recording-triggers">
      <li>
        {!triggers.afterReconnect
          ? t("After auto-reconnect: off.")
          : reconnectName
            ? t("After auto-reconnect: {{name}}, once, {{seconds}} s after the account is back in the game.", {
                name: reconnectName,
                seconds: triggers.delaySeconds,
              })
            : t("After auto-reconnect: once, {{seconds}} s after the account is back in the game; no global recording.", {
                seconds: triggers.delaySeconds,
              })}
      </li>
      <li>
        {!triggers.afkRepeats
          ? t("On anti-AFK: off (AFK mode sends a key or a click instead).")
          : afkName
            ? t("On anti-AFK: {{name}}, every {{interval}} in AFK mode.", { name: afkName, interval })
            : t("On anti-AFK: every {{interval}} in AFK mode; no global recording.", { interval })}
      </li>
    </ul>
  );
}
