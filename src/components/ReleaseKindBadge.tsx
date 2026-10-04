import { useTr } from "../i18n/text";
import type { ReleaseKind } from "../releaseNotes";

/**
 * Selo do tipo da release — correção, novidades ou atualização geral (regra em
 * .github/scripts/release-kind.mjs). Aparece ao lado da versão na janela de
 * atualização e na página "What's new". Cores calmas e distintas: âmbar para
 * correção, a cor de destaque do tema para novidades, violeta para geral.
 * Release sem tipo (as antigas) não tem selo.
 */
const TONES: Record<ReleaseKind, string> = {
  fix: "bg-amber-500/12 text-amber-300 ring-amber-400/25",
  feature: "bg-[var(--accent-soft)] text-[var(--accent-color)] ring-[var(--accent-color)]/25",
  mixed: "bg-violet-500/12 text-violet-300 ring-violet-400/25",
};

export function ReleaseKindBadge({ kind, className = "" }: { kind: ReleaseKind | null; className?: string }) {
  const t = useTr();
  if (!kind) return null;
  const label = kind === "fix" ? t("Fix") : kind === "feature" ? t("New features") : t("General update");
  return (
    <span
      data-release-kind={kind}
      className={`inline-flex items-center whitespace-nowrap rounded-full px-2 py-px text-[11px] font-medium leading-[18px] ring-1 ring-inset ${TONES[kind]} ${className}`}
    >
      {label}
    </span>
  );
}
