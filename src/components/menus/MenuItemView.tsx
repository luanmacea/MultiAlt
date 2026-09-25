import { useRef, useState } from "react";
import { useTr } from "../../i18n/text";

export interface MenuItem {
  label: string;
  action?: () => void;
  separator?: boolean;
  submenu?: MenuItem[];
  devOnly?: boolean;
  className?: string;
}

/**
 * A partir de `el` (um item de menu ou o wrapper de um submenu), acha o
 * próximo/anterior irmão que é um item navegável e devolve o elemento que
 * deve receber o foco. Pula separadores (não têm `role="menuitem"`) e olha
 * dentro do wrapper `.submenu-trigger`, cujo item de fato fica um nível
 * abaixo.
 */
function menuItemFocusTarget(el: Element): HTMLElement | null {
  if (el.matches('[role="menuitem"]')) return el as HTMLElement;
  const inner = el.querySelector(':scope > [role="menuitem"]');
  return inner as HTMLElement | null;
}

function focusSibling(from: Element, direction: 1 | -1) {
  let el: Element | null = from;
  while (el) {
    el = direction === 1 ? el.nextElementSibling : el.previousElementSibling;
    if (!el) return;
    const target = menuItemFocusTarget(el);
    if (target) {
      target.focus();
      return;
    }
  }
}

export function MenuItemView({ item, close }: { item: MenuItem; close: () => void }) {
  const t = useTr();
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLDivElement>(null);
  const submenuRef = useRef<HTMLDivElement>(null);

  if (item.separator) {
    return <div className="my-1 border-t border-zinc-800/80" role="separator" />;
  }

  if (item.submenu) {
    return (
      <div className="submenu-trigger relative">
        <div
          ref={triggerRef}
          role="menuitem"
          tabIndex={0}
          aria-haspopup="true"
          aria-expanded={open}
          className="flex items-center justify-between px-3 py-1.5 text-[13px] text-zinc-300 hover:bg-zinc-800 cursor-default rounded-md mx-1 outline-none"
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " " || e.key === "ArrowRight") {
              e.preventDefault();
              setOpen(true);
              const first = submenuRef.current?.querySelector('[role="menuitem"]') as HTMLElement | null;
              first?.focus();
            } else if (e.key === "ArrowDown") {
              e.preventDefault();
              focusSibling(e.currentTarget.parentElement as HTMLElement, 1);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              focusSibling(e.currentTarget.parentElement as HTMLElement, -1);
            }
            // Esc não é tratado aqui — quem fecha o menu com Esc é o listener
            // global do ContextMenu.
          }}
        >
          <span>{t(item.label)}</span>
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-zinc-600">
            <path d="m9 18 6-6-6-6" />
          </svg>
        </div>
        <div
          ref={submenuRef}
          role="menu"
          className={`theme-modal-scope theme-panel theme-border submenu-panel ${open ? "submenu-open" : "hidden"} absolute left-full top-0 -mt-1 ml-0.5 min-w-[200px] bg-zinc-900/95 backdrop-blur-xl border border-zinc-700/50 rounded-xl shadow-2xl py-1 z-50 animate-fade-in`}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft") {
              e.stopPropagation();
              setOpen(false);
              triggerRef.current?.focus();
            }
          }}
        >
          <div className="pl-1">
            {item.submenu.map((sub, i) => (
              <MenuItemView key={i} item={sub} close={close} />
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      role="menuitem"
      tabIndex={0}
      className={`flex items-center px-3 py-1.5 text-[13px] hover:bg-zinc-800 cursor-default rounded-md mx-1 outline-none ${item.className || "text-zinc-300"}`}
      onClick={() => {
        item.action?.();
        close();
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          item.action?.();
          close();
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          focusSibling(e.currentTarget, 1);
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          focusSibling(e.currentTarget, -1);
        }
      }}
    >
      {t(item.label)}
    </div>
  );
}
