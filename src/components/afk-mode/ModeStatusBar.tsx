import type { ReactNode } from "react";

/**
 * A barra de estado de cada aba do Modo AFK: diz **se o modo está ligado** e
 * traz os botões de ligar/parar, sempre à vista (fica fora da área que rola).
 *
 * Existe por uma queixa concreta: o Auto Rejoin ligava num clique pelo "Em
 * jogo" e a pessoa não achava onde ver que estava rodando, nem onde parar.
 */
export function ModeStatusBar({
  running,
  title,
  facts,
  actions,
  message,
  testId,
  dataTour,
}: {
  running: boolean;
  title: string;
  /** Fatos curtos ao lado do título (quantas contas, próximo ciclo...). */
  facts?: ReactNode[];
  actions: ReactNode;
  /** Linha de baixo: por que não liga, ou o erro do último Start. */
  message?: { text: string; tone: "muted" | "warn" | "error" } | null;
  testId?: string;
  /** Âncora do tutorial da página (ver components/tour/tours.ts). */
  dataTour?: string;
}) {
  const shown = (facts ?? []).filter((f) => f !== null && f !== undefined && f !== false && f !== "");
  return (
    <section
      data-testid={testId}
      data-tour={dataTour}
      data-running={running ? "true" : "false"}
      aria-live="polite"
      className={`relative shrink-0 rounded-xl border px-4 py-3 transition-colors ${
        running
          ? "border-emerald-500/35 bg-[linear-gradient(100deg,rgba(16,185,129,0.16),rgba(16,185,129,0.04)_55%,transparent)]"
          : "theme-border bg-[var(--panel-soft)]"
      }`}
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2.5">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <span className="relative flex h-3 w-3 shrink-0 items-center justify-center" aria-hidden>
            {running ? (
              <>
                <span className="absolute inline-flex h-full w-full rounded-full bg-emerald-400/60 motion-safe:animate-ping" />
                <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-emerald-400" />
              </>
            ) : (
              <span className="inline-flex h-2.5 w-2.5 rounded-full border-2 border-[var(--panel-muted)] opacity-70" />
            )}
          </span>
          <div className="min-w-0">
            <div
              className={`text-[14px] font-semibold leading-tight ${
                running ? "text-emerald-200" : "text-[var(--panel-fg)]"
              }`}
            >
              {title}
            </div>
            {shown.length > 0 ? (
              <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] theme-muted">
                {shown.map((fact, index) => (
                  <span key={index} className="inline-flex min-w-0 items-center gap-1.5">
                    {fact}
                  </span>
                ))}
              </div>
            ) : null}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">{actions}</div>
      </div>
      {message ? (
        <div
          role={message.tone === "error" ? "alert" : undefined}
          className={`mt-2 text-[12px] leading-4 break-words ${
            message.tone === "error"
              ? "text-red-300"
              : message.tone === "warn"
                ? "text-amber-300"
                : "theme-muted"
          }`}
        >
          {message.text}
        </div>
      ) : null}
    </section>
  );
}

/** Botão principal da barra (ligar): leva a cor de destaque do tema. */
export const PRIMARY_ACTION =
  "sidebar-btn-sm theme-accent-bg theme-accent theme-accent-border font-medium px-4 disabled:opacity-50 disabled:cursor-not-allowed";
/** Parar sem fechar nada. */
export const NEUTRAL_ACTION =
  "sidebar-btn-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed";
/** Ação que fecha cliente: vermelho, sempre com confirmação antes. */
export const DANGER_ACTION =
  "sidebar-btn-sm text-red-200 border-red-400/40 hover:bg-red-500/15 disabled:opacity-50 disabled:cursor-not-allowed";
