import { useStore } from "../../store";
import { SingleSelectSidebar } from "./SingleSelectSidebar";

/**
 * Account Settings sidebar — shown only for single-account selection.
 * Multi-account actions (launch, batch ops) live in BottomActionBar + ChooseGameScreen.
 */
export function DetailSidebar() {
  const store = useStore();

  // Sidebar only makes sense when editing a single account's settings.
  // Multi-select is handled by BottomActionBar.
  if (!store.selectedAccount || store.selectedAccounts.length > 1) return null;

  return <SingleSelectSidebar />;
}
