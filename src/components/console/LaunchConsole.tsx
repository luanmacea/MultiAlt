import { useEffect, useRef, useState, type UIEvent } from "react";
import { Terminal, Trash2 } from "lucide-react";
import { useStore, type LaunchLogLevel } from "../../store";
import { useTr } from "../../i18n/text";
import { accountLabel } from "../../utils/accountName";
import { TONE_STYLES, type ToneStyle } from "../../utils/toastTone";

/**
 * Os níveis do log de launch são os mesmos tons do feedback de ação, então a
 * paleta é a compartilhada (`TONE_STYLES`) — o toast e o rodapé combinam com
 * este Console em vez de cada tela ter a sua cor.
 */
const LEVEL_STYLES: Record<LaunchLogLevel, ToneStyle> = TONE_STYLES;

/**
 * O histórico do que o app fez (evento `launch-log`): launch, Auto Rejoin,
 * Watcher e erros, cada linha com a origem (`step`) numa coluna de largura
 * fixa. Só o log — as listas da sessão (Entrando, Em jogo) moram na página
 * Session. Rola sozinho até a linha nova, a não ser que a pessoa tenha subido.
 */
export function LaunchConsole({ className = "" }: { className?: string }) {
  const t = useTr();
  const store = useStore();
  const logs = store.launchLogs;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  const nameFor = (userId: number | null): string => {
    if (userId === null) return "—";
    const a = store.accounts.find((acc) => acc.UserID === userId);
    return accountLabel(a, store, userId);
  };

  const fmtTime = (ts: number) =>
    new Date(ts).toLocaleTimeString([], { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, autoScroll]);

  function onScroll(e: UIEvent<HTMLDivElement>) {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  }

  return (
    <div className={`flex flex-col min-h-0 ${className}`} data-testid="launch-console" data-tour="console-log">
      <div className="shrink-0 flex items-center justify-between px-1 pb-2">
        <span className="text-[12px] theme-muted">
          {logs.length === 0 ? t("No activity yet") : t("{{count}} log lines", { count: logs.length })}
        </span>
        <button
          onClick={() => store.clearLaunchLogs()}
          disabled={logs.length === 0}
          className="flex items-center gap-1.5 min-h-[28px] text-[12px] theme-muted hover:text-[var(--panel-fg)] px-2.5 py-1 rounded-md theme-btn-ghost border theme-border transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Trash2 size={12} strokeWidth={1.5} aria-hidden="true" />
          {t("Clear")}
        </button>
      </div>

      <div
        ref={scrollRef}
        onScroll={onScroll}
        role="log"
        aria-label={t("Console")}
        // Piso de 160px: com a janela baixa o log não some atrás do cartão de cima.
        className="flex-1 min-h-[160px] overflow-y-auto rounded-lg border theme-border bg-[var(--panel-soft)] font-mono text-[12px] leading-relaxed p-3"
      >
        {logs.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10">
            <Terminal size={22} strokeWidth={1.5} aria-hidden="true" />
            <p className="text-[12px]">{t("Launch a game or start Auto Rejoin to see the activity here")}</p>
          </div>
        ) : (
          logs.map((log) => {
            const style = LEVEL_STYLES[log.level] ?? LEVEL_STYLES.info;
            return (
              <div key={log.id} className="flex items-start gap-2 py-0.5">
                <span className="theme-muted shrink-0 tabular-nums">{fmtTime(log.ts)}</span>
                <span aria-hidden="true" className={`shrink-0 w-1.5 h-1.5 rounded-full mt-[6px] ${style.dot}`} />
                {/* De onde veio a linha: launch, Auto Rejoin e Watcher juntos. */}
                {log.step ? (
                  <span
                    data-testid="log-step"
                    // Largura fixa: sem ela, `[watcher]` e `[rejoin-retry]`
                    // empurram o nome da conta para colunas diferentes e a
                    // leitura vertical do log se perde. 112px foi medido no
                    // nome mais longo na fonte mono de 12px.
                    className="shrink-0 theme-muted w-[112px] truncate"
                    title={log.step}
                  >
                    [{log.step}]
                  </span>
                ) : null}
                <span className="shrink-0 text-[var(--accent-color)] max-w-[120px] truncate">{nameFor(log.userId)}</span>
                <span className={`${style.text} break-words`}>{log.message}</span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
