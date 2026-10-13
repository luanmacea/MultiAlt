/**
 * Histórico de páginas do app, para Voltar/Avançar (botões laterais do mouse e
 * Alt+setas — ver `hooks/useMouseNavigation.ts`). Mesmo modelo do navegador:
 * uma lista e a posição atual; navegar para outro lugar depois de voltar apaga
 * o que estava à frente. Puro, para ser testado sem React.
 */
export interface PageHistory<P> {
  entries: P[];
  index: number;
}

export type PageHistoryAction<P> =
  | { type: "visit"; page: P }
  | { type: "back" }
  | { type: "forward" };

/** Teto do histórico: as mais antigas saem primeiro. */
export const PAGE_HISTORY_LIMIT = 50;

export function createPageHistory<P>(initial: P): PageHistory<P> {
  return { entries: [initial], index: 0 };
}

export function currentPage<P>(history: PageHistory<P>): P {
  return history.entries[history.index];
}

/**
 * Sem mudança, devolve o **mesmo** objeto — é assim que quem chama sabe que não
 * havia para onde ir (Voltar no começo, Avançar no fim).
 */
export function pageHistoryReducer<P>(history: PageHistory<P>, action: PageHistoryAction<P>): PageHistory<P> {
  switch (action.type) {
    case "back":
      return history.index > 0 ? { entries: history.entries, index: history.index - 1 } : history;
    case "forward":
      return history.index < history.entries.length - 1
        ? { entries: history.entries, index: history.index + 1 }
        : history;
    case "visit": {
      // A mesma página em que o histórico já está: repetição, ou a troca que o
      // próprio Voltar/Avançar pediu. Não mexe em nada (nem no Avançar).
      if (currentPage(history) === action.page) return history;
      const entries = [...history.entries.slice(0, history.index + 1), action.page];
      const trimmed = entries.length > PAGE_HISTORY_LIMIT ? entries.slice(entries.length - PAGE_HISTORY_LIMIT) : entries;
      return { entries: trimmed, index: trimmed.length - 1 };
    }
  }
}
