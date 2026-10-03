import { useId, type ReactNode } from "react";
import { useEscapeStack } from "../../hooks/useEscapeStack";

/**
 * Casca comum das páginas da área principal (Session, AFK Mode, Avatars,
 * Scripts, Theme, Nexus, Settings). Antes cada uma era um modal centralizado;
 * agora ocupam a área inteira à direita da barra lateral, com cabeçalho
 * próprio: título, uma frase do que a página faz e as ações dela.
 *
 * - `theme-modal-scope` continua aqui: é ele que traduz as classes `zinc-*`
 *   herdadas dos modais para as variáveis do tema (ver index.css).
 * - Escape volta para a lista de contas, pela pilha de Escape: diálogo ou
 *   popover aberto por cima consome o Escape antes (mesmo padrão da Choose
 *   Game). Escape digitado num campo não sai da página.
 */
export interface PageShellProps {
  title: string;
  description?: ReactNode;
  /** Botões e indicadores à direita do título. */
  actions?: ReactNode;
  /** Faixa logo abaixo do cabeçalho (abas, por exemplo). */
  toolbar?: ReactNode;
  /** O que fazer no Escape — normalmente voltar para a lista de contas. */
  onLeave: () => void;
  /**
   * Classes do corpo. O padrão rola por dentro com respiro; páginas que dividem
   * a área em colunas próprias (Scripts, Theme) passam o seu layout.
   */
  bodyClassName?: string;
  dataTour?: string;
  children: ReactNode;
}

export function PageShell({
  title,
  description,
  actions,
  toolbar,
  onLeave,
  bodyClassName = "overflow-y-auto px-6 py-5",
  dataTour,
  children,
}: PageShellProps) {
  const headingId = useId();
  useEscapeStack(true, onLeave, { ignoreFromFields: true });

  return (
    <section
      aria-labelledby={headingId}
      data-tour={dataTour}
      className="page-enter theme-modal-scope theme-panel flex flex-1 min-w-0 min-h-0 flex-col"
    >
      <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3 px-6 pt-5 pb-4 border-b theme-border shrink-0">
        <div className="min-w-0 flex-1 basis-[280px]">
          <h1 id={headingId} className="text-[18px] leading-tight font-semibold tracking-tight text-[var(--panel-fg)]">
            {title}
          </h1>
          {description ? (
            <div className="mt-1.5 max-w-[68ch] text-[12.5px] leading-relaxed text-[var(--panel-muted)]">
              {description}
            </div>
          ) : null}
        </div>
        {actions ? <div className="flex flex-wrap items-center gap-2 shrink-0">{actions}</div> : null}
      </header>
      {toolbar ? <div className="shrink-0 border-b theme-border px-6">{toolbar}</div> : null}
      <div className={`flex-1 min-h-0 ${bodyClassName}`}>{children}</div>
    </section>
  );
}
