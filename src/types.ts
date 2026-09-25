export interface Account {
  Valid: boolean;
  SecurityToken: string;
  Username: string;
  LastUse: string;
  Alias: string;
  Description: string;
  Password: string;
  Group: string;
  UserID: number;
  Fields: Record<string, string>;
  LastAttemptedRefresh: string;
  BrowserTrackerID: string;
}

export interface ThemeData {
  accounts_background: string;
  accounts_foreground: string;
  buttons_background: string;
  buttons_foreground: string;
  buttons_border: string;
  toggle_on_background?: string;
  toggle_off_background?: string;
  toggle_knob_background?: string;
  forms_background: string;
  forms_foreground: string;
  textboxes_background: string;
  textboxes_foreground: string;
  textboxes_border: string;
  label_background: string;
  label_foreground: string;
  label_transparent: boolean;
  dark_top_bar: boolean;
  show_headers: boolean;
  light_images: boolean;
  button_style: string;
  font_sans?: ThemeFontSpec;
  font_mono?: ThemeFontSpec;
}

export type ThemeFontSource = "google" | "local" | "system";

export interface ThemeFontGoogleSpec {
  weights: number[];
}

export interface ThemeFontLocalSpec {
  file: string; // stored under runtime `RAMThemeFonts/`
  weight: number;
  style: "normal" | "italic";
}

export interface ThemeFontSpec {
  source: ThemeFontSource;
  family: string;
  fallbacks: string[];
  google?: ThemeFontGoogleSpec;
  local?: ThemeFontLocalSpec;
}

export interface ThumbnailData {
  targetId: number;
  imageUrl: string | null;
  state: string;
}

export interface ParsedGroup {
  key: string;
  displayName: string;
  sortKey: number;
  accounts: Account[];
}

export interface PlatformCapabilities {
  os: string;
  sessionType: string;
  preferredRunner: string;
  detectedRunner: string;
  runnerPath: string | null;
  supportsSingleLaunch: boolean;
  supportsMultiLaunch: boolean;
  supportsWatcher: boolean;
  supportsWatcherMemory: boolean;
  supportsWindowControls: boolean;
  supportsBotting: boolean;
  supportsUpdater: boolean;
  supportsClientSettings: boolean;
  reasons: string[];
  warnings: string[];
}

/** Kind of target a pasted join link resolved to. */
export type JoinTargetKind = "invite" | "private" | "job" | "place";

/**
 * Launchable target resolved from a pasted Roblox link by the
 * `resolve_join_link` command (experience invites, VIP/private servers,
 * plain game links, `roblox://` deep links and `ro.blox.com` short links).
 */
export interface JoinTarget {
  kind: JoinTargetKind;
  placeId: number;
  /** Roblox `gameInstanceId`; empty when the link carries none. */
  jobId: string;
  accessCode: string;
  linkCode: string;
  launchData: string;
  inviterId: number | null;
  /** Non-fatal warning, e.g. "Expired" / "InviterNotInExperience". */
  note: string | null;
}

/**
 * Amigo **online** de uma conta, como o backend devolve em
 * `get_online_friends` / `get_online_friends_for_accounts`.
 *
 * `presenceType`: `0` offline, `1` online no site, `2` em jogo, `3` no Studio.
 * `gameId` é o Job ID do servidor; vem `null` quando a privacidade do amigo
 * esconde o servidor — nesse caso não há como entrar junto.
 */
export interface OnlineFriend {
  userId: number;
  name: string;
  displayName: string;
  presenceType: number;
  lastLocation: string;
  placeId: number | null;
  /** Place "raiz" da experiência; tem prioridade sobre `placeId` no launch. */
  rootPlaceId: number | null;
  gameId: string | null;
}

/**
 * Amigos online de UMA conta selecionada. `error` isola a falha daquela conta
 * para que uma conta com cookie morto não apague a lista das outras.
 */
export interface AccountFriends {
  userId: number;
  friends: OnlineFriend[];
  error: string | null;
}

/** Payload do evento `friends-online-progress` (contas já consultadas). */
export interface FriendsOnlineProgress {
  done: number;
  total: number;
}

/**
 * Um numero no comeco do nome do grupo ordena o grupo e some da exibicao
 * (`1 Main` mostra `Main`) — convencao herdada do RAM antigo.
 *
 * Pega a sequencia INTEIRA de digitos, nao um pedaco dela: parando em 3, o nome
 * "2024 Alts" aparecia como "4 Alts" e o usuario via o proprio texto partido.
 */
export function parseGroupName(group: string): { sortKey: number; displayName: string } {
  const match = group.match(/^(\d+)\s*/);
  if (match) {
    const remainder = group.slice(match[0].length);
    return { sortKey: parseInt(match[1], 10), displayName: remainder || group };
  }
  return { sortKey: 999999, displayName: group };
}

export function timeAgo(dateStr: string): string {
  if (!dateStr) return "never";
  const date = new Date(dateStr);
  const diff = Date.now() - date.getTime();
  if (diff < 0) return "just now";
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  if (days > 365) return `${Math.floor(days / 365)}y`;
  if (days > 30) return `${Math.floor(days / 30)}mo`;
  if (days > 0) return `${days}d`;
  if (hours > 0) return `${hours}h`;
  if (minutes > 0) return `${minutes}m`;
  return "now";
}

export function getFreshnessColor(lastUse: string): string | null {
  if (!lastUse) return "#fa1a0d";
  const days = (Date.now() - new Date(lastUse).getTime()) / 86400000;
  if (days < 20) return null;
  const t = Math.min((days - 20) / 10, 1);
  const r = Math.round(255 + (250 - 255) * t);
  const g = Math.round(204 + (26 - 204) * t);
  const b = Math.round(77 + (13 - 77) * t);
  return `rgb(${r},${g},${b})`;
}

/**
 * Um arquivo de backup gravado na pasta de backups.
 * Espelha `BackupEntry` em `src-tauri/src/commands/backups.rs` (serde camelCase).
 */
export interface BackupEntry {
  id: string;
  fileName: string;
  /** ISO-8601 de quando o backup foi criado. */
  createdAt: string;
  /** Rótulo do usuário; `null` quando o backup foi criado sem nome. */
  label: string | null;
  sizeBytes: number;
  /** Nomes dos arquivos de dados guardados dentro do backup. */
  files: string[];
  /** `false` quando o arquivo está corrompido/incompleto — não pode ser restaurado. */
  valid: boolean;
  /** `true` para backups criados pelo próprio app (segurança / retenção). */
  automatic?: boolean;
}

/** Resultado de `restore_backup`. */
export interface RestoreReport {
  backupId: string;
  /** Backup automático dos dados substituídos; `null` se não deu para criar. */
  safetyBackupId: string | null;
  /** Arquivos efetivamente gravados de volta. */
  restored: string[];
  /** Arquivos do zip que foram ignorados (caminho inesperado, falha de escrita). */
  skipped: string[];
  accountsReloaded: boolean;
  requiresRestart: boolean;
  /** Por que o reinício é necessário (um motivo por arquivo). */
  restartReasons: string[];
}

/** Onde os backups moram e quanto ocupam (`backups_info`). */
export interface BackupsInfo {
  dir: string;
  /** `true` quando os dados ainda vivem ao lado do executável. */
  portable: boolean;
  totalBytes: number;
  count: number;
}

/**
 * Fila de launch (Painel de Sessão).
 *
 * O backend mantém uma entrada por conta do lote atual e emite o evento
 * `launch-queue` com o payload completo a cada mudança de estado.
 */
export type LaunchQueueState = "queued" | "launching" | "done" | "failed" | "cancelled";

export interface LaunchQueueEntry {
  userId: number;
  state: LaunchQueueState;
  /** Mensagem de erro quando `state === "failed"`; `null` nos demais estados. */
  error: string | null;
  updatedAtMs: number;
}

export interface LaunchQueuePayload {
  entries: LaunchQueueEntry[];
  /** `true` enquanto a fila está processando contas. */
  active: boolean;
  placeId: number;
  jobId: string;
}

// ── Escolha de servidor ──────────────────────────────────────────────────────

/**
 * Preferência de servidor do lote.
 *
 * `bestfit` é o padrão: o servidor **mais cheio que ainda caiba o lote com uma
 * vaga de folga** — joga junto de outras pessoas sem arriscar deixar conta de
 * fora. `none` é o comportamento antigo (Job ID vazio, o Roblox escolhe).
 *
 * Todas menos `none` resolvem UM servidor para o lote inteiro antes de lançar,
 * para as contas caírem juntas — ver `pick_server` no backend.
 */
export type ServerPreference = "bestfit" | "random" | "emptiest" | "fullest" | "none";

/** Geolocalização do IP da máquina que hospeda o servidor. */
export interface IpRegion {
  ip: string;
  city: string;
  /** Estado/província, quando o serviço informa. */
  region: string;
  country: string;
  countryCode: string;
}

/** Região de um servidor (`get_server_regions`). */
export interface ServerRegion {
  jobId: string;
  region: IpRegion | null;
  /** Texto já formatado por `General.ServerRegionFormat`. */
  label: string;
  error: string | null;
}

/** Progresso do evento `server-region-progress`. */
export interface ServerRegionProgress {
  done: number;
  total: number;
}

/** Resultado de `pick_server`. */
export interface PickedServer {
  jobId: string;
  playing: number;
  maxPlayers: number;
  region: ServerRegion | null;
  /** `true` quando não havia servidor na região pedida e a escolha caiu no
   * melhor disponível — a UI pergunta antes de entrar. */
  regionFallback: boolean;
}

// ── Criação de contas no navegador ───────────────────────────────────────────

/** Identidade gerada para um cadastro (`SignupIdentity` no backend). */
export interface SignupIdentity {
  username: string;
  password: string;
  /** Dia com dois dígitos, como o `<select>` do Roblox espera. */
  day: string;
  /** Mês em três letras em inglês — é o `value` do `<select>`, não o rótulo. */
  month: string;
  year: string;
  gender: string;
}

/** Estado da sessão de cadastro (evento `signup-progress`). */
export interface SignupStatus {
  active: boolean;
  /** Conta atual (1-based) e total pedido. */
  current: number;
  total: number;
  created: number;
  /** `idle` | `opening` | `filling` | `waiting-user` | `saving` | `stopping` | `done` | `error` */
  phase: string;
  identity: SignupIdentity | null;
  lastError: string | null;
  createdUsernames: string[];
}

/**
 * Estado do "lembrar de mim" da tela de senha (`remembered_unlock_state`).
 *
 * A senha fica cifrada pelo DPAPI do usuário do Windows, com o prazo dentro do
 * blob — ver `data/accounts/remember.rs`.
 */
export interface RememberState {
  /** `false` fora do Windows: sem proteção do SO, a caixa não aparece. */
  supported: boolean;
  /** Há um lembrete guardado agora. */
  active: boolean;
  defaultHours: number;
}

/**
 * Uma página da varredura de servidores (evento `server-scan`).
 *
 * A lista vem **já ordenada pelo backend** e cresce a cada página: num jogo
 * grande as primeiras páginas podem não ter nenhum servidor que caiba o lote.
 */
export interface ServerScanUpdate {
  scanId: number;
  placeId: number;
  servers: {
    id: string;
    playing: number;
    maxPlayers: number;
    ping?: number | null;
  }[];
  /** Quantos servidores foram examinados até agora. */
  scanned: number;
  /** Quantos deles cabem o lote inteiro. */
  fitting: number;
  done: boolean;
  /** Parou por bater o limite de páginas, não por acabarem os servidores. */
  stoppedAtLimit: boolean;
  error: string | null;
}
