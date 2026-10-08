/**
 * Tipos e textos da página Groups. Espelham `commands/groups.rs` e
 * `api/roblox/groups.rs` (camelCase). Ver docs/features/groups.md.
 */

/** Um grupo da busca (`GroupSummary`). */
export interface GroupSummary {
  id: number;
  name: string;
  description: string;
  memberCount: number;
  /** `false` = o dono aprova cada pedido. */
  publicEntryAllowed: boolean;
  hasVerifiedBadge: boolean;
  isLocked: boolean;
}

export interface GroupSearchPage {
  groups: GroupSummary[];
  nextCursor: string | null;
}

export type GroupJoinStatus =
  | "waiting"
  | "joining"
  | "joined"
  | "pending"
  | "alreadyMember"
  | "challenge"
  | "failed"
  | "cancelled"
  | "notMember";

export interface GroupJoinAccountResult {
  userId: number;
  status: GroupJoinStatus;
  reason: string | null;
}

/** Retrato do lote, pelo evento `groups-join-state`. */
export interface GroupJoinSnapshot {
  running: boolean;
  groupId: number | null;
  groupName: string;
  total: number;
  done: number;
  currentUserId: number | null;
  accounts: GroupJoinAccountResult[];
}

export const IDLE_GROUP_JOIN: GroupJoinSnapshot = {
  running: false,
  groupId: null,
  groupName: "",
  total: 0,
  done: 0,
  currentUserId: null,
  accounts: [],
};

/** Rótulo (chave de tradução) e cor do selo de cada estado. */
export const STATUS_META: Record<GroupJoinStatus, { label: string; tone: string }> = {
  waiting: { label: "Waiting", tone: "theme-border bg-[var(--panel-soft)] theme-muted" },
  joining: { label: "Joining...", tone: "border-sky-500/30 bg-sky-500/15 text-sky-300" },
  joined: { label: "Joined", tone: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300" },
  pending: { label: "Pending approval", tone: "border-violet-500/30 bg-violet-500/15 text-violet-300" },
  alreadyMember: { label: "Already a member", tone: "border-emerald-500/30 bg-emerald-500/10 text-emerald-300/90" },
  challenge: { label: "Needs captcha", tone: "border-amber-500/30 bg-amber-500/15 text-amber-300" },
  failed: { label: "Failed", tone: "border-red-500/30 bg-red-500/15 text-red-300" },
  cancelled: { label: "Cancelled", tone: "theme-border bg-[var(--panel-soft)] theme-muted" },
  notMember: { label: "Not a member yet", tone: "border-amber-500/30 bg-amber-500/10 text-amber-300/90" },
};

/** Estados em que a pessoa ainda tem o que fazer no navegador da conta. */
export function needsBrowser(status: GroupJoinStatus | undefined): boolean {
  return status === "challenge" || status === "notMember";
}

/** Snapshot que veio torto (sem a lista) não derruba a tela. */
export function normalizeSnapshot(raw: unknown): GroupJoinSnapshot | null {
  if (!raw || typeof raw !== "object") return null;
  const value = raw as Partial<GroupJoinSnapshot>;
  return {
    ...IDLE_GROUP_JOIN,
    ...value,
    accounts: Array.isArray(value.accounts) ? value.accounts : [],
  };
}
