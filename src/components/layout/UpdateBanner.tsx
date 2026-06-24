import { useStore } from "../../store";
import { useTr } from "../../i18n/text";

export function UpdateBanner() {
  const t = useTr();
  const store = useStore();

  if (!store.updateInfo) return null;

  return (
    <div className="flex items-center justify-between px-4 py-1.5 bg-sky-600/15 border-b border-sky-500/20 shrink-0">
      <span className="text-xs text-sky-300">
        {t("v{{version}} available", { version: store.updateInfo.version })}
      </span>
      <button
        onClick={() => store.setUpdateDialogOpen(true)}
        className="text-xs font-medium text-sky-400 hover:text-sky-300 transition-colors"
      >
        {t("View Update")}
      </button>
    </div>
  );
}
