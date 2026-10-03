/**
 * Cache em memória que vive enquanto o app está aberto.
 *
 * Existe para as abas da Choose Game (Games, Servers, Friends): sair de uma aba
 * desmontava o componente e a volta começava do zero — tela vazia, spinner e a
 * mesma consulta de novo. Com o cache, a aba volta mostrando o que já tinha e
 * atualiza por trás (ver `docs/features/ui-layout.md`, "Choose Game").
 *
 * Regras:
 *
 * - **Só memória.** Nada vai para o disco nem para o `localStorage`: é dado da
 *   rede (servidores, amigos online), que envelhece rápido. Fechou o app, sumiu.
 * - **Não poupa requisição por conta própria** — a aba continua consultando ao
 *   voltar, como antes. O que muda é que a tela não fica vazia enquanto isso.
 * - **Tamanho limitado** (`maxEntries`): a chave inclui place, seleção de contas
 *   etc., então sem teto o cache cresceria a cada jogo aberto. Ao passar do
 *   teto sai a chave usada há mais tempo.
 * - `clearSessionCaches()` zera todos — os testes chamam isso pelo
 *   `resetTauriMocks()`, senão o dado de um teste vazaria para o seguinte.
 */
export class SessionCache<T> {
  private readonly entries = new Map<string, T>();

  constructor(private readonly maxEntries = 20) {
    registry.add(this);
  }

  get(key: string): T | undefined {
    return this.entries.get(key);
  }

  has(key: string): boolean {
    return this.entries.has(key);
  }

  /** Grava e marca a chave como a mais recente. */
  set(key: string, value: T): void {
    this.entries.delete(key);
    this.entries.set(key, value);
    while (this.entries.size > this.maxEntries) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) break;
      this.entries.delete(oldest);
    }
  }

  delete(key: string): void {
    this.entries.delete(key);
  }

  clear(): void {
    this.entries.clear();
  }

  get size(): number {
    return this.entries.size;
  }
}

const registry = new Set<SessionCache<unknown>>();

/** Esquece tudo que as abas guardaram nesta sessão. */
export function clearSessionCaches(): void {
  for (const cache of registry) cache.clear();
}
