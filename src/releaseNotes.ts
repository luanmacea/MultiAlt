import { REPO_API_URL } from "./repo";

/**
 * Notas de release: o texto que a página da release no GitHub mostra, ajustado
 * para quem lê dentro do app. Dividido entre a janela de atualização
 * ([UpdateDialog](./components/dialogs/UpdateDialog.tsx)) e a página de
 * novidades ([ChangelogPage](./components/pages/ChangelogPage.tsx)).
 *
 * O `fetch` em `api.github.com` daqui é a mesma exceção à regra "frontend não
 * acessa rede" que a janela de atualização já tinha (docs/architecture.md).
 */

/** Tira os blocos recolhidos (`<details>…</details>`) de uma lista de linhas. */
function withoutDetails(lines: string[]): string[] {
  const kept: string[] = [];
  let inDetails = false;
  for (const line of lines) {
    if (/^\s*<details>/i.test(line)) inDetails = true;
    if (!inDetails) kept.push(line);
    if (/<\/details>\s*$/i.test(line)) inDetails = false;
  }
  return kept;
}

/**
 * Tipo da release: correção (`fix`), novidades (`feature`) ou atualização
 * geral (`mixed`). Quem decide é o workflow de release
 * (.github/scripts/release-kind.mjs), que escreve na primeira linha do texto
 * uma marca para máquina e, logo abaixo, o selo visível na página do GitHub.
 */
export type ReleaseKind = "fix" | "feature" | "mixed";

const KIND_MARKER = /<!--\s*release-kind:\s*(fix|feature|mixed)\s*-->/i;

/**
 * Linhas do tipo que o app não mostra como texto: a marca (de qualquer valor)
 * e os três selos, que no app viram o selo da própria tela. Os textos dos selos
 * são os de `RELEASE_KIND_BADGES` (release-kind.mjs) — o release-kind.test.mjs
 * confere que os dois lados batem.
 */
const KIND_LINE =
  /^\s*(?:<!--\s*release-kind:[^>]*-->|\*\*\s*(?:🩹 Hotfix|✨ New features|📦 General update)\s*\*\*)\s*$/u;

/** Item de lista em linguagem simples que é correção: "Fixed: …". */
const FIXED_ITEM = /^\**\s*fixed\s*:/i;

/**
 * A página da release abre com "## Download" (o botão do instalador para quem
 * chega pelo GitHub — ver o passo "Finalize release notes" do release-v4.yml)
 * e guarda a lista técnica de PRs num bloco recolhido (<details>). Na janela de
 * atualização nenhum dos dois serve: o app já baixa e instala sozinho, e quem
 * atualiza quer a lista simples. Tira a seção de download inteira (até o
 * próximo título "## ") e os blocos <details>. A marca e o selo do tipo da
 * release também saem: a janela mostra o tipo no próprio selo.
 */
export function notesForUpdateDialog(body: string): string {
  const all = body.split(/\r?\n/);
  const lines = all.filter((l) => !KIND_LINE.test(l));
  const start = lines.findIndex((l) => /^##\s+Download\b/i.test(l));
  const withoutDownload =
    start < 0
      ? lines
      : (() => {
          const next = lines.findIndex((l, i) => i > start && /^##\s+\S/.test(l));
          return [...lines.slice(0, start), ...(next < 0 ? [] : lines.slice(next))];
        })();
  const kept = withoutDetails(withoutDownload);
  const out = kept.join("\n").replace(/\n{3,}/g, "\n\n").trim();
  return start < 0 && kept.length === all.length ? body : out;
}

/** Item de lista em markdown (`-`, `*`, `+`, inclusive o `\*` escapado). */
const BULLET = /^\s*\\?[-*+]\s+(.*)$/;

/**
 * " by @autor in https://github.com/.../pull/12" ou " by @autor in #12": o
 * rodapé que a lista automática do GitHub põe em cada título de PR.
 */
const PR_SUFFIX = /\s+by\s+@[A-Za-z0-9-]+(?:\[bot\])?\s+in\s+(?:https?:\/\/\S+|#\d+)\s*$/i;

/**
 * O que a página de novidades mostra de uma versão: só a seção
 * "## What's Changed", em markdown, ou `""` quando não sobra nada.
 *
 * - Da v0.1.10 em diante a seção já é a lista em linguagem simples; a lista
 *   técnica fica num `<details>`, que sai.
 * - Até a v0.1.9 a seção era a lista automática de títulos de PR. Sai o autor e
 *   o número do PR, a linha "Full Changelog" e os itens com `[skip release]` —
 *   PR que não publica versão (README, site; ver docs/development.md), ou seja,
 *   não mudou nada no app de quem lê.
 * - Release sem a seção (v0.1.1, v0.1.2) volta vazia e a página a pula.
 */
export function changelogNotes(body: string): string {
  const section = whatsChangedLines(body);
  if (!section) return "";

  const out: string[] = [];
  for (const raw of section) {
    const line = raw.trimEnd();
    if (/^\s*\**\s*Full Changelog\b/i.test(line)) continue;
    const bullet = line.match(BULLET);
    if (bullet) {
      if (/\[skip release\]/i.test(bullet[1])) continue;
      const text = bullet[1].replace(PR_SUFFIX, "").trim();
      if (!text || /^Release notes could not be generated\.?$/i.test(text)) continue;
      out.push(`- ${text}`);
      continue;
    }
    out.push(line);
  }
  return out.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/**
 * Linhas da seção "## What's Changed" (até o próximo título), sem os blocos
 * recolhidos e sem as linhas do tipo; `null` quando a release não tem a seção.
 */
function whatsChangedLines(body: string): string[] | null {
  const lines = body.split(/\r?\n/);
  const start = lines.findIndex((l) => /^##\s+What['’]s Changed\b/i.test(l));
  if (start < 0) return null;
  const end = lines.findIndex((l, i) => i > start && /^##\s+\S/.test(l));
  return withoutDetails(lines.slice(start + 1, end < 0 ? undefined : end)).filter((l) => !KIND_LINE.test(l));
}

/**
 * Tipo de uma release pelo texto dela, ou `null` (sem selo).
 *
 * - Com a marca `<!-- release-kind: … -->` (releases depois de 04/10/2026),
 *   vale a marca.
 * - Sem marca, a lista em linguagem simples de "## What's Changed" é
 *   classificada como o workflow faz: todos os itens com "Fixed:" → correção;
 *   nenhum → novidades; os dois → atualização geral.
 * - Release antiga, com a lista técnica de títulos de PR (" by @autor in …"),
 *   ou sem lista nenhuma, fica sem tipo: chutar ali seria inventar.
 */
export function releaseKindOf(body: string): ReleaseKind | null {
  const marker = KIND_MARKER.exec(body);
  if (marker) return marker[1].toLowerCase() as ReleaseKind;

  const items = (whatsChangedLines(body) ?? [])
    .map((line) => line.match(BULLET)?.[1]?.trim() ?? "")
    .filter(Boolean);
  if (items.length === 0 || items.some((item) => PR_SUFFIX.test(item))) return null;
  const fixes = items.filter((item) => FIXED_ITEM.test(item)).length;
  if (fixes === items.length) return "fix";
  return fixes === 0 ? "feature" : "mixed";
}

/** "v0.1.10-beta" → "0.1.10". O canal (`-beta`) não diz nada para quem lê. */
export function versionFromTag(tag: string): string {
  return tag.trim().replace(/^v/i, "").replace(/[-+].*$/, "");
}

/** Compara número a número ("0.1.10" > "0.1.9"); aceita tag com `v` e canal. */
export function compareVersions(a: string, b: string): number {
  const pa = versionFromTag(a).split(".").map((n) => Number.parseInt(n, 10) || 0);
  const pb = versionFromTag(b).split(".").map((n) => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

export interface ReleaseEntry {
  tag: string;
  /** Versão sem `v` e sem canal: "0.1.10". */
  version: string;
  /** ISO 8601 de quando a release saiu. */
  publishedAt: string;
  /** Markdown de `changelogNotes`, nunca vazio. */
  notes: string;
  /** Correção, novidades ou geral; `null` em release antiga (sem selo). */
  kind: ReleaseKind | null;
  url: string;
}

export type ReleaseHistoryErrorKind = "rate-limited" | "unavailable";

export class ReleaseHistoryError extends Error {
  readonly kind: ReleaseHistoryErrorKind;
  constructor(kind: ReleaseHistoryErrorKind, message: string) {
    super(message);
    this.name = "ReleaseHistoryError";
    this.kind = kind;
  }
}

interface GithubRelease {
  tag_name?: unknown;
  published_at?: unknown;
  created_at?: unknown;
  html_url?: unknown;
  draft?: unknown;
  body?: unknown;
}

function toEntry(raw: GithubRelease): ReleaseEntry | null {
  if (raw.draft === true || typeof raw.tag_name !== "string" || !raw.tag_name.trim()) return null;
  const body = typeof raw.body === "string" ? raw.body : "";
  const notes = changelogNotes(body);
  if (!notes) return null;
  const publishedAt =
    typeof raw.published_at === "string" ? raw.published_at : typeof raw.created_at === "string" ? raw.created_at : "";
  return {
    tag: raw.tag_name,
    version: versionFromTag(raw.tag_name),
    publishedAt,
    notes,
    kind: releaseKindOf(body),
    url: typeof raw.html_url === "string" ? raw.html_url : "",
  };
}

/**
 * Quantas releases pedir. Uma página só: 30 versões cobrem bem mais do que
 * alguém rola, e cada página a mais gastaria outro dos 60 pedidos por hora que
 * o GitHub dá sem login. O resto fica no link "ver todas no GitHub".
 */
const RELEASES_PER_PAGE = 30;

/**
 * Lista da sessão. Guarda a promessa (duas aberturas ao mesmo tempo dividem o
 * pedido) e só a de sucesso: depois de uma falha, abrir a página de novo ou
 * clicar em "Tentar de novo" pede outra vez.
 */
let cached: Promise<ReleaseEntry[]> | null = null;

async function loadReleaseHistory(): Promise<ReleaseEntry[]> {
  let response: Response;
  try {
    response = await fetch(`${REPO_API_URL}/releases?per_page=${RELEASES_PER_PAGE}`, {
      headers: { Accept: "application/vnd.github+json" },
    });
  } catch (error) {
    throw new ReleaseHistoryError("unavailable", String(error));
  }

  if (!response.ok) {
    const remaining = response.headers?.get("x-ratelimit-remaining");
    const limited = response.status === 429 || (response.status === 403 && remaining === "0");
    throw new ReleaseHistoryError(limited ? "rate-limited" : "unavailable", `GitHub answered ${response.status}`);
  }

  let data: unknown;
  try {
    data = await response.json();
  } catch (error) {
    throw new ReleaseHistoryError("unavailable", String(error));
  }
  if (!Array.isArray(data)) throw new ReleaseHistoryError("unavailable", "Unexpected answer from GitHub");

  return data
    .map((raw) => toEntry((raw ?? {}) as GithubRelease))
    .filter((entry): entry is ReleaseEntry => entry !== null)
    .sort((a, b) => Date.parse(b.publishedAt || "0") - Date.parse(a.publishedAt || "0"));
}

/** A lista já carregada, para a página abrir pronta, sem piscar o "carregando". */
let settled: ReleaseEntry[] | null = null;

/** As versões publicadas, mais nova primeiro. Uma vez por sessão. */
export function fetchReleaseHistory(): Promise<ReleaseEntry[]> {
  if (!cached) {
    const pending = loadReleaseHistory();
    cached = pending;
    pending.then(
      (entries) => {
        if (cached === pending) settled = entries;
      },
      () => {
        if (cached === pending) cached = null;
      }
    );
  }
  return cached;
}

/** Lista já carregada nesta sessão, ou `null` — sem pedir nada. */
export function cachedReleaseHistory(): ReleaseEntry[] | null {
  return settled;
}

/** Só para teste: esquece a lista da sessão. */
export function resetReleaseHistoryCache(): void {
  cached = null;
  settled = null;
}
