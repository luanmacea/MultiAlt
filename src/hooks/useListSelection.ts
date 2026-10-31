import { useCallback, useEffect, useId, useRef } from "react";
import {
  planClick,
  planDrag,
  startsOnOtherControl,
  type ListKey,
} from "../utils/listSelection";

/**
 * Seleção em lista com o mesmo jeito da lista principal de contas, para toda
 * lista de contas com caixinhas:
 * - clique liga/desliga só a linha (o que cada lista já fazia; Ctrl/Cmd+clique
 *   é o mesmo);
 * - Shift+clique aplica o novo estado da clicada a todas as linhas desde a
 *   última clicada;
 * - arrastar o mouse por cima das linhas (seleção por área) aplica o estado
 *   oposto ao da linha em que o arrasto começou a cada linha por onde passa.
 *
 * Linha travada (`isDisabled`) é pulada, e a lista toda travada (`disabled`)
 * não responde. O teclado segue o do controle da linha: Espaço/Enter dispara o
 * clique (com Shift, o intervalo).
 *
 * Uso: espalhe `rowProps(key)` no elemento da linha e chame `toggle(key, event)`
 * no clique/alteração do controle. `apply(keys, on)` é quem de fato muda o
 * estado da lista (pode ser assíncrono; chamadas seguidas durante o arrasto
 * chegam em ordem).
 */
export interface ListSelectionOptions<K extends ListKey> {
  /** As linhas como aparecem na tela (é daqui que sai o intervalo do Shift). */
  order: readonly K[];
  isSelected: (key: K) => boolean;
  /** Passa `keys` para o estado `on`. */
  apply: (keys: K[], on: boolean) => void;
  isDisabled?: (key: K) => boolean;
  /** A lista inteira travada (ex.: sessão em andamento). */
  disabled?: boolean;
}

interface DragState<K extends ListKey> {
  start: K;
  on: boolean;
  applied: Set<K>;
  dragging: boolean;
}

export function useListSelection<K extends ListKey>(options: ListSelectionOptions<K>) {
  const scope = useId();
  const latest = useRef(options);
  latest.current = options;
  const anchor = useRef<K | null>(null);
  /** Shift no mousedown: o clique que vem do rótulo de uma caixa nativa não o carrega em todo navegador. */
  const shiftAtMouseDown = useRef(false);
  const suppressClick = useRef(false);
  const cleanup = useRef<(() => void) | null>(null);

  useEffect(() => () => cleanup.current?.(), []);

  const keyFromAttr = useCallback((raw: string | null): K | undefined => {
    if (raw === null) return undefined;
    return latest.current.order.find((k) => String(k) === raw);
  }, []);

  const toggle = useCallback((key: K, event?: { shiftKey?: boolean } | Event | null) => {
    if (suppressClick.current) {
      suppressClick.current = false;
      return;
    }
    const opts = latest.current;
    if (opts.disabled) return;
    const shift = Boolean((event as { shiftKey?: boolean } | null | undefined)?.shiftKey) || shiftAtMouseDown.current;
    shiftAtMouseDown.current = false;
    const plan = planClick(opts.order, anchor.current, key, opts.isSelected, {
      shift,
      isDisabled: opts.isDisabled,
    });
    if (!plan) return;
    anchor.current = key;
    // Shift+clique não deve deixar o texto do intervalo pintado.
    if (plan.keys.length > 1) window.getSelection?.()?.removeAllRanges();
    if (plan.keys.length > 0) opts.apply(plan.keys, plan.on);
  }, []);

  const onMouseDown = useCallback(
    (event: React.MouseEvent<HTMLElement>, key: K) => {
      const opts = latest.current;
      shiftAtMouseDown.current = event.shiftKey;
      if (opts.disabled || event.button !== 0 || opts.isDisabled?.(key)) return;
      if (startsOnOtherControl(event.target as Element, event.currentTarget)) return;

      cleanup.current?.();
      const drag: DragState<K> = { start: key, on: !opts.isSelected(key), applied: new Set(), dragging: false };

      const onMove = (ev: MouseEvent) => {
        const row = (ev.target as Element | null)?.closest?.(`[data-sel-scope="${scope}"]`);
        const current = keyFromAttr(row?.getAttribute("data-sel-key") ?? null);
        if (current === undefined) return;
        if (!drag.dragging && current === drag.start) return;
        const now = latest.current;
        if (!drag.dragging) {
          drag.dragging = true;
          document.body.style.userSelect = "none";
          window.getSelection?.()?.removeAllRanges();
        }
        const keys = planDrag(now.order, drag.start, current, drag.applied, now.isDisabled);
        if (keys.length === 0) return;
        for (const k of keys) drag.applied.add(k);
        anchor.current = current;
        now.apply(keys, drag.on);
      };

      const finish = () => {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        document.body.style.userSelect = "";
        cleanup.current = null;
      };
      const onUp = () => {
        const dragged = drag.dragging;
        finish();
        if (dragged) {
          // O clique que o navegador manda a seguir não é um segundo toque.
          suppressClick.current = true;
          window.setTimeout(() => {
            suppressClick.current = false;
          }, 0);
        }
        window.setTimeout(() => {
          shiftAtMouseDown.current = false;
        }, 0);
      };

      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
      cleanup.current = finish;
    },
    [keyFromAttr, scope]
  );

  /** Atributos da linha: marca para o arrasto reconhecer e começa o arrasto. */
  const rowProps = useCallback(
    (key: K) => ({
      "data-sel-scope": scope,
      "data-sel-key": String(key),
      onMouseDown: (event: React.MouseEvent<HTMLElement>) => onMouseDown(event, key),
    }),
    [onMouseDown, scope]
  );

  return { rowProps, toggle };
}
