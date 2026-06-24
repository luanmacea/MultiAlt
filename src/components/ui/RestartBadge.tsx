import { useTr } from "../../i18n/text";

export function RestartBadge() {
  const t = useTr();
  return (
    <span className="inline-flex items-center gap-1 ml-2 px-1.5 py-0.5 rounded bg-sky-500/10 border border-sky-500/20 text-[10px] text-sky-400/70">
      {t("restart required")}
    </span>
  );
}
