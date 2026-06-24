import { useTr } from "../../i18n/text";

export interface MenuItem {
  label: string;
  action?: () => void;
  separator?: boolean;
  submenu?: MenuItem[];
  devOnly?: boolean;
  className?: string;
}

export function MenuItemView({ item, close }: { item: MenuItem; close: () => void }) {
  const t = useTr();
  if (item.separator) {
    return <div className="my-1 border-t border-zinc-800/80" />;
  }

  if (item.submenu) {
    return (
      <div className="submenu-trigger relative">
        <div className="flex items-center justify-between px-3 py-1.5 text-[13px] text-zinc-300 hover:bg-zinc-800 cursor-default rounded-md mx-1">
          <span>{t(item.label)}</span>
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-zinc-600">
            <path d="m9 18 6-6-6-6" />
          </svg>
        </div>
        <div className="theme-modal-scope theme-panel theme-border submenu-panel hidden absolute left-full top-0 -mt-1 ml-0.5 min-w-[200px] bg-zinc-900/95 backdrop-blur-xl border border-zinc-700/50 rounded-xl shadow-2xl py-1 z-50 animate-fade-in">
          <div className="pl-1">
            {item.submenu.map((sub, i) => (
              <MenuItemView key={i} item={sub} close={close} />
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`flex items-center px-3 py-1.5 text-[13px] hover:bg-zinc-800 cursor-default rounded-md mx-1 ${item.className || "text-zinc-300"}`}
      onClick={() => {
        item.action?.();
        close();
      }}
    >
      {t(item.label)}
    </div>
  );
}
