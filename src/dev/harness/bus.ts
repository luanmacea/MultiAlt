/**
 * Barramento do harness de UI.
 *
 * O harness roda o **frontend de verdade** no navegador, com o lado Tauri
 * trocado por dublês (ver `vite.config.ts`, alias sob `UI_HARNESS=1`). Serve
 * para validar a interface — ordenação de lista, estados de carregamento,
 * mensagens — sem compilar o app nem tocar na conta de ninguém.
 *
 * Não substitui os testes: ele existe para **procurar** problemas que só
 * aparecem com a tela montada e dados chegando aos poucos. O que ele achar
 * vira teste depois.
 */

export type InvokeHandler = (cmd: string, args: Record<string, unknown>) => unknown;

interface HarnessState {
  handler: InvokeHandler;
  listeners: Map<string, Set<(payload: unknown) => void>>;
  /** Tudo que a UI chamou, na ordem — o agente lê isto para conferir o fluxo. */
  calls: { cmd: string; args: Record<string, unknown> }[];
}

const state: HarnessState = {
  handler: () => undefined,
  listeners: new Map(),
  calls: [],
};

export function setInvokeHandler(handler: InvokeHandler): void {
  state.handler = handler;
}

export function harnessInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  state.calls.push({ cmd, args });
  try {
    return Promise.resolve(state.handler(cmd, args));
  } catch (error) {
    return Promise.reject(error);
  }
}

export function harnessListen(
  event: string,
  handler: (payload: unknown) => void
): () => void {
  const set = state.listeners.get(event) ?? new Set();
  set.add(handler);
  state.listeners.set(event, set);
  return () => set.delete(handler);
}

/** Publica um evento para a UI, como o backend faria. */
export function harnessEmit(event: string, payload: unknown): void {
  for (const handler of state.listeners.get(event) ?? []) handler(payload);
}

export function harnessCalls(): { cmd: string; args: Record<string, unknown> }[] {
  return state.calls;
}

export function resetHarnessCalls(): void {
  state.calls = [];
}
