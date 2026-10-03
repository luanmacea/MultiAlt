import { useCallback } from "react";
import { useStore } from "../store";
import type { Account } from "../types";
import { accountInitial, accountLabel, hideAccountAvatar } from "../utils/accountName";

/**
 * O nome de conta que a tela mostra, já com o modo "Names hidden" aplicado.
 * Toda tela que mostra o nome de uma conta do usuário usa isto (ou
 * `accountLabel` de utils/accountName.ts, fora de componente) — ver a regra no
 * topo daquele arquivo.
 *
 * `fallback` é o que aparece sem a conta na lista (ex.: "User ID: 123").
 */
export function useAccountLabel(): (
  account: Pick<Account, "Alias" | "Username"> | null | undefined,
  fallback?: string | number
) => string {
  const { hideUsernames, hiddenNameLetters } = useStore();
  return useCallback(
    (account, fallback = "") => accountLabel(account, { hideUsernames, hiddenNameLetters }, fallback),
    [hideUsernames, hiddenNameLetters]
  );
}

/** A inicial do círculo sem foto, sem revelar o que a prévia esconderia. */
export function useAccountInitial(): (account: Pick<Account, "Username"> | null | undefined) => string {
  const { hideUsernames, hiddenNameLetters } = useStore();
  return useCallback(
    (account) => accountInitial(account, { hideUsernames, hiddenNameLetters }),
    [hideUsernames, hiddenNameLetters]
  );
}

/** Se a foto da conta some junto com o nome (nomes ocultos sem "mostrar avatares"). */
export function useHideAccountAvatar(): boolean {
  const { hideUsernames, showAvatarsWhenHidden } = useStore();
  return hideAccountAvatar({ hideUsernames, showAvatarsWhenHidden });
}
