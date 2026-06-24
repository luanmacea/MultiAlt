import { useTr } from "../../i18n/text";

export function SidebarSection({ title, children }: { title: string; children: React.ReactNode }) {
  const t = useTr();
  return (
    <div className="flex flex-col gap-1.5">
      <div className="theme-label text-[10px] font-medium uppercase tracking-wider">{t(title)}</div>
      {children}
    </div>
  );
}
