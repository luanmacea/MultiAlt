import React, { type ReactNode } from "react";
import { tr } from "../../i18n/text";

/**
 * Última rede antes da tela branca.
 *
 * Um throw no render do React desmonta a árvore inteira e deixa a janela vazia
 * — o **mesmo** sintoma de um WebView2 quebrado (ver
 * `src-tauri/src/webview_recovery.rs`). Da cadeira do usuário os dois casos são
 * indistinguíveis, e a recuperação é diferente: bug do app se resolve
 * recarregando a interface, falha do WebView2 exige safe mode de vídeo. Este
 * boundary existe para o app dizer qual dos dois aconteceu.
 *
 * Desenhar algo aqui também vale como "o frontend pintou" para o watchdog do
 * backend, o que evita reabrir o app em safe mode por causa de um bug nosso.
 *
 * As cores vão em `style` além das classes de tema porque o crash pode ter
 * acontecido **antes** de o tema ser aplicado às variáveis CSS — e aí `var(...)`
 * vazio deixaria texto invisível.
 */
interface AppErrorBoundaryProps {
  children: ReactNode;
  /** O teste injeta o seu próprio; no app é recarregar a janela. */
  onReload?: () => void;
}

interface AppErrorBoundaryState {
  error: Error | null;
}

export class AppErrorBoundary extends React.Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  constructor(props: AppErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  /** `throw "texto"` é válido em JS: o que não é `Error` vira um. */
  static getDerivedStateFromError(error: unknown): AppErrorBoundaryState {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  componentDidCatch(error: unknown, info: React.ErrorInfo): void {
    // O console é o que sobra para diagnosticar depois, já que a tela de erro
    // mostra só a mensagem.
    console.error("AppErrorBoundary:", error, info.componentStack);
  }

  private reload = (): void => {
    if (this.props.onReload) {
      this.props.onReload();
      return;
    }
    window.location.reload();
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div
        className="fixed inset-0 z-[100] flex flex-col items-center justify-center gap-4 p-6 text-sm"
        style={{ background: "#0b0b0d", color: "#e4e4e7" }}
      >
        <h1 className="text-base font-semibold">{tr("Something broke in the interface")}</h1>
        <p className="max-w-xl text-center opacity-80">
          {tr(
            "Your accounts and settings were not touched. Reloading rebuilds the interface without closing any Roblox client."
          )}
        </p>
        <pre
          className="w-full max-w-xl max-h-[40vh] overflow-auto rounded-lg p-3 text-xs font-mono whitespace-pre-wrap"
          style={{ background: "#18181b", border: "1px solid #27272a" }}
        >
          {error.stack || error.message}
        </pre>
        <button
          type="button"
          onClick={this.reload}
          className="rounded-lg px-4 py-2 text-xs font-medium"
          style={{ background: "#27272a", border: "1px solid #3f3f46", color: "#e4e4e7" }}
        >
          {tr("Reload interface")}
        </button>
      </div>
    );
  }
}
