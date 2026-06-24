import { User } from "lucide-react";
import { useStore } from "../../store";
import { MultiSelectSidebar } from "./MultiSelectSidebar";
import { SingleSelectSidebar } from "./SingleSelectSidebar";
import { useTr } from "../../i18n/text";

export function DetailSidebar() {
  const t = useTr();
  const store = useStore();

  if (store.selectedAccounts.length > 1) {
    return <MultiSelectSidebar />;
  }

  if (!store.selectedAccount) {
    return (
      <div className="theme-surface theme-border w-72 border-l flex flex-col items-center justify-center shrink-0">
        <User size={32} strokeWidth={1} className="theme-muted mb-3" />
        <p className="theme-muted text-xs">{t("Select an account")}</p>
      </div>
    );
  }

  return <SingleSelectSidebar />;
}
