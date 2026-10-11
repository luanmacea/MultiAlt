import type { ReactNode } from "react";
import { ArrowRight } from "lucide-react";

/**
 * Peças comuns às seções que saíram de Settings › General (11/10/2026).
 *
 * Nada aqui grava setting: são o cartão com um botão de ação (checagem,
 * walkthrough, criptografia) e a linha que manda para a página onde um ajuste
 * mora de verdade.
 */

/** Plataforma pelo user agent, como a antiga aba General fazia (os testes trocam o UA). */
export function isWindowsUserAgent(): boolean {
  return typeof navigator !== "undefined" && navigator.userAgent.toLowerCase().includes("windows");
}

const BUTTON =
  "shrink-0 rounded-lg border border-zinc-700/70 bg-zinc-800 px-3 py-1.5 text-[12px] font-medium text-zinc-200 transition-colors hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-50";

/** Cartão com título, uma frase e um botão. Os textos chegam já traduzidos (`t` no chamador, para o extrator achar a chave). */
export function ActionCard({
  title,
  description,
  action,
  onAction,
  disabled = false,
  children,
}: {
  title: string;
  description: ReactNode;
  action: string;
  onAction: () => void;
  disabled?: boolean;
  /** Conteúdo extra debaixo da linha (barra de progresso, erro). */
  children?: ReactNode;
}) {
  return (
    <div className="px-1 py-2">
      <div className="rounded-lg border border-zinc-800/70 bg-zinc-900/35 px-3 py-2">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[13px] text-zinc-200">{title}</div>
            <div className="mt-0.5 text-[12px] text-zinc-500">{description}</div>
          </div>
          <button type="button" onClick={onAction} disabled={disabled} className={BUTTON}>
            {action}
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

/**
 * Um ajuste que mora em outra página: o nome, uma frase com o estado e o botão
 * que leva até lá. Fica no lugar do controle duplicado: um controle editável por
 * chave, na tela em que o usuário está quando precisa dele. Textos já traduzidos.
 */
export function ElsewhereLink({
  title,
  description,
  action,
  onNavigate,
}: {
  title: string;
  description: string;
  action: string;
  onNavigate: () => void;
}) {
  return (
    <div className="flex items-center gap-3 py-2 px-1">
      <div className="min-w-0">
        <div className="text-[13px] text-zinc-300">{title}</div>
        <div className="mt-0.5 text-[12px] text-zinc-500">{description}</div>
      </div>
      <button
        type="button"
        onClick={onNavigate}
        className="ml-auto shrink-0 inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/70 px-3 py-1.5 text-[12px] font-medium text-zinc-200 transition-colors hover:bg-zinc-800"
      >
        {action}
        <ArrowRight size={12} strokeWidth={1.8} aria-hidden="true" />
      </button>
    </div>
  );
}
