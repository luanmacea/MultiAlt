// Tipo da release e o bump que ele decide (suíte `release`, release-kind.test.mjs).
//
// Toda release tem um tipo, que aparece na página da release, na janela de
// atualização e na página "What's new" do app:
// - `fix`     (Hotfix): só correções → soma patch;
// - `feature` (New features): só novidades → soma minor;
// - `mixed`   (General update): os dois, ou sem lista para classificar → minor.
//
// De onde sai o tipo, nesta ordem:
// 1. marca na mensagem do commit da release: `[type:fix]`, `[type:feature]`,
//    `[type:mixed]`;
// 2. rótulo no PR: `type:fix`, `type:feature`, `type:mixed`;
// 3. os itens da seção `## What's new` do PR: todos começando com "Fixed:" →
//    fix; nenhum → feature; os dois → mixed. Sem seção ou sem itens → mixed.
//
// O bump explícito continua valendo e vence o tipo: `bump` escolhido no
// workflow_dispatch, `[bump:minor]`/`[bump:patch]` na mensagem, rótulos
// `bump:minor`/`bump:patch` no PR.
//
// O texto da release carrega o tipo duas vezes, na primeira linha: uma marca
// para máquina (`<!-- release-kind: fix -->`, invisível na página do GitHub)
// e o selo visível. O app lê a marca e esconde as duas (src/releaseNotes.ts).

export const RELEASE_KINDS = ["fix", "feature", "mixed"];

export const RELEASE_KIND_BADGES = {
  fix: "**🩹 Hotfix**",
  feature: "**✨ New features**",
  mixed: "**📦 General update**",
};

const BULLET = /^\s*\\?[-*+]\s+(.*)$/;
const FIXED = /^\**\s*fixed\s*:/i;
const TYPE_MARKER = /\[type:(fix|feature|mixed)\]/i;

/** O que está sob `## What's new` no texto do PR, até o próximo título `## `. */
export function whatsNewSection(prBody) {
  const lines = String(prBody || "").split(/\r?\n/);
  const start = lines.findIndex((line) => /^##\s+What['’]s new\s*$/i.test(line));
  if (start < 0) return "";
  const end = lines.findIndex((line, i) => i > start && /^##\s+\S/.test(line));
  return lines
    .slice(start + 1, end < 0 ? undefined : end)
    .join("\n")
    .trim();
}

/** Classifica a lista em linguagem simples pelo prefixo "Fixed:" de cada item. */
export function classifyWhatsNew(section) {
  const items = String(section || "")
    .split(/\r?\n/)
    .map((line) => BULLET.exec(line)?.[1]?.trim())
    .filter(Boolean);
  if (items.length === 0) return "mixed";
  const fixes = items.filter((item) => FIXED.test(item)).length;
  if (fixes === items.length) return "fix";
  if (fixes === 0) return "feature";
  return "mixed";
}

/** Tipo forçado pela mensagem do commit (vence) ou pelo rótulo do PR; senão `null`. */
export function kindOverride({ message = "", labels = [] } = {}) {
  const fromMessage = TYPE_MARKER.exec(String(message || ""));
  if (fromMessage) return fromMessage[1].toLowerCase();
  const normalized = labels.map((label) => String(label).toLowerCase());
  for (const kind of RELEASE_KINDS) {
    if (normalized.includes(`type:${kind}`)) return kind;
  }
  return null;
}

export function decideReleaseKind({ prBody = "", message = "", labels = [] } = {}) {
  return kindOverride({ message, labels }) ?? classifyWhatsNew(whatsNewSection(prBody));
}

export function bumpForKind(kind) {
  return kind === "fix" ? "patch" : "minor";
}

/**
 * Tudo o que o passo "Resolve bump and channel" do release-v4.yml decide:
 * tipo, bump e canal. `message` é a mensagem do commit que disparou a
 * release; `labels` e `prBody`, do PR que levou o commit para a `main`.
 */
export function resolveReleaseControls({ message = "", labels = [], prBody = "", dispatchBump = "" } = {}) {
  const text = String(message || "").toLowerCase();
  const normalized = labels.map((label) => String(label).toLowerCase());
  const kind = decideReleaseKind({ prBody, message, labels });

  let bump = bumpForKind(kind);
  if (dispatchBump === "patch" || dispatchBump === "minor") bump = dispatchBump;
  if (text.includes("[bump:minor]")) bump = "minor";
  if (text.includes("[bump:patch]")) bump = "patch";
  if (normalized.includes("bump:minor")) bump = "minor";
  if (normalized.includes("bump:patch")) bump = "patch";

  let channel = "beta";
  if (text.includes("[channel:stable]")) channel = "stable";
  if (text.includes("[channel:beta]")) channel = "beta";
  if (normalized.includes("channel:stable")) channel = "stable";
  if (normalized.includes("channel:beta")) channel = "beta";

  return { kind, bump, channel };
}

/** Primeiras linhas do texto da release: a marca para máquina e o selo. */
export function releaseKindHeader(kind) {
  if (!RELEASE_KINDS.includes(kind)) {
    throw new Error(`Unsupported release kind: ${kind}`);
  }
  return `<!-- release-kind: ${kind} -->\n${RELEASE_KIND_BADGES[kind]}`;
}
