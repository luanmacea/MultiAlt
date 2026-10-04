import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Archive, FolderOpen, Plus, RotateCcw, Trash2, X } from "lucide-react";
import { useConfirm, usePrompt } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { timeAgo, type BackupEntry, type BackupsInfo, type RestoreReport } from "../../types";
import { SectionLabel } from "../ui/SectionLabel";
import { hydrateGameLists, pauseGameListsMirror } from "../server-list/gameListsSync";

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 KB";
  const kb = bytes / 1024;
  if (kb < 1024) return `${Math.max(1, Math.round(kb))} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

/** "3h ago" / "just now" — `timeAgo` devolve só o intervalo cru. */
function relativeLabel(iso: string, t: ReturnType<typeof useTr>): string {
  const ago = timeAgo(iso);
  if (ago === "never") return t("unknown date");
  if (ago === "now" || ago === "just now") return t("just now");
  return t("{{ago}} ago", { ago });
}

/** Data absoluta para o `title`; devolve o valor cru quando o backend mandar lixo. */
function absoluteDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString();
}

/**
 * Seção **Backups** da página Settings. Até 03/10/2026 era um diálogo
 * (`BackupsDialog`) aberto por um botão "Manage" em Misc > Data — escondido
 * demais para algo tão importante. O conteúdo é o mesmo (criar, listar,
 * restaurar, apagar; restaurar e apagar pedem confirmação), sem a moldura de
 * modal.
 *
 * A página monta todas as seções de uma vez e esconde as outras com
 * `display: none`; por isso `active`: a pasta de backups só é lida quando a
 * seção aparece, e de novo a cada vez que ela volta a aparecer.
 */
export function BackupsTab({ active }: { active: boolean }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const prompt = usePrompt();

  const [entries, setEntries] = useState<BackupEntry[]>([]);
  const [info, setInfo] = useState<BackupsInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [creating, setCreating] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<RestoreReport | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [list, meta] = await Promise.all([
        invoke<BackupEntry[]>("list_backups"),
        invoke<BackupsInfo>("backups_info"),
      ]);
      // O backend já devolve do mais recente para o mais antigo; reordenamos
      // por data para a lista não depender disso.
      const sorted = [...(list ?? [])].sort(
        (a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt)
      );
      setEntries(sorted);
      setInfo(meta ?? null);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    setReport(null);
    setError(null);
    void refresh();
  }, [active, refresh]);

  useEffect(() => {
    if (!active) return;
    const unlisten = listen("backup-restored", () => {
      void refresh();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [active, refresh]);

  async function handleCreate() {
    // O rótulo é opcional: string vazia cria sem rótulo, cancelar aborta.
    const label = await prompt(t("Name this backup (optional)"), "");
    if (label === null) return;
    setCreating(true);
    try {
      await invoke<BackupEntry>("create_backup", { label: label.trim() || null });
      setError(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  }

  async function handleRestore(entry: BackupEntry) {
    const ok = await confirm(
      t(
        'Restore "{{label}}"? Your current accounts, settings, scripts and themes will be replaced. A safety backup of the current data is created first.',
        { label: entryTitle(entry) }
      ),
      true
    );
    if (!ok) return;
    setBusyId(entry.id);
    // O espelho dos favoritos para durante a restauração: uma gravação que
    // chegasse depois dela poria a lista de antes por cima do RAMGameLists.json
    // restaurado. Volta na hidratação do fim, que junta o restaurado com o local.
    await pauseGameListsMirror();
    try {
      const result = await invoke<RestoreReport>("restore_backup", { id: entry.id });
      setReport(result ?? null);
      setError(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      void hydrateGameLists();
      setBusyId(null);
    }
  }

  async function handleDelete(entry: BackupEntry) {
    const ok = await confirm(
      t('Delete backup "{{label}}"? This cannot be undone.', { label: entryTitle(entry) }),
      true
    );
    if (!ok) return;
    setBusyId(entry.id);
    try {
      await invoke<boolean>("delete_backup", { id: entry.id });
      setError(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  }

  async function handleOpenFolder() {
    try {
      await invoke("open_backups_folder");
    } catch (e) {
      store.addToast(String(e));
    }
  }

  function entryTitle(entry: BackupEntry): string {
    return entry.label || entry.fileName;
  }

  // O relatório traz só o id do backup; o nome do arquivo vem da lista.
  const restoredName = report
    ? entries.find((e) => e.id === report.backupId)?.fileName ?? report.backupId
    : "";

  return (
    <div className="space-y-3">
      <p className="text-[12.5px] text-zinc-500 px-1">
        {t("A backup keeps a copy of your accounts, settings, scripts and themes.")}{" "}
        {t("Restoring replaces the files in your data folder.")}
      </p>

      {/* Cartão de cima: onde os dados moram e as duas ações da seção. */}
      <div className="rounded-xl border border-zinc-800/70 bg-zinc-900/35 px-4 py-3.5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <div className="text-[11px] uppercase tracking-widest text-zinc-600 font-medium">
              {t("Data folder")}
            </div>
            <div className="mt-1 text-[12px] font-mono text-zinc-300 break-all" title={info?.dir ?? ""}>
              {info?.dir || "—"}
            </div>
            {info && (
              <div className="mt-1 text-[12px] text-zinc-500">
                {t("{{n}} backups", { n: info.count })} · {formatBytes(info.totalBytes)}
              </div>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={() => void handleOpenFolder()}
              className="flex items-center gap-1.5 rounded-lg border border-zinc-700/70 bg-zinc-800 px-3 py-1.5 text-[12px] font-medium text-zinc-200 transition-colors hover:bg-zinc-700"
              title={t("Open backups folder")}
            >
              <FolderOpen size={13} strokeWidth={1.6} />
              {t("Open folder")}
            </button>
            <button
              type="button"
              onClick={() => void handleCreate()}
              disabled={creating}
              className="flex items-center gap-1.5 rounded-lg border border-sky-700/50 bg-sky-900/30 px-3 py-1.5 text-[12px] font-medium text-sky-200 transition-colors hover:bg-sky-900/50 disabled:opacity-60"
            >
              <Plus size={13} strokeWidth={2} />
              {creating ? t("Creating...") : t("Create backup")}
            </button>
          </div>
        </div>

        {info?.portable && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-2 text-[12px] text-amber-300">
            <AlertTriangle size={13} strokeWidth={1.75} className="mt-px shrink-0" />
            <span>
              {t(
                "Portable mode: your data lives next to the executable. Moving or updating the app can leave it behind — keep a backup before you move it."
              )}
            </span>
          </div>
        )}

        {/*
          O zip leva o `AccountData.key` — sem ele o backup não restaura —,
          então sem senha quem tem o zip tem a chave. A decisão foi manter a
          chave no zip **com este aviso**, e ele tem que estar onde o backup é
          criado: a tela de criptografia, onde também está, só abre sozinha
          para quem ainda não tem contas. Com senha o `.key` não existe e o
          zip sai protegido por ela. `null` (não deu para saber) avisa.
        */}
        {store.accountsEncrypted !== true && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 py-2 text-[12px] text-amber-300">
            <AlertTriangle size={13} strokeWidth={1.75} className="mt-px shrink-0" />
            <span>
              {t(
                "Without a password, the backup zip carries the key that opens your accounts — it has to, or it could never be restored — so a leaked zip is not protected. If you keep backups in cloud storage like OneDrive or Google Drive, set a password in Settings > Misc > Change Encryption Method."
              )}
            </span>
          </div>
        )}
      </div>

      {error && (
        <div
          role="alert"
          className="rounded-md border border-rose-500/30 bg-rose-500/10 px-2.5 py-2 text-[12px] text-rose-300"
        >
          {error}
        </div>
      )}

      {report && (
        <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-4 py-3 text-[12px] text-emerald-200 space-y-1">
          <div className="flex items-center justify-between gap-3">
            <span className="text-[12.5px] font-medium">{t("Backup restored")}</span>
            <button
              type="button"
              onClick={() => setReport(null)}
              className="text-emerald-300/70 hover:text-emerald-200"
              title={t("Dismiss")}
            >
              <X size={13} strokeWidth={2} />
            </button>
          </div>
          <div className="font-mono break-all">{t("Restored from {{file}}", { file: restoredName })}</div>
          {report.safetyBackupId ? (
            <div className="font-mono break-all">
              {t("Safety backup of your previous data: {{id}}", { id: report.safetyBackupId })}
            </div>
          ) : (
            <div className="text-amber-300">
              {t("No safety backup could be created for the replaced data.")}
            </div>
          )}
          {report.restored.length > 0 && (
            <div className="text-emerald-300/80">
              {t("Replaced: {{files}}", { files: report.restored.join(", ") })}
            </div>
          )}
          {report.skipped.length > 0 && (
            <div className="text-amber-300">
              {t("Skipped: {{files}}", { files: report.skipped.join(", ") })}
            </div>
          )}
          {report.requiresRestart ? (
            <div className="mt-1.5 flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/15 px-2.5 py-2 text-amber-200">
              <AlertTriangle size={13} strokeWidth={1.75} className="mt-px shrink-0" />
              <span>
                <strong className="font-semibold">{t("Restart required")}</strong>
                {" — "}
                {t("Close and reopen the app so the restored data is loaded.")}
                {report.restartReasons.length > 0 ? ` (${report.restartReasons.join("; ")})` : ""}
              </span>
            </div>
          ) : (
            report.accountsReloaded && (
              <div className="text-emerald-300/80">
                {t("Your accounts were reloaded, no restart needed.")}
              </div>
            )
          )}
        </div>
      )}

      <SectionLabel>Saved backups</SectionLabel>

      {loading && entries.length === 0 && !error && (
        <div className="text-[12px] text-zinc-500 py-8 text-center">{t("Loading backups...")}</div>
      )}

      {!loading && entries.length === 0 && (
        <div className="rounded-xl border border-dashed border-zinc-800/80 text-[12px] text-zinc-500 py-8 px-4 text-center">
          {t("No backups yet. Create one before moving the app or editing your accounts.")}
        </div>
      )}

      {entries.length > 0 && (
        <ul className="rounded-xl border border-zinc-800/70 divide-y divide-zinc-800/70 overflow-hidden">
          {entries.map((entry) => {
            const busy = busyId === entry.id;
            return (
              <li
                key={entry.id}
                data-testid={`backup-${entry.id}`}
                className="flex items-center justify-between gap-3 bg-zinc-900/35 px-4 py-2.5"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <Archive size={13} strokeWidth={1.5} className="text-zinc-500 shrink-0" />
                    <span className="text-[13px] text-zinc-200 truncate">{entryTitle(entry)}</span>
                    {entry.automatic && (
                      <span className="px-1.5 py-0.5 rounded bg-zinc-700/50 text-[11px] text-zinc-300 shrink-0">
                        {t("Automatic")}
                      </span>
                    )}
                    {!entry.valid && (
                      <span className="px-1.5 py-0.5 rounded bg-rose-500/15 text-[11px] text-rose-300 shrink-0">
                        {t("Invalid")}
                      </span>
                    )}
                  </div>
                  <div
                    className="mt-0.5 flex items-center gap-2 text-[12px] text-zinc-500 truncate"
                    title={absoluteDate(entry.createdAt)}
                  >
                    <span>{relativeLabel(entry.createdAt, t)}</span>
                    <span>•</span>
                    <span>{formatBytes(entry.sizeBytes)}</span>
                    <span>•</span>
                    <span title={entry.files.join(", ")}>{t("{{n}} files", { n: entry.files.length })}</span>
                  </div>
                  <div className="mt-0.5 text-[11px] font-mono text-zinc-600 truncate">{entry.fileName}</div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <button
                    type="button"
                    onClick={() => void handleRestore(entry)}
                    disabled={!entry.valid || busy}
                    title={
                      entry.valid
                        ? t("Restore this backup")
                        : t("This backup is damaged and cannot be restored")
                    }
                    className="flex items-center gap-1.5 rounded-lg border border-zinc-700/70 bg-zinc-800 px-2.5 py-1 text-[12px] font-medium text-zinc-200 transition-colors hover:bg-zinc-700 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <RotateCcw size={12} strokeWidth={1.75} />
                    {t("Restore")}
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleDelete(entry)}
                    disabled={busy}
                    title={t("Delete this backup")}
                    aria-label={t("Delete this backup")}
                    className="p-1.5 rounded text-rose-400 hover:bg-rose-900/40 transition-colors disabled:opacity-50"
                  >
                    <Trash2 size={14} strokeWidth={1.5} />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
