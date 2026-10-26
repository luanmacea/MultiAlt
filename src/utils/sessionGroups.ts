import type { ClientHealth, CurrentSession } from "../types";

/**
 * Como a lista "Em jogo" da página Session se organiza (pedido do dono,
 * 11/10/2026: quem tem muitas contas quer ver quem está junto de quem). Só
 * dados: a tela decide como desenhar.
 *
 * - Quem precisa de atenção (reconectando, não respondendo, caiu) sai do grupo
 *   do jogo e vai para o topo: a sessão de quem caiu já fechou, e é ali que a
 *   pessoa olha primeiro.
 * - O resto, por jogo (place da sessão de agora) e, dentro dele, por servidor
 *   (Job ID): contas no mesmo servidor ficam juntas.
 * - Sem sessão (ainda sem jogo, ou fora do Windows, onde o log não é lido),
 *   num grupo à parte.
 */

export type RunningStatus =
  | "reconnecting"
  | "notResponding"
  | "dropped"
  | "humanCheck"
  | "playing"
  | "waiting"
  | "unknown";

/** O estado de uma conta com cliente aberto, do mais urgente ao mais calmo. */
export function runningStatus(
  session: CurrentSession | undefined,
  health: ClientHealth | undefined,
  reconnecting: boolean
): RunningStatus {
  if (reconnecting) return "reconnecting";
  if (health?.notResponding) return "notResponding";
  if (health?.drop) return "dropped";
  // A verificação do Roblox espera a pessoa: sem ela, a conta não entra.
  if (health?.challenge) return "humanCheck";
  if (session) return "playing";
  if (health?.logFound && !health.inGame) return "waiting";
  return "unknown";
}

export function needsAttention(status: RunningStatus): boolean {
  return status === "reconnecting" || status === "notResponding" || status === "dropped" || status === "humanCheck";
}

export interface ServerGroup {
  /** Chave estável do grupo (o Job ID, ou "unknown"). */
  key: string;
  jobId: string | null;
  /** `true` privado/VIP, `false` público, `null` não se sabe. */
  privateServer: boolean | null;
  userIds: number[];
}

export interface GameGroup {
  key: string;
  placeId: number;
  userIds: number[];
  servers: ServerGroup[];
}

export interface InGameGroups {
  attention: number[];
  games: GameGroup[];
  noGame: number[];
}

/**
 * Agrupa as contas com cliente aberto. `runningIds` já vem na ordem da lista de
 * contas (estável entre polls); dentro de cada grupo essa ordem é mantida. Os
 * jogos e os servidores com mais contas vêm primeiro; empate fica na ordem em
 * que apareceram.
 */
export function groupRunning(
  runningIds: number[],
  sessions: ReadonlyMap<number, CurrentSession>,
  statusOf: (userId: number) => RunningStatus
): InGameGroups {
  const attention: number[] = [];
  const noGame: number[] = [];
  const games = new Map<number, GameGroup>();

  for (const userId of runningIds) {
    if (needsAttention(statusOf(userId))) {
      attention.push(userId);
      continue;
    }
    const session = sessions.get(userId);
    if (!session) {
      noGame.push(userId);
      continue;
    }
    let game = games.get(session.placeId);
    if (!game) {
      game = { key: `place-${session.placeId}`, placeId: session.placeId, userIds: [], servers: [] };
      games.set(session.placeId, game);
    }
    game.userIds.push(userId);
    const serverKey = session.jobId ?? "unknown";
    let server = game.servers.find((s) => s.key === serverKey);
    if (!server) {
      server = { key: serverKey, jobId: session.jobId, privateServer: session.privateServer, userIds: [] };
      game.servers.push(server);
    }
    server.userIds.push(userId);
  }

  const bySize = <T extends { userIds: number[] }>(list: T[]) =>
    list
      .map((item, index) => ({ item, index }))
      .sort((a, b) => b.item.userIds.length - a.item.userIds.length || a.index - b.index)
      .map(({ item }) => item);

  const sortedGames = bySize([...games.values()]).map((game) => ({ ...game, servers: bySize(game.servers) }));
  return { attention, games: sortedGames, noGame };
}

/** Job ID curto para o cabeçalho do servidor ("1a2b3c4d"); o inteiro vai no tooltip. */
export function shortJobId(jobId: string): string {
  return jobId.slice(0, 8);
}
