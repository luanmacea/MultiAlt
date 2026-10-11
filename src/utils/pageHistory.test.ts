import { describe, expect, it } from "vitest";
import {
  PAGE_HISTORY_LIMIT,
  createPageHistory,
  currentPage,
  pageHistoryReducer,
  type PageHistory,
} from "./pageHistory";

function visitAll(history: PageHistory<string>, pages: string[]): PageHistory<string> {
  return pages.reduce((h, page) => pageHistoryReducer(h, { type: "visit", page }), history);
}

/**
 * Histórico das páginas para os botões laterais do mouse (Voltar/Avançar) e
 * Alt+setas: o mesmo modelo do navegador, só que dentro do app.
 */
describe("pageHistoryReducer", () => {
  it("começa só com a página inicial, sem voltar nem avançar", () => {
    const h = createPageHistory("accounts");
    expect(currentPage(h)).toBe("accounts");
    expect(pageHistoryReducer(h, { type: "back" })).toBe(h);
    expect(pageHistoryReducer(h, { type: "forward" })).toBe(h);
  });

  it("volta e avança pelas páginas visitadas", () => {
    let h = visitAll(createPageHistory("accounts"), ["settings", "session"]);
    expect(currentPage(h)).toBe("session");

    h = pageHistoryReducer(h, { type: "back" });
    expect(currentPage(h)).toBe("settings");
    h = pageHistoryReducer(h, { type: "back" });
    expect(currentPage(h)).toBe("accounts");
    // Nada antes da primeira: fica onde está.
    expect(pageHistoryReducer(h, { type: "back" })).toBe(h);

    h = pageHistoryReducer(h, { type: "forward" });
    expect(currentPage(h)).toBe("settings");
    h = pageHistoryReducer(h, { type: "forward" });
    expect(currentPage(h)).toBe("session");
    expect(pageHistoryReducer(h, { type: "forward" })).toBe(h);
  });

  it("navegar para outro lugar depois de voltar apaga o que estava à frente", () => {
    let h = visitAll(createPageHistory("accounts"), ["settings", "session"]);
    h = pageHistoryReducer(h, { type: "back" });
    h = pageHistoryReducer(h, { type: "visit", page: "theme" });

    expect(currentPage(h)).toBe("theme");
    expect(h.entries).toEqual(["accounts", "settings", "theme"]);
    expect(pageHistoryReducer(h, { type: "forward" })).toBe(h);
  });

  it("a mesma página duas vezes seguidas não vira duas entradas", () => {
    const h = visitAll(createPageHistory("accounts"), ["settings", "settings"]);
    expect(h.entries).toEqual(["accounts", "settings"]);
    expect(pageHistoryReducer(h, { type: "visit", page: "settings" })).toBe(h);
  });

  it("visitar a página em que o histórico já está (voltar/avançar) não apaga o que está à frente", () => {
    let h = visitAll(createPageHistory("accounts"), ["settings", "session"]);
    h = pageHistoryReducer(h, { type: "back" });
    // O hook vê a troca de página que ele mesmo pediu.
    const same = pageHistoryReducer(h, { type: "visit", page: "settings" });
    expect(same).toBe(h);
    expect(currentPage(pageHistoryReducer(same, { type: "forward" }))).toBe("session");
  });

  it("guarda no máximo o limite, descartando as mais antigas", () => {
    const pages = Array.from({ length: PAGE_HISTORY_LIMIT + 10 }, (_, i) => `p${i}`);
    let h = visitAll(createPageHistory("start"), pages);
    expect(h.entries).toHaveLength(PAGE_HISTORY_LIMIT);
    expect(currentPage(h)).toBe(`p${PAGE_HISTORY_LIMIT + 9}`);
    expect(h.entries[0]).toBe("p10");

    for (let i = 0; i < PAGE_HISTORY_LIMIT + 5; i++) h = pageHistoryReducer(h, { type: "back" });
    expect(currentPage(h)).toBe("p10");
  });
});
