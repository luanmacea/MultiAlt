import { useState, useEffect, useRef } from "react";
import { X } from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { Tooltip } from "../ui/Tooltip";
import { useTr } from "../../i18n/text";

interface FieldRow {
  id: number;
  key: string;
  value: string;
}

let nextId = 0;

export function AccountFieldsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const account = store.selectedAccount;
  const [rows, setRows] = useState<FieldRow[]>([]);
  const [flash, setFlash] = useState(false);
  const titleRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (!open || !account) return;
    const entries = Object.entries(account.Fields || {});
    setRows(entries.map(([key, value]) => ({ id: nextId++, key, value })));
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") handleClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, account?.UserID, handleClose]);

  if (!visible || !account) return null;

  function flashGreen() {
    setFlash(true);
    setTimeout(() => setFlash(false), 300);
  }

  async function saveFields(updatedRows: FieldRow[]) {
    const fields: Record<string, string> = {};
    for (const row of updatedRows) {
      if (row.key.trim()) fields[row.key.trim()] = row.value;
    }
    await store.updateAccount({ ...account!, Fields: fields });
    flashGreen();
  }

  function handleKeyChange(id: number, newKey: string) {
    setRows((prev) => prev.map((r) => (r.id === id ? { ...r, key: newKey } : r)));
  }

  function handleValueChange(id: number, newValue: string) {
    setRows((prev) => prev.map((r) => (r.id === id ? { ...r, value: newValue } : r)));
  }

  function handleValueKeyDown(e: React.KeyboardEvent, id: number) {
    if (e.key === "Enter") {
      e.preventDefault();
      const updated = rows.map((r) => (r.id === id ? { ...r } : r));
      void saveFields(updated);
    }
  }

  function addRow() {
    setRows((prev) => [...prev, { id: nextId++, key: t("Field"), value: t("Value") }]);
  }

  async function deleteRow(id: number) {
    const updated = rows.filter((r) => r.id !== id);
    setRows(updated);
    await saveFields(updated);
  }

  const title = t("Fields - {{name}}", { name: account.Alias || account.Username });

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      onClick={handleClose}
    >
      <div
        className={`theme-modal-scope theme-panel theme-border border rounded-2xl shadow-2xl w-[400px] max-h-[400px] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3">
          <h2
            ref={titleRef}
            className={`text-sm font-semibold transition-colors duration-150 ${
              flash ? "text-emerald-400" : "text-zinc-100"
            }`}
          >
            {title}
          </h2>
          <div className="flex items-center gap-2">
            <Tooltip content={t("Add field")} side="bottom">
              <button
                onClick={addRow}
                className="text-zinc-500 hover:text-zinc-300 transition-colors text-lg leading-none"
              >
                +
              </button>
            </Tooltip>
            <button onClick={handleClose} className="text-zinc-600 hover:text-zinc-400 transition-colors">
              <X size={16} strokeWidth={2} />
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-5 pb-4 space-y-1.5">
          {rows.length === 0 && (
            <p className="text-[11px] text-zinc-600 text-center py-6">{t("No fields. Click + to add one.")}</p>
          )}
          {rows.map((row) => (
            <div key={row.id} className="flex items-center gap-1.5">
              <input
                value={row.key}
                onChange={(e) => handleKeyChange(row.id, e.target.value)}
                className="flex-[45] min-w-0 px-2.5 py-1.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-xs text-zinc-300 focus:outline-none focus:border-zinc-600 transition-colors"
                spellCheck={false}
                placeholder={t("Key")}
              />
              <input
                value={row.value}
                onChange={(e) => handleValueChange(row.id, e.target.value)}
                onKeyDown={(e) => handleValueKeyDown(e, row.id)}
                className="flex-[45] min-w-0 px-2.5 py-1.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-xs text-zinc-300 focus:outline-none focus:border-zinc-600 transition-colors"
                spellCheck={false}
                placeholder={t("Value")}
              />
              <Tooltip content={t("Remove field")}>
                <button
                  onClick={() => void deleteRow(row.id)}
                  className="shrink-0 w-6 h-6 flex items-center justify-center text-red-500/60 hover:text-red-400 transition-colors rounded"
                >
                  <X size={12} strokeWidth={2.5} />
                </button>
              </Tooltip>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
