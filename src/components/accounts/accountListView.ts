import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { tr } from "../../i18n/text";

/**
 * Como a lista de contas é desenhada (`General.AccountListView`): `list`
 * (linhas, o padrão de sempre) ou `cards` (grade de cards com a foto e o nome
 * maiores, como os amigos no site do Roblox).
 *
 * Pedido do dono (14/10/2026): no monitor grande a linha cheia deixa quase
 * toda a largura vazia. Fica como opção na barra de cima (botão ao lado de
 * "Nomes visíveis") e não como padrão: a lista em linhas continua sendo o que
 * cabe melhor em janela pequena.
 */
export type AccountListView = "list" | "cards";

export function normalizeAccountListView(raw: string | null | undefined): AccountListView {
  return (raw ?? "").trim().toLowerCase() === "cards" ? "cards" : "list";
}

export function useAccountListView(): [AccountListView, (view: AccountListView) => void] {
  const store = useStore();
  const view = normalizeAccountListView(store.settings?.General?.AccountListView);
  const set = (next: AccountListView) => {
    invoke("update_setting", { section: "General", key: "AccountListView", value: next })
      .then(() => store.reloadSettings())
      .catch((e) => store.addToast(tr("Could not save: {{error}}", { error: String(e) })));
  };
  return [view, set];
}
