import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useTr } from "../../i18n/text";

/**
 * Fileira lateral de cards, como o "Continue →" e o "Friends" do site do
 * Roblox: um título em cima e os itens lado a lado, rolando para o lado.
 *
 * Pedido do dono (14/10/2026): as listas em linha cheia da Escolher jogo
 * deixavam a tela larga quase vazia (nome à esquerda, botões na outra ponta).
 * Em fileira, cada seção ocupa uma faixa só e o ícone/nome ficam maiores.
 *
 * As setas só aparecem quando há o que rolar para aquele lado. A roda do mouse
 * continua rolando a página na vertical (a fileira não a sequestra); com a
 * roda na horizontal (ou Shift+roda) o navegador já rola a fileira.
 */
export interface CardRailProps {
  title: ReactNode;
  /** Ao lado do título (contagem, indicador de carregando). */
  meta?: ReactNode;
  /** No canto direito do cabeçalho, antes das setas. */
  actions?: ReactNode;
  /** Ícone/foto antes do título (a conta, na aba Amigos). */
  leading?: ReactNode;
  children: ReactNode;
  testId?: string;
  className?: string;
  ariaBusy?: boolean;
  /** Elemento da seção (`section` por padrão; os testes contam as seções). */
  as?: "section" | "div";
}

export function CardRail({
  title,
  meta,
  actions,
  leading,
  children,
  testId,
  className = "",
  ariaBusy,
  as: Tag = "section",
}: CardRailProps) {
  const t = useTr();
  const scroller = useRef<HTMLDivElement | null>(null);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);

  const measure = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 2);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 2);
  }, []);

  useEffect(() => {
    measure();
    const el = scroller.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    // Os cards chegam aos poucos (conta por conta): a largura total muda.
    for (const child of Array.from(el.children)) observer.observe(child);
    return () => observer.disconnect();
  }, [measure, children]);

  const page = (direction: 1 | -1) => {
    const el = scroller.current;
    if (!el) return;
    el.scrollBy({ left: direction * Math.max(160, el.clientWidth * 0.8), behavior: "smooth" });
  };

  const arrow =
    "w-7 h-7 rounded-full border theme-border theme-btn-ghost flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] disabled:opacity-30 disabled:cursor-default transition-colors";

  return (
    <Tag data-testid={testId} aria-busy={ariaBusy || undefined} className={`min-w-0 ${className}`}>
      <header className="flex items-center gap-2 px-1 pb-2">
        {leading}
        <h4 className="text-[14px] font-semibold text-[var(--panel-fg)] truncate">{title}</h4>
        {meta ? <span className="shrink-0 flex items-center gap-1.5 text-[12px] theme-muted">{meta}</span> : null}
        <div className="ml-auto shrink-0 flex items-center gap-1.5">
          {actions}
          {canLeft || canRight ? (
            <>
              <button
                type="button"
                aria-label={t("Scroll left")}
                disabled={!canLeft}
                onClick={() => page(-1)}
                className={arrow}
              >
                <ChevronLeft size={14} strokeWidth={2} />
              </button>
              <button
                type="button"
                aria-label={t("Scroll right")}
                disabled={!canRight}
                onClick={() => page(1)}
                className={arrow}
              >
                <ChevronRight size={14} strokeWidth={2} />
              </button>
            </>
          ) : null}
        </div>
      </header>
      <div
        ref={scroller}
        onScroll={measure}
        className="flex gap-3 overflow-x-auto overflow-y-hidden pb-1 px-1 scroll-smooth [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {children}
      </div>
    </Tag>
  );
}
