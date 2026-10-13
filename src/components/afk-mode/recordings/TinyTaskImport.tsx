import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileUp } from "lucide-react";
import { useTr } from "../../../i18n/text";
import { Select } from "../../ui/Select";
import {
  MAX_RECORDING_NAME_CHARS,
  recordingNameFromFile,
  type TinyTaskArea,
  type TinyTaskImport,
  type TinyTaskSummary,
} from "../../../recordings";

/** Uma conta com cliente aberto pelo app, para escolher a janela de referência. */
export interface OpenWindowChoice {
  userId: number;
  name: string;
}

/** O rascunho que a importação entrega ao editor. */
export interface ImportedDraft extends TinyTaskImport {
  name: string;
}

type Translate = ReturnType<typeof useTr>;

/** A frase de um erro da importação (códigos do backend). */
export function tinyTaskErrorText(t: Translate, code: string): string {
  switch (code) {
    case "empty":
      return t("This file is empty.");
    case "tooLarge":
      return t("This file is too large to be a TinyTask recording.");
    case "notTinyTask":
      return t("This file is not a TinyTask recording (.rec).");
    case "noWindow":
      return t("That account's Roblox window is not open anymore. Open it and try again.");
    default:
      return code;
  }
}

/** As linhas do resumo: o que ficou e o que ficou de fora. */
export function tinyTaskSummaryLines(t: Translate, steps: number, s: TinyTaskSummary): string[] {
  const lines = [t("Imported {{steps}} steps from {{events}} TinyTask events.", { steps, events: s.events })];
  if (s.skippedKeys.length > 0) {
    lines.push(
      t("Keys left out (not in the recordings' key list): {{keys}}.", {
        keys: s.skippedKeys.map((k) => (k.count > 1 ? `${k.key} ×${k.count}` : k.key)).join(", "),
      })
    );
  }
  if (s.clicksOutside > 0) {
    lines.push(t("Clicks outside the chosen window, left out: {{count}}.", { count: s.clicksOutside }));
  }
  if (s.otherMouse > 0) {
    lines.push(t("Right clicks, middle clicks and scrolling, left out: {{count}}.", { count: s.otherMouse }));
  }
  if (s.drags > 0) {
    lines.push(t("Drags turned into a click where the button went down: {{count}}.", { count: s.drags }));
  }
  if (s.cappedWaits > 0) {
    lines.push(t("Waits longer than 60 s shortened to 60 s: {{count}}.", { count: s.cappedWaits }));
  }
  if (s.stopKey) {
    lines.push(t("The key that stopped the TinyTask recording ({{key}}) was left out.", { key: s.stopKey }));
  }
  if (s.truncated) {
    lines.push(t("The file had more steps than a recording holds; the rest was left out."));
  }
  return lines;
}

/**
 * "Import from TinyTask (.rec)": o arquivo (por um botão de verdade), o nome
 * (sugerido pelo arquivo) e a janela em que ele foi gravado — essa só aparece
 * com mais de uma conta aberta. A janela é a de uma conta com cliente aberto
 * pelo app: o backend lê a área interna dela (`recording_window_area`) e
 * converte os cliques em porcentagem dessa área. O Import entrega o resultado
 * já com o nome; quem salva é a aba (`onImported`), e um erro dela volta aqui.
 */
export function TinyTaskImportPanel({
  windows,
  onImported,
  onCancel,
}: {
  windows: OpenWindowChoice[];
  onImported: (draft: ImportedDraft) => Promise<void>;
  onCancel: () => void;
}) {
  const t = useTr();
  const fileRef = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState("");
  const [userId, setUserId] = useState<string>(() => (windows[0] ? String(windows[0].userId) : ""));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chosen = windows.some((w) => String(w.userId) === userId) ? userId : windows[0] ? String(windows[0].userId) : "";

  function pickFile(next: File | null) {
    setFile(next);
    setError(null);
    if (next) setName(recordingNameFromFile(next.name) || t("From TinyTask"));
  }

  async function importFile() {
    if (!file || !chosen || busy) return;
    setBusy(true);
    setError(null);
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const area = await invoke<TinyTaskArea>("recording_window_area", { userId: Number(chosen) });
      const out = await invoke<TinyTaskImport>("import_tinytask_recording", { bytes, area });
      await onImported({ ...out, name: name.trim() || recordingNameFromFile(file.name) || t("From TinyTask") });
    } catch (e) {
      setError(tinyTaskErrorText(t, String(e)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rounded-lg border theme-border bg-[var(--panel-soft)] p-3 space-y-2.5" data-testid="tinytask-import">
      {windows.length === 0 ? (
        <div className="text-[11.5px] text-amber-300/90 leading-4" role="status">
          {t("Open the account whose Roblox window you recorded in first: the clicks are placed relative to that window.")}
        </div>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <input
              ref={fileRef}
              type="file"
              accept=".rec"
              aria-label={t("TinyTask file")}
              onChange={(e) => pickFile(e.target.files?.[0] ?? null)}
              className="sr-only"
              tabIndex={-1}
            />
            <button type="button" onClick={() => fileRef.current?.click()} className="sidebar-btn-sm flex items-center gap-1.5">
              <FileUp size={12} strokeWidth={1.75} aria-hidden />
              {t("Choose a .rec file")}
            </button>
            <span className="min-w-0 truncate text-[11.5px] theme-muted" title={file?.name}>
              {file ? file.name : t("No file chosen")}
            </span>
          </div>
          {file ? (
            <label className="flex items-center gap-2">
              <span className="text-[11.5px] theme-muted w-40 shrink-0">{t("Name")}</span>
              <input
                value={name}
                maxLength={MAX_RECORDING_NAME_CHARS}
                onChange={(e) => setName(e.target.value)}
                aria-label={t("Name of the imported recording")}
                className="sidebar-input text-xs flex-1 min-w-0"
              />
            </label>
          ) : null}
          {windows.length > 1 ? (
            <div className="flex items-center gap-2">
              <span className="text-[11.5px] theme-muted w-40 shrink-0">{t("Recorded in the window of")}</span>
              <Select
                value={chosen}
                options={windows.map((w) => ({ value: String(w.userId), label: w.name }))}
                ariaLabel={t("Window it was recorded in")}
                onChange={setUserId}
                className="flex-1 min-w-0"
              />
            </div>
          ) : null}
          <p className="text-[11px] theme-muted leading-4">
            {t("Clicks are saved relative to the window: record in a window the size of your accounts' windows and don't move it before importing.")}
          </p>
        </>
      )}
      {error ? (
        <div className="text-[11.5px] text-amber-300/90 leading-4" role="alert">
          {error}
        </div>
      ) : null}
      <div className="flex items-center gap-2">
        <button
          onClick={() => void importFile()}
          disabled={!file || !chosen || busy}
          className="theme-btn inline-flex items-center gap-1.5 min-h-[28px] px-3 py-1 text-[12px] font-medium disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {busy ? t("Importing...") : t("Import")}
        </button>
        <button onClick={onCancel} className="sidebar-btn-sm">
          {t("Cancel")}
        </button>
      </div>
    </div>
  );
}
