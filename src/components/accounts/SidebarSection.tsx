import { useId, useState } from "react";
import { ChevronDown } from "lucide-react";
import { useTr } from "../../i18n/text";

const COLLAPSED_KEY_PREFIX = "multialt.sidebarSection.collapsed.";

/** Recolhida da última vez? Só conveniência de quem olha: sem storage, abre. */
function readCollapsed(id: string): boolean {
  try {
    return window.localStorage.getItem(COLLAPSED_KEY_PREFIX + id) === "1";
  } catch {
    return false;
  }
}

function writeCollapsed(id: string, collapsed: boolean): void {
  try {
    if (collapsed) window.localStorage.setItem(COLLAPSED_KEY_PREFIX + id, "1");
    else window.localStorage.removeItem(COLLAPSED_KEY_PREFIX + id);
  } catch {
    // Sem storage (janela privada, cota): a escolha vale até trocar de tela.
  }
}

/**
 * Seção do painel da conta. Com `collapseId`, o título vira um botão que
 * recolhe a seção, e a escolha fica lembrada neste navegador (`localStorage`).
 */
export function SidebarSection({
  title,
  children,
  collapseId,
}: {
  title: string;
  children: React.ReactNode;
  collapseId?: string;
}) {
  const t = useTr();
  const contentId = useId();
  const [collapsed, setCollapsed] = useState(() => (collapseId ? readCollapsed(collapseId) : false));
  const titleClass = "theme-label text-[11px] font-medium uppercase tracking-wider";

  if (!collapseId) {
    return (
      <div className="flex flex-col gap-1.5" data-sidebar-section={title}>
        <div className={titleClass}>{t(title)}</div>
        {children}
      </div>
    );
  }

  function toggle() {
    const next = !collapsed;
    setCollapsed(next);
    writeCollapsed(collapseId!, next);
  }

  return (
    <div className="flex flex-col gap-1.5" data-sidebar-section={title}>
      <button
        type="button"
        onClick={toggle}
        aria-expanded={!collapsed}
        aria-controls={collapsed ? undefined : contentId}
        className={`${titleClass} flex items-center justify-between gap-2 text-left hover:text-[var(--panel-fg)]`}
      >
        <span>{t(title)}</span>
        <ChevronDown
          size={12}
          strokeWidth={2}
          aria-hidden="true"
          className={`shrink-0 transition-transform ${collapsed ? "-rotate-90" : ""}`}
        />
      </button>
      {!collapsed && (
        <div id={contentId} className="flex flex-col gap-1.5">
          {children}
        </div>
      )}
    </div>
  );
}
