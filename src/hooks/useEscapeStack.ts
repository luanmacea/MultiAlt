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
  closesOnBack: boolean;
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

/**
 * O botão Voltar do mouse (e Alt+Esquerda) fecha o que está aberto por cima
 * exatamente como o Escape fecharia — ver `useMouseNavigation`. Só o **topo**
 * conta: se ele é a base de uma tela (`closesOnBack: false` — a casca da página,
 * a seleção da lista de contas), não há nada "por cima", e quem age é o
 * histórico de páginas. Devolve `true` quando fechou alguma coisa.
 */
export function fireEscapeForBack(): boolean {
  const top = stack[stack.length - 1];
  if (!top || !top.closesOnBack) return false;
  top.fire();
  return true;
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
 *   `closesOnBack: false` marca a base de uma tela (sair da página, limpar a
 *   seleção): o botão Voltar do mouse não a dispara e anda no histórico de
 *   páginas no lugar (ver `fireEscapeForBack`).
 */
export function useEscapeStack(
  active: boolean,
  onEscape: () => void,
  options: { ignoreFromFields?: boolean; closesOnBack?: boolean } = {}
) {
  const onEscapeRef = useRef(onEscape);
  const ignoreFromFields = options.ignoreFromFields ?? false;
  const closesOnBack = options.closesOnBack ?? true;

  useEffect(() => {
    onEscapeRef.current = onEscape;
  }, [onEscape]);

  useEffect(() => {
    if (!active) return;
    const entry: EscapeEntry = {
      fire: () => onEscapeRef.current(),
      ignoreFromFields,
      closesOnBack,
    };
    stack.push(entry);
    ensureListening();
    return () => {
      const index = stack.indexOf(entry);
      if (index >= 0) stack.splice(index, 1);
    };
  }, [active, ignoreFromFields, closesOnBack]);
}

/** Só para teste: quantos estão na pilha agora (pega entrada pendurada). */
export function escapeStackDepth(): number {
  return stack.length;
}
