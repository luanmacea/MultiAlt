import { Check, ChevronDown, ChevronRight, ChevronUp, GripVertical } from "lucide-react";
import { useStore } from "../../store";
import type { ParsedGroup } from "../../types";
import { AccountRow } from "./AccountRow";
import { useTr } from "../../i18n/text";

export function GroupSection({
  group,
  collapsed,
  onToggle,
  onDrop,
}: {
  group: ParsedGroup;
  collapsed: boolean;
  onToggle: () => void;
  onDrop: (groupKey: string) => void;
}) {
  const t = useTr();
  const store = useStore();
  const showHeader = group.key !== "__all__" && store.showGroups && (store.theme?.show_headers ?? true);

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  }

  /**
   * O cabeçalho recebe dois arrastos diferentes: conta (mover para o grupo) e
   * grupo (reordenar). O `groupDragState` é separado do `dragState` justamente
   * para poder distinguir aqui — misturados, soltar um grupo moveria contas.
   */
  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    const arrastado = store.groupDragState?.groupKey;
    if (arrastado) {
      store.setGroupDragState(null);
      if (arrastado !== group.key) void store.reorderGroups(arrastado, group.key);
      return;
    }
    onDrop(group.key);
  }

  /**
   * Vizinhos deste grupo na ordem que está na tela. Arrastar depende de mouse
   * firme e de o navegador cooperar; as setas fazem o mesmo `reorderGroups`,
   * uma posição por clique, e são a única forma de reordenar por teclado.
   */
  const ordem = store.groups.filter((g) => g.key !== "__all__");
  const posicao = ordem.findIndex((g) => g.key === group.key);
  const acima = posicao > 0 ? ordem[posicao - 1] : null;
  const abaixo = posicao >= 0 && posicao < ordem.length - 1 ? ordem[posicao + 1] : null;

  function moverPara(alvo: { key: string } | null, e: React.MouseEvent) {
    // Sem isto o clique sobe para o cabeçalho e colapsa o grupo.
    e.stopPropagation();
    if (!alvo) return;
    void store.reorderGroups(group.key, alvo.key);
  }

  const groupIds = group.accounts.map((a) => a.UserID);
  const allSelected = groupIds.length > 0 && groupIds.every((id) => store.selectedIds.has(id));
  const someSelected = groupIds.some((id) => store.selectedIds.has(id));
  const multiMode = store.selectedIds.size > 1;

  function toggleGroupSelection() {
    if (allSelected) {
      const next = new Set(store.selectedIds);
      groupIds.forEach((id) => next.delete(id));
      store.setSelectedIds(next);
    } else {
      const next = new Set(store.selectedIds);
      groupIds.forEach((id) => next.add(id));
      store.setSelectedIds(next);
    }
  }

  function handleGroupCheckbox(e: React.MouseEvent) {
    e.stopPropagation();
    toggleGroupSelection();
  }

  function handleGroupCheckboxKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      toggleGroupSelection();
    }
  }

  return (
    <div className="mb-0.5">
      {showHeader && (
        <div
          data-group-header="true"
          data-group-key={group.key}
          role="button"
          tabIndex={0}
          aria-expanded={!collapsed}
          className={`theme-group-header group/ghead flex items-center gap-2 px-3 py-1.5 cursor-pointer select-none text-xs transition-all duration-150 outline-none ${
            store.dragState || store.groupDragState
              ? "hover:bg-[var(--accent-soft)] hover:pl-4"
              : "hover:bg-[var(--row-hover)]"
          }`}
          onClick={onToggle}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onToggle();
            }
          }}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
        >
          {/* Punho do arrasto: começa aqui, e não no cabeçalho inteiro, porque o
              cabeçalho também colapsa o grupo e recebe drop de conta. */}
          <div
            draggable
            onDragStart={(e) => {
              e.stopPropagation();
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", group.key);
              store.setGroupDragState({ groupKey: group.key });
            }}
            // Arrasto cancelado não pode deixar estado velho: o próximo drop de
            // conta acharia que ainda há um grupo sendo movido.
            onDragEnd={() => store.setGroupDragState(null)}
            onClick={(e) => e.stopPropagation()}
            className="shrink-0 opacity-30 group-hover/ghead:opacity-100 transition-opacity cursor-grab active:cursor-grabbing -ml-1.5 px-0.5"
            title={t("Drag to reorder groups")}
          >
            <GripVertical size={12} strokeWidth={1.5} className="theme-muted" />
          </div>

          <div className={`shrink-0 overflow-hidden transition-all duration-150 ease-out ${
            multiMode ? "w-3.5 opacity-100" : "w-0 opacity-0"
          }`}>
            <div
              role="checkbox"
              aria-checked={allSelected ? true : someSelected ? "mixed" : false}
              tabIndex={multiMode ? 0 : -1}
              className={`w-3.5 h-3.5 rounded border flex items-center justify-center transition-all duration-100 cursor-pointer outline-none ${
                allSelected ? "" : someSelected ? "" : "theme-border group-hover:brightness-110"
              }`}
              style={
                allSelected
                  ? { backgroundColor: "var(--accent-color)", borderColor: "var(--accent-color)" }
                  : someSelected
                  ? { backgroundColor: "var(--accent-soft)", borderColor: "var(--accent-strong)" }
                  : undefined
              }
              onClick={handleGroupCheckbox}
              onKeyDown={handleGroupCheckboxKeyDown}
            >
              {allSelected && (
                <Check size={8} stroke="var(--forms-bg)" strokeWidth={3.5} />
              )}
              {someSelected && !allSelected && (
                <div className="w-1.5 h-1.5 rounded-sm bg-[var(--accent-color)]" />
              )}
            </div>
          </div>
          <ChevronRight size={12} fill="currentColor" stroke="none" className={`theme-muted transition-transform duration-200 ${collapsed ? "" : "rotate-90"}`} />
          <span className="theme-label font-medium">{group.displayName === "Default" ? t("Default") : group.displayName}</span>
          <span className="theme-muted text-[11px] tabular-nums">{group.accounts.length}</span>

          {/* Setas à direita: o caminho que não depende de arrastar. */}
          <span className="ml-auto flex items-center gap-0.5 shrink-0">
            <button
              type="button"
              onClick={(e) => moverPara(acima, e)}
              onKeyDown={(e) => e.stopPropagation()}
              disabled={!acima}
              aria-label={t("Move group up")}
              title={t("Move group up")}
              className="p-0.5 rounded theme-muted opacity-40 group-hover/ghead:opacity-100 hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-20 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronUp size={13} strokeWidth={2} />
            </button>
            <button
              type="button"
              onClick={(e) => moverPara(abaixo, e)}
              onKeyDown={(e) => e.stopPropagation()}
              disabled={!abaixo}
              aria-label={t("Move group down")}
              title={t("Move group down")}
              className="p-0.5 rounded theme-muted opacity-40 group-hover/ghead:opacity-100 hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-20 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronDown size={13} strokeWidth={2} />
            </button>
          </span>
        </div>
      )}

      <div
        className={`overflow-hidden transition-all duration-200 ${
          showHeader && collapsed ? "max-h-0 opacity-0" : "max-h-[9999px] opacity-100"
        }`}
        onDragOver={!showHeader ? handleDragOver : undefined}
        onDrop={!showHeader ? handleDrop : undefined}
      >
        {group.accounts.map((account) => (
          <AccountRow key={account.UserID} account={account} />
        ))}
      </div>
    </div>
  );
}
