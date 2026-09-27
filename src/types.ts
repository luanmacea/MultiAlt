/** Teto do apelido: acima disso a UI corta ou quebra a linha, conforme `WrapLongNames`. */
export const MAX_ALIAS_LENGTH = 240;

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

/**
 * Espelho de `SafeModeReport` em `src-tauri/src/lib.rs` (`get_webview_safe_mode`).
 *
 * `active`: este boot está com a aceleração de vídeo desligada.
 * `sticky`: existe o marcador `webview.safemode` em disco, então a próxima
 * abertura também vem em safe mode — é o que decide se vale oferecer a saída.
 */
export interface WebviewSafeModeState {
  active: boolean;
  sticky: boolean;
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

/**
 * Nomes de grupo distintos entre as contas, para oferecer como sugestão
 * (`<datalist>`) num campo que ainda precisa ficar editável para criar grupo
 * novo — mesma fonte que `BottomActionBar.tsx` usa em `allGroups` (antes cada
 * tela calculava a sua, e a sidebar de multi-seleção, hoje apagada, tinha a
 * terceira cópia).
 *
 * Devolve o texto **cru** de `Account.Group`, nunca o resultado de
 * `parseGroupName`: essa função só existe para decidir ordenação/rótulo de
 * exibição de um grupo já formado, e passar o `displayName` dela para uma
 * opção de sugestão foi o bug de P0 — a pessoa escolhia "Alts" na lista, mas
 * a conta continuava marcada com "10 Alts" (ou pior, ganhava um grupo novo
 * sem o prefixo). O agrupamento é por texto literal (`store.tsx`), então
 * "bloxgen" e "BloxGen" aparecem como duas opções diferentes de propósito.
 */
export function collectGroupNames(accounts: Account[]): string[] {
  const set = new Set<string>();
  for (const account of accounts) set.add(account.Group || "Default");
  return [...set].sort();
}

/**
 * Ordem manual dos grupos, como fica guardada em `General.GroupOrder`.
 *
 * **JSON, não lista separada por vírgula** (que é o padrão das outras chaves de
 * lista do INI): nome de grupo é texto livre digitado pelo usuário e pode conter
 * vírgula. O INI mantém tudo depois do primeiro `=`, então o JSON passa intacto.
 *
 * Valor estragado (chave apagada, texto que não é JSON, JSON que não é lista)
 * vira "sem ordem manual" em vez de erro: a lista de contas não pode deixar de
 * abrir por causa de uma linha torta no arquivo de settings.
 */
export function parseGroupOrder(raw: string | null | undefined): string[] {
  if (!raw) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const item of parsed) {
    if (typeof item !== "string" || !item || seen.has(item)) continue;
    seen.add(item);
    out.push(item);
  }
  return out;
}

export function serializeGroupOrder(keys: string[]): string {
  return JSON.stringify(keys);
}

/**
 * Ordem final dos grupos: primeiro os que o usuário arrastou (na ordem que ele
 * deixou), depois os que ele nunca tocou, na ordem automática de sempre
 * (prefixo numérico, e alfabética pelo nome exibido).
 *
 * Grupo que não existe mais **sai** da ordem manual, e grupo novo entra no fim
 * — não no meio, onde ninguém o pôs.
 */
export function orderGroupKeys(keys: Iterable<string>, manualOrder: string[]): string[] {
  const existing = new Set(keys);
  const manual = manualOrder.filter((key) => existing.has(key));
  const placed = new Set(manual);
  const rest = [...existing]
    .filter((key) => !placed.has(key))
    .map((key) => ({ key, ...parseGroupName(key) }))
    .sort((a, b) => a.sortKey - b.sortKey || a.displayName.localeCompare(b.displayName))
    .map((entry) => entry.key);
  return [...manual, ...rest];
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

/** A partir de quantos dias sem uso a conta ganha a bolinha de envelhecimento. */
export const AGED_AFTER_DAYS = 20;
/** Em quantos dias a rampa de cor chega no extremo (20 → 30 dias). */
const AGED_RAMP_DAYS = 10;
/** Âmbar do começo da rampa (20 dias). */
const AGED_FROM: [number, number, number] = [255, 204, 77];
/** Laranja do fim da rampa (30 dias ou mais). */
const AGED_TO: [number, number, number] = [249, 115, 22];
/** Extremos da rampa em CSS, para a legenda desenhar o degradê de verdade. */
export const AGED_COLOR_FROM = `rgb(${AGED_FROM.join(",")})`;
export const AGED_COLOR_TO = `rgb(${AGED_TO.join(",")})`;

/**
 * Cor da bolinha de "parada há muito tempo", ou `null` quando não há nada a dizer.
 *
 * A rampa termina em **laranja**, nunca em vermelho: vermelho é a bolinha de
 * sessão inválida, que é outro problema. Antes as duas ficavam visualmente
 * iguais e conta velha parecia conta quebrada.
 *
 * Conta sem `LastUse` não tem idade conhecida — fica sem bolinha, em vez de
 * nascer vermelha assim que é importada.
 */
export function getFreshnessColor(lastUse: string): string | null {
  if (!lastUse) return null;
  const days = (Date.now() - new Date(lastUse).getTime()) / 86400000;
  if (days < AGED_AFTER_DAYS) return null;
  const t = Math.min((days - AGED_AFTER_DAYS) / AGED_RAMP_DAYS, 1);
  const [r, g, b] = AGED_FROM.map((from, i) => Math.round(from + (AGED_TO[i] - from) * t));
  return `rgb(${r},${g},${b})`;
}

/**
 * Mascara o nome da conta quando o modo "Hidden" da toolbar está ligado.
 *
 * Mesma regra que a lista aplica em `AccountRow`: mostra as primeiras
 * "Preview Letters" e esconde o resto; sem letras de preview (ou quando o
 * preview mostraria o nome inteiro) o nome some por completo.
 */
export function maskAccountName(name: string, hidden: boolean, previewLetters: number): string {
  if (!hidden) return name;
  if (previewLetters > 0 && previewLetters < name.length) {
    return name.slice(0, previewLetters) + "********";
  }
  return "************";
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

/**
 * Vinculação de amizades em andamento (Make Friends), no Painel de Sessão.
 *
 * Mesmo desenho da fila de launch: o backend guarda o estado e emite
 * `friend-link-state` com o retrato **completo** a cada mudança; a tela só
 * substitui. Antes o progresso era `{phase, done, total}` guardado em `useState`
 * de dois componentes — a tela que remontava no meio perdia tudo, e na fase de
 * envio o `done` contava **pares**, não contas.
 */
export type FriendLinkAccountState = "pending" | "processing" | "done" | "failed";

export interface FriendLinkAccountEntry {
  userId: number;
  state: FriendLinkAccountState;
  /** Mensagem quando `state === "failed"`. */
  error: string | null;
}

export interface FriendLinkState {
  /** `true` enquanto a operação roda. */
  active: boolean;
  phase: "idle" | "checking" | "linking" | "verifying" | "done";
  /** Contas que já terminaram (concluídas ou com erro). */
  processed: number;
  total: number;
  accounts: FriendLinkAccountEntry[];
  mode: string;
  /** Conta principal no modo `star`. */
  mainUserId: number | null;
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
 * Problema com o `AccountData.key` (`vault_key_warning`).
 *
 * É a **única** rede contra o lockout de quem usa a chave do aparelho: sem o
 * arquivo de chave, um vault sem senha não abre e não existe senha para digitar.
 * Por isso vem estruturado — a frase mora no catálogo de i18n, não no backend —
 * e por isso aparece como faixa fixa, não como toast que passa.
 */
export interface VaultKeyWarning {
  /**
   * - `writeFailed`: o arquivo não pôde ser gravado. É o grave.
   * - `writeFailedTransient`: falhou agora (antivírus, indexador); a gravação
   *   seguinte tenta de novo. Tom brando de propósito: alarme falso treina o
   *   usuário a ignorar alarme.
   * - `weakWrapper`: gravou, mas sem o embrulho do DPAPI.
   */
  code:
    | "writeFailed"
    | "writeFailedTransient"
    | "weakWrapper"
    /**
     * A migração para o formato cifrado falhou e o `AccountData.json` **continua
     * em texto puro**, com o cookie de todas as contas legível. Era o pior
     * fail-open da tarefa: o app subia normal e a tela dizia "Device Key".
     */
    | "migrationFailed"
    /** Gravou, mas sem confirmação do `fsync`. */
    | "syncUnconfirmed";
  path: string;
  /** Detalhe do SO, para reportar. Nunca contém segredo. */
  detail?: string;
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

/**
 * Exceções de launch de **uma conta só**, por cima do perfil global.
 *
 * Ficam em `Account.Fields` (o mesmo lugar de `RobloxVersion`), e o backend as
 * lê em `account_client_overrides` (`commands/launch_shared.rs`) no instante em
 * que aquela conta vai abrir. Campo vazio quer dizer "herda o global" — não
 * "zero": um FPS vazio não é FPS 0.
 *
 * `volume` aqui é a escala que aparece na tela, 0 a 10, igual ao controle de
 * dentro do jogo; no `Fields` ele é gravado como fração de 0 a 1, que é o que o
 * `GlobalBasicSettings_13.xml` guarda.
 */
export interface AccountLaunchOverrides {
  enabled: boolean;
  maxFps: string;
  volume: string;
  /** `""` herda, `"auto"` é a qualidade automática, `"1"`–`"10"` é nível fixo. */
  graphics: string;
  /** `""` herda, `"true"` tela cheia, `"false"` em janela. */
  fullscreen: string;
  /** `""` herda, `"true"` minimiza ao abrir, `"false"` não minimiza. */
  startMinimized: string;
  windowWidth: string;
  windowHeight: string;
}

/** Chaves de `Account.Fields` usadas pelas exceções (as mesmas do Rust). */
export const ACCOUNT_OVERRIDE_FIELDS = {
  enabled: "ClientOverridesEnabled",
  maxFps: "ClientOverrideMaxFPS",
  volume: "ClientOverrideVolume",
  graphics: "ClientOverrideGraphics",
  fullscreen: "ClientOverrideFullscreen",
  startMinimized: "ClientOverrideStartMinimized",
  windowWidth: "ClientOverrideWindowWidth",
  windowHeight: "ClientOverrideWindowHeight",
} as const;

export const EMPTY_ACCOUNT_LAUNCH_OVERRIDES: AccountLaunchOverrides = {
  enabled: false,
  maxFps: "",
  volume: "",
  graphics: "",
  fullscreen: "",
  startMinimized: "",
  windowWidth: "",
  windowHeight: "",
};

function overrideBool(raw: string | undefined): string {
  const v = (raw ?? "").trim().toLowerCase();
  return v === "true" || v === "false" ? v : "";
}

export function readAccountLaunchOverrides(
  fields: Record<string, string> | undefined
): AccountLaunchOverrides {
  const f = fields ?? {};
  const volumeFraction = parseFloat((f[ACCOUNT_OVERRIDE_FIELDS.volume] ?? "").trim());
  return {
    enabled: overrideBool(f[ACCOUNT_OVERRIDE_FIELDS.enabled]) === "true",
    maxFps: (f[ACCOUNT_OVERRIDE_FIELDS.maxFps] ?? "").trim(),
    // Fração → escala da tela. `1` virando `10` é o esperado: o XML guarda o
    // volume cheio como 1.0.
    volume: Number.isFinite(volumeFraction)
      ? String(Math.round(volumeFraction * 100) / 10)
      : "",
    graphics: (f[ACCOUNT_OVERRIDE_FIELDS.graphics] ?? "").trim().toLowerCase(),
    fullscreen: overrideBool(f[ACCOUNT_OVERRIDE_FIELDS.fullscreen]),
    startMinimized: overrideBool(f[ACCOUNT_OVERRIDE_FIELDS.startMinimized]),
    windowWidth: (f[ACCOUNT_OVERRIDE_FIELDS.windowWidth] ?? "").trim(),
    windowHeight: (f[ACCOUNT_OVERRIDE_FIELDS.windowHeight] ?? "").trim(),
  };
}

/**
 * Devolve um `Fields` novo com as exceções gravadas. Campo vazio **apaga** a
 * chave em vez de gravar `""`, para o arquivo de contas não juntar entulho de
 * configuração que ninguém usa.
 */
export function writeAccountLaunchOverrides(
  fields: Record<string, string> | undefined,
  overrides: AccountLaunchOverrides
): Record<string, string> {
  const out = { ...(fields ?? {}) };

  const set = (key: string, value: string) => {
    if (value === "") delete out[key];
    else out[key] = value;
  };

  set(ACCOUNT_OVERRIDE_FIELDS.enabled, overrides.enabled ? "true" : "");
  set(ACCOUNT_OVERRIDE_FIELDS.maxFps, overrides.maxFps.trim());
  const volume = parseFloat(overrides.volume.trim());
  set(
    ACCOUNT_OVERRIDE_FIELDS.volume,
    Number.isFinite(volume) ? (Math.min(Math.max(volume, 0), 10) / 10).toFixed(3) : ""
  );
  set(ACCOUNT_OVERRIDE_FIELDS.graphics, overrides.graphics);
  set(ACCOUNT_OVERRIDE_FIELDS.fullscreen, overrides.fullscreen);
  set(ACCOUNT_OVERRIDE_FIELDS.startMinimized, overrides.startMinimized);
  set(ACCOUNT_OVERRIDE_FIELDS.windowWidth, overrides.windowWidth.trim());
  set(ACCOUNT_OVERRIDE_FIELDS.windowHeight, overrides.windowHeight.trim());

  return out;
}
