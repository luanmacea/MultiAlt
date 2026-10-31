/**
 * Lógica pura da seleção em lista (Shift, Ctrl e arrastar por cima das linhas),
 * a mesma em toda lista de contas do app que não é a lista principal. O hook
 * `useListSelection` só liga isto aos eventos do DOM; as regras ficam aqui para
 * serem testadas sem tela. Ver docs/features/ui-layout.md ("Seleção em lista").
 */

export type ListKey = number | string;

/** O que um clique manda aplicar: `keys` passam para o estado `on`. */
export interface SelectionPlan<K extends ListKey> {
  keys: K[];
  on: boolean;
}

/** As linhas de `a` até `b`, inclusive, na ordem da tela. Vazio se uma delas não está na lista. */
export function rangeBetween<K extends ListKey>(order: readonly K[], a: K, b: K): K[] {
  const from = order.indexOf(a);
  const to = order.indexOf(b);
  if (from < 0 || to < 0) return [];
  const [lo, hi] = from <= to ? [from, to] : [to, from];
  return order.slice(lo, hi + 1);
}

/**
 * Clique numa linha. Sem Shift (com ou sem Ctrl/Cmd) vira o que a lista já
 * fazia: liga/desliga só essa linha. Com Shift, o novo estado da clicada vale
 * para o intervalo desde a última clicada (`anchor`). Linha travada é pulada;
 * clicar numa travada não faz nada.
 */
export function planClick<K extends ListKey>(
  order: readonly K[],
  anchor: K | null,
  key: K,
  isSelected: (key: K) => boolean,
  options: { shift?: boolean; isDisabled?: (key: K) => boolean } = {}
): SelectionPlan<K> | null {
  const isDisabled = options.isDisabled ?? (() => false);
  if (isDisabled(key)) return null;
  const on = !isSelected(key);
  if (options.shift && anchor !== null && anchor !== key && order.includes(anchor) && order.includes(key)) {
    return { keys: rangeBetween(order, anchor, key).filter((k) => !isDisabled(k)), on };
  }
  return { keys: [key], on };
}

/**
 * Arrastar: as linhas entre a de onde o mouse saiu e a que ele está agora, sem
 * repetir as que o arrasto já aplicou. Voltar o mouse não desfaz o que passou.
 */
export function planDrag<K extends ListKey>(
  order: readonly K[],
  start: K,
  current: K,
  alreadyApplied: ReadonlySet<K>,
  isDisabled: (key: K) => boolean = () => false
): K[] {
  return rangeBetween(order, start, current).filter((k) => !alreadyApplied.has(k) && !isDisabled(k));
}

/** Junta ou tira `keys` de uma lista de ids, sem repetir e sem mexer na ordem do resto. */
export function applyToList<K extends ListKey>(list: readonly K[], keys: readonly K[], on: boolean): K[] {
  if (on) {
    const have = new Set(list);
    return [...list, ...keys.filter((k) => !have.has(k))];
  }
  const drop = new Set(keys);
  return list.filter((k) => !drop.has(k));
}

/** Idem para um Set (devolve um novo; não muda o original). */
export function applyToSet<K extends ListKey>(set: ReadonlySet<K>, keys: readonly K[], on: boolean): Set<K> {
  const next = new Set(set);
  for (const k of keys) {
    if (on) next.add(k);
    else next.delete(k);
  }
  return next;
}

/**
 * A mão começou o arrasto em cima de quê? Botão, link ou campo que não seja a
 * própria linha nem uma caixa de seleção são controles de outra coisa (Mark,
 * Focus...): arrastar a partir deles não seleciona nada.
 */
export function startsOnOtherControl(target: Element | null, row: Element): boolean {
  const control = target?.closest("button, a, input, select, textarea");
  if (!control || control === row) return false;
  if (control.getAttribute("role") === "checkbox") return false;
  if (control instanceof HTMLInputElement && control.type === "checkbox") return false;
  return row.contains(control);
}
