import { useEffect, useRef } from "react";

/**
 * Pilha LIFO de quem responde ao `Escape`.
 *
 * O app tinha 26 handlers de `Escape` espalhados por 23 arquivos, cada um num
 * `window`/`document.addEventListener` próprio, e **nenhum** interrompia a
 * propagação. Consequência: um Escape com um diálogo aberto sobre a Choose Game
 * fechava os dois (e a Choose Game era sempre a primeira a registrar, então
 * ganhava), e na lista de contas o Escape limpava a seleção junto com o menu de
 * contexto.
 *
 * Aqui existe **um** listener. Quem monta depois fica no topo — que é exatamente
 * a ordem visual: popover sobre diálogo sobre tela — e só o topo recebe o
 * Escape. A ordem de registro deixa de importar.
 */
interface EscapeEntry {
  fire: () => void;
  ignoreFromFields: boolean;
}

const stack: EscapeEntry[] = [];
let listening = false;

function isField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
}

function onKeyDown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  // Quem está mais perto do alvo trata primeiro e marca com `preventDefault()`
  // (é o que `usePrompt` faz): nesse caso o Escape já foi consumido, e o diálogo
  // atrás não pode fechar no mesmo evento.
  if (event.defaultPrevented) return;
  const top = stack[stack.length - 1];
  if (!top) return;
  if (top.ignoreFromFields && isField(event.target)) return;
  event.preventDefault();
  top.fire();
}

function ensureListening() {
  if (listening) return;
  window.addEventListener("keydown", onKeyDown);
  listening = true;
}

/**
 * @param active quando `false`, não entra na pilha (um popover fechado não pode
 *   roubar o Escape do diálogo que o contém).
 * @param onEscape o que fazer — fechar, cancelar, limpar seleção.
 * @param options `ignoreFromFields` deixa o Escape digitado dentro de um campo
 *   passar direto, para não fechar a tela inteira enquanto alguém edita um valor.
 */
export function useEscapeStack(
  active: boolean,
  onEscape: () => void,
  options: { ignoreFromFields?: boolean } = {}
) {
  const onEscapeRef = useRef(onEscape);
  const ignoreFromFields = options.ignoreFromFields ?? false;

  useEffect(() => {
    onEscapeRef.current = onEscape;
  }, [onEscape]);

  useEffect(() => {
    if (!active) return;
    const entry: EscapeEntry = {
      fire: () => onEscapeRef.current(),
      ignoreFromFields,
    };
    stack.push(entry);
    ensureListening();
    return () => {
      const index = stack.indexOf(entry);
      if (index >= 0) stack.splice(index, 1);
    };
  }, [active, ignoreFromFields]);
}

/** Só para teste: quantos estão na pilha agora (pega entrada pendurada). */
export function escapeStackDepth(): number {
  return stack.length;
}
