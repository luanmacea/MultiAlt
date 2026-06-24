import { useTr } from "../../i18n/text";

export function SectionLabel({ children }: { children: string }) {
  const t = useTr();
  return (
    <div className="text-[10px] uppercase tracking-widest text-zinc-600 font-medium px-1 pt-3 pb-1">
      {t(children)}
    </div>
  );
}
