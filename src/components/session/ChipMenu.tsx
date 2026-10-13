import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Check } from "lucide-react";
import { useEscapeStack } from "../../hooks/useEscapeStack";

/**
 * Chip compacto que abre um menu pequeno de opções (uma escolhida). É o
 * controle das linhas da lista "Em jogo" (teto de memória, reconexão) e da
 * barra de lote: mostra o valor numa pílula de 24 px de altura e só abre a
 * lista quando a pessoa quer mudar — o seletor nativo de 104 px que havia
 * antes cobria a leitura de memória e espremia o resto da linha.
 *
 * Teclado: Enter/Espaço/seta abrem; setas andam; Enter escolhe; Escape fecha
 * (pela pilha de Escape) e devolve o foco ao chip.
 */

export interface ChipMenuOption {
  value: string;
  label: string;
  checked?: boolean;
  /** Separa do grupo de cima (ex.: "Custom…" no fim). */
  separated?: boolean;
}

export function ChipMenu({
  ariaLabel,
  title,
  menuLabel,
  options,
  onSelect,
  disabled = false,
  tone = "default",
  children,
  className = "",
  testId,
}: {
  ariaLabel: string;
  title?: string;
  /** Nome do menu aberto (para leitor de tela). */
  menuLabel: string;
  options: ChipMenuOption[];
  onSelect: (value: string) => void;
  disabled?: boolean;
  tone?: "default" | "warn";
  children: ReactNode;
  className?: string;
  testId?: string;
}) {
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  function close(returnFocus = true) {
    setOpen(false);
    if (returnFocus) buttonRef.current?.focus();
  }

  useEscapeStack(open, () => close());

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      const target = e.target as Node;
      if (menuRef.current?.contains(target) || buttonRef.current?.contains(target)) return;
      close(false);
    }
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  useLayoutEffect(() => {
    if (!open) return;
    function place() {
      const anchor = buttonRef.current?.getBoundingClientRect();
      const menu = menuRef.current?.getBoundingClientRect();
      if (!anchor) return;
      const width = menu?.width || 180;
      const height = menu?.height || 0;
      const margin = 8;
      const left = Math.min(window.innerWidth - margin - width, Math.max(margin, anchor.right - width));
      const below = anchor.bottom + 4;
      const top = below + height > window.innerHeight - margin ? Math.max(margin, anchor.top - 4 - height) : below;
      setPos({ top, left });
    }
    place();
    const raf = requestAnimationFrame(place);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

  // Abre com o foco na opção escolhida (ou na primeira).
  useEffect(() => {
    if (!open) return;
    const items = menuRef.current?.querySelectorAll<HTMLButtonElement>("[role='menuitemradio']");
    if (!items || items.length === 0) return;
    const current = [...items].find((el) => el.getAttribute("aria-checked") === "true") ?? items[0];
    current.focus();
  }, [open]);

  function onMenuKey(e: React.KeyboardEvent) {
    const items = [...(menuRef.current?.querySelectorAll<HTMLButtonElement>("[role='menuitemradio']") ?? [])];
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = e.key === "ArrowDown" ? (index + 1) % items.length : (index - 1 + items.length) % items.length;
      items[next]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      items[e.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (e.key === "Tab") {
      close(false);
    }
  }

  const toneClass =
    tone === "warn"
      ? "border-amber-500/50 text-amber-400"
      : "theme-border text-[var(--panel-fg)]";

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        aria-label={ariaLabel}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-disabled={disabled || undefined}
        title={title}
        data-testid={testId}
        onClick={() => {
          if (!disabled) setOpen((v) => !v);
        }}
        onKeyDown={(e) => {
          if (!disabled && e.key === "ArrowDown") {
            e.preventDefault();
            setOpen(true);
          }
        }}
        className={`inline-flex items-center gap-1 h-6 shrink-0 rounded-md border bg-[var(--panel-bg)] px-1.5 text-[11.5px] tabular-nums transition-colors ${toneClass} ${
          disabled ? "opacity-70 cursor-not-allowed" : "hover:bg-[var(--row-hover)]"
        } ${className}`}
      >
        {children}
      </button>
      {open &&
        createPortal(
          <div
            ref={menuRef}
            role="menu"
            aria-label={menuLabel}
            onKeyDown={onMenuKey}
            className="theme-modal-scope theme-panel fixed z-[999] min-w-[170px] max-w-[260px] rounded-lg border theme-border p-1 shadow-xl animate-scale-in"
            style={pos ? { top: pos.top, left: pos.left } : { top: 0, left: 0, visibility: "hidden" }}
          >
            {options.map((option) => (
              <Fragment key={option.value}>
                {option.separated && <div role="separator" className="my-1 border-t theme-border" />}
                <button
                  type="button"
                  role="menuitemradio"
                  aria-checked={option.checked === true}
                  onClick={() => {
                    close();
                    onSelect(option.value);
                  }}
                  className="flex w-full min-h-[28px] items-center gap-2 rounded-md px-2 text-left text-[12px] text-[var(--panel-fg)] hover:bg-[var(--row-hover)] focus:bg-[var(--row-hover)] outline-none"
                >
                  <span aria-hidden="true" className="w-3.5 shrink-0 text-[var(--accent-color)]">
                    {option.checked ? <Check size={13} strokeWidth={2} /> : null}
                  </span>
                  <span className="min-w-0 truncate">{option.label}</span>
                </button>
              </Fragment>
            ))}
          </div>,
          document.body
        )}
    </>
  );
}
