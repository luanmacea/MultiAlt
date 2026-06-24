import { invoke } from "@tauri-apps/api/core";
import { File, FileText, Globe, Plus, X } from "lucide-react";
import { useStore } from "../../store";
import { usePrompt } from "../../hooks/usePrompt";
import { tr, useTr } from "../../i18n/text";

interface AddAccountDialogProps {
  open: boolean;
  onClose: () => void;
}

export function AddAccountDialog({ open, onClose }: AddAccountDialogProps) {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();

  if (!open) return null;

  async function handleQuickAdd() {
    onClose();
    const input = await prompt(tr("Cookie or username"));
    if (!input?.trim()) return;
    const value = input.trim();

    try {
      if (value.includes("_|WARNING:-DO-NOT-SHARE")) {
        await store.addAccountByCookie(value);
        return;
      }

      const user = await invoke<{ id: number; name: string }>("lookup_user", { username: value });
      await invoke("add_account", {
        securityToken: "",
        username: user.name,
        userId: user.id,
      });
      await store.loadAccounts();
      store.addToast(tr("Added {{name}}", { name: user.name }));
    } catch (e) {
      store.addToast(tr("Add failed: {{error}}", { error: String(e) }));
    }
  }

  async function handleBrowserLogin() {
    onClose();
    await store.openLoginBrowser();
  }

  function handleImportCookie() {
    onClose();
    store.setImportDialogTab("cookie");
    store.setImportDialogOpen(true);
  }

  function handleImportOldAccountData() {
    onClose();
    store.setImportDialogTab("legacy");
    store.setImportDialogOpen(true);
  }

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
      onClick={onClose}
    >
      <div
        className="theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[420px] max-w-[calc(100vw-2rem)] overflow-hidden animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-800/70">
          <h2 className="text-sm font-semibold text-zinc-100">{t("Add Account")}</h2>
          <button
            onClick={onClose}
            className="theme-muted hover:opacity-100 transition-opacity"
            aria-label={t("Close")}
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="px-4 py-3">
          <p className="text-xs text-zinc-400 mb-3">{t("Choose how to add an account")}</p>

          <div className="space-y-1.5">
            <button
              onClick={handleQuickAdd}
              className="flex items-center gap-2.5 w-full px-3 py-2.5 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left rounded-lg transition-colors"
            >
              <Plus size={15} strokeWidth={1.75} className="theme-muted" />
              {t("Quick Add")}
            </button>

            <button
              onClick={handleBrowserLogin}
              className="flex items-center gap-2.5 w-full px-3 py-2.5 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left rounded-lg transition-colors"
            >
              <Globe size={15} strokeWidth={1.75} className="theme-muted" />
              {t("Browser Login")}
            </button>

            <button
              onClick={handleImportCookie}
              className="flex items-center gap-2.5 w-full px-3 py-2.5 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left rounded-lg transition-colors"
            >
              <File size={15} strokeWidth={1.75} className="theme-muted" />
              {t("Import Cookie")}
            </button>

            <button
              onClick={handleImportOldAccountData}
              className="flex items-center gap-2.5 w-full px-3 py-2.5 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left rounded-lg transition-colors"
            >
              <FileText size={15} strokeWidth={1.75} className="theme-muted" />
              {t("Import Old Account Data")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
