import { useStore } from "../store";
import { useConfirm, usePrompt } from "./usePrompt";
import { useTr } from "../i18n/text";
import { groupLabel } from "../types";

/**
 * O grupo `Default` é o implícito (sem grupo no arquivo) e não se renomeia —
 * "renomeá-lo" seria mover todas as contas sem grupo. `__all__` é o grupo
 * sintético da lista sem cabeçalho.
 */
export function canRenameGroup(groupKey: string | null | undefined): groupKey is string {
  const key = (groupKey ?? "").trim();
  return key !== "" && key !== "Default" && key !== "__all__";
}

/**
 * "Rename Group…": pergunta o nome novo (o campo já vem com o nome atual,
 * prefixo de ordem incluído) e, se ele já for de outro grupo, confirma a junção
 * antes. Usado pelo menu de contexto da conta e pelo lápis do cabeçalho do grupo.
 */
export function useRenameGroup() {
  const store = useStore();
  const t = useTr();
  const prompt = usePrompt();
  const confirm = useConfirm();

  return async function renameGroupFlow(groupKey: string): Promise<boolean> {
    if (!canRenameGroup(groupKey)) return false;
    const entered = await prompt(t("New name for the group:"), groupKey);
    if (entered === null) return false;
    const name = entered.trim();
    if (!name || name === groupKey) return false;

    const existing = new Set(store.accounts.map((a) => a.Group || "Default"));
    if (existing.has(name)) {
      const ok = await confirm(
        t('A group named "{{target}}" already exists. Move the accounts of "{{group}}" into it?', {
          target: groupLabel(name, t),
          group: groupLabel(groupKey, t),
        })
      );
      if (!ok) return false;
    }
    return store.renameGroup(groupKey, name);
  };
}
