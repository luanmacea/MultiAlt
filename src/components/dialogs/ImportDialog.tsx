import { useState, useEffect, useRef } from "react";
import { X, File as FileIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { usePrompt } from "../../hooks/usePrompt";
import { SlidingTabBar } from "../ui/SlidingTabBar";
import { useTr } from "../../i18n/text";
import { parseImportLine } from "../../utils/cookies";

type TabId = "cookie" | "userpass" | "legacy";

interface ImportResult {
  text: string;
  ok: boolean;
}

interface OldAccountImportSummary {
  total: number;
  added: number;
  replaced: number;
  skipped: number;
}

export function ImportDialog({
  open,
  onClose,
  defaultTab = "cookie",
}: {
  open: boolean;
  onClose: () => void;
  defaultTab?: TabId;
}) {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const [tab, setTab] = useState<TabId>("cookie");
  const [input, setInput] = useState("");
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState("");
  const [results, setResults] = useState<ImportResult[]>([]);
  const [selectedFileName, setSelectedFileName] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setTab(defaultTab);
    setInput("");
    setResults([]);
    setProgress("");
    setSelectedFileName("");
  }, [open, handleClose, defaultTab]);

  useEffect(() => {
    if (open && tab === "cookie" && textareaRef.current) {
      setTimeout(() => textareaRef.current?.focus(), 100);
    }
  }, [open, tab]);

  if (!visible) return null;

  /**
   * Importa uma linha que traz cookie — sozinho ou em `user:pass:cookie`.
   *
   * A senha só é enviada quando a linha a traz: `add_account` a recebe como
   * `Option<String>`, e omitir a chave mantém o import por cookie puro
   * exatamente como era.
   */
  async function importByCookie(
    parsed: { password: string; cookie: string },
    existingIds: Set<number>
  ): Promise<ImportResult> {
    try {
      const info = await invoke<{ user_id: number; name: string }>("validate_cookie", {
        cookie: parsed.cookie,
      });
      if (existingIds.has(info.user_id)) {
        // A conta já existe, então `add_account` nem é chamado — e com ele fica
        // de fora a senha que a linha trazia. Dizer só "already exists" deixa
        // pensar que a senha foi guardada.
        return {
          text: parsed.password
            ? t("{{name}} - already exists; the password in this line was not stored", {
                name: info.name,
              })
            : t("{{name}} - already exists", { name: info.name }),
          ok: false,
        };
      }
      await invoke("add_account", {
        securityToken: parsed.cookie,
        username: info.name,
        userId: info.user_id,
        ...(parsed.password ? { password: parsed.password } : {}),
      });
      existingIds.add(info.user_id);
      return { text: t("Added {{name}}", { name: info.name }), ok: true };
    } catch (e) {
      return { text: t("Failed: {{error}}", { error: String(e) }), ok: false };
    }
  }

  async function handleImportCookie() {
    const lines = input.split("\n").map((l) => l.trim()).filter(Boolean);
    if (lines.length === 0) return;
    setImporting(true);
    setResults([]);

    const existingIds = new Set(store.accounts.map((a) => a.UserID));
    const out: ImportResult[] = [];

    for (let i = 0; i < lines.length; i++) {
      setProgress(t("Importing {{current}}/{{total}}...", { current: i + 1, total: lines.length }));
      const parsed = parseImportLine(lines[i]);
      // Credencial incompleta não entra pela metade: sem cookie não há sessão
      // para usar, e uma conta gravada só com senha nunca abre nada.
      if (parsed.kind === "userpass") {
        out.push({
          text: t("Skipped {{name}}: no cookie in this line", { name: parsed.username }),
          ok: false,
        });
      } else {
        out.push(await importByCookie(parsed, existingIds));
      }
      setResults([...out]);
    }

    await store.loadAccounts();
    setProgress("");
    setImporting(false);
  }

  async function handleImportUserPass() {
    const lines = input.split("\n").map((l) => l.trim()).filter(Boolean);
    if (lines.length === 0) return;
    setImporting(true);
    setResults([]);

    const existingIds = new Set(store.accounts.map((a) => a.UserID));
    const out: ImportResult[] = [];
    for (let i = 0; i < lines.length; i++) {
      setProgress(t("Importing {{current}}/{{total}}...", { current: i + 1, total: lines.length }));
      const parsed = parseImportLine(lines[i]);
      // Linha que já traz o cookie não precisa de navegador nenhum: a sessão
      // está ali, e abrir o login seria pedir CAPTCHA por nada.
      if (parsed.kind === "cookie") {
        out.push(await importByCookie(parsed, existingIds));
        setResults([...out]);
        continue;
      }
      if (parsed.kind !== "userpass") {
        out.push({ text: t("Skipped invalid line (use username:password)"), ok: false });
        setResults([...out]);
        continue;
      }
      try {
        const info = await invoke<{ user_id: number; name: string }>("import_userpass", {
          username: parsed.username,
          password: parsed.password,
        });
        existingIds.add(info.user_id);
        out.push({ text: t("Added {{name}}", { name: info.name }), ok: true });
      } catch (e) {
        out.push({ text: t("Failed: {{error}}", { error: String(e) }), ok: false });
      }
      setResults([...out]);
    }

    await store.loadAccounts();
    setProgress("");
    setImporting(false);
  }

  async function importOldAccountData(file: File) {
    if (file.name.toLowerCase() !== "accountdata.json") {
      setResults([{ text: t("Please select AccountData.json"), ok: false }]);
      return;
    }

    setImporting(true);
    setResults([]);
    setSelectedFileName(file.name);

    try {
      setProgress(t("Reading file..."));
      const raw = await file.arrayBuffer();
      const fileData = Array.from(new Uint8Array(raw));
      let password: string | null = null;
      let summary: OldAccountImportSummary | null = null;

      while (summary === null) {
        try {
          setProgress(password === null ? t("Importing accounts...") : t("Decrypting and importing..."));
          summary = await invoke<OldAccountImportSummary>("import_old_account_data", {
            fileData,
            password,
          });
        } catch (e) {
          const errorMessage = String(e);
          if (errorMessage.includes("IMPORT_PASSWORD_REQUIRED")) {
            // Não é só "esqueci a senha": um vault cifrado pela chave do
            // aparelho de **outra** instalação (ou da mesma antes de uma troca de
            // método) chega aqui, e nunca teve senha nenhuma. Pedir a senha sem
            // dizer isso manda o usuário adivinhar uma senha inexistente.
            const entered = await prompt(
              t(
                "This AccountData.json is encrypted. Enter its password — or cancel, if it was never password-protected: a file locked with another installation's device key can only be imported together with that installation's AccountData.key.",
              ),
            );
            if (entered === null) {
              setResults([{ text: t("Import cancelled"), ok: false }]);
              return;
            }
            password = entered;
            continue;
          }
          if (errorMessage.toLowerCase().includes("import password is incorrect")) {
            const retry = await prompt(t("Password is incorrect. Enter the AccountData.json password:"));
            if (retry === null) {
              setResults([{ text: t("Import cancelled"), ok: false }]);
              return;
            }
            password = retry;
            continue;
          }
          throw e;
        }
      }

      await store.loadAccounts();

      const out: ImportResult[] = [];
      if (summary.added > 0) {
        out.push({ text: t("Added {{count}} account(s)", { count: summary.added }), ok: true });
      }
      if (summary.replaced > 0) {
        out.push({ text: t("Replaced {{count}} existing account(s)", { count: summary.replaced }), ok: true });
      }
      if (summary.skipped > 0) {
        out.push({ text: t("Skipped {{count}} duplicate/invalid record(s)", { count: summary.skipped }), ok: false });
      }
      if (summary.total === 0 || out.length === 0) {
        out.push({ text: t("No accounts imported"), ok: false });
      }

      setResults(out);
      store.addToast(t("Import complete: +{{added}}, replaced {{replaced}}", { added: summary.added, replaced: summary.replaced }));
    } catch (e) {
      setResults([{ text: t("Failed: {{error}}", { error: String(e) }), ok: false }]);
    } finally {
      setProgress("");
      setImporting(false);
    }
  }

  async function handleLegacyFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    await importOldAccountData(file);
  }

  async function handleLegacyFileDrop(e: React.DragEvent<HTMLButtonElement>) {
    e.preventDefault();
    const file = e.dataTransfer.files?.[0];
    if (!file) return;
    await importOldAccountData(file);
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    if (tab !== "cookie") return;
    const text = e.dataTransfer.getData("text/plain");
    if (text) setInput((prev) => (prev ? `${prev}\n${text}` : text));
  }

  function handleTabChange(id: TabId) {
    setTab(id);
    setResults([]);
    setProgress("");
    setInput("");
    setSelectedFileName("");
  }

  const tabs: { id: TabId; label: string }[] = [
    { id: "cookie", label: t("Import by Cookie") },
    { id: "userpass", label: t("Import by User:Pass") },
    { id: "legacy", label: t("Import Old Account Data") },
  ];

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      onClick={handleClose}
    >
      <div
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[560px] ${tab === "legacy" ? "max-h-[320px]" : tab === "cookie" ? "max-h-[520px]" : "max-h-[420px]"} flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-0">
          <h2 className="text-sm font-semibold text-zinc-100">{t("Import Accounts")}</h2>
          <button onClick={handleClose} className="text-zinc-600 hover:text-zinc-400 transition-colors">
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="pt-1">
          <SlidingTabBar
            tabs={tabs}
            activeTab={tab}
            onTabChange={(id) => handleTabChange(id as TabId)}
          />
        </div>

        <div className={`px-5 pb-4 ${tab === "legacy" ? "pt-2" : "flex-1 flex flex-col min-h-0"}`}>
          {tab === "cookie" ? (
            <>
              {/*
                O cookie era pedido cru: nem o que é, nem onde achar, nem o que
                entrega. `validate_cookie` (api/auth.rs:45) manda só o cookie e
                recebe a conta de volta — quem o tem está logado como ela, sem
                senha e sem verificação em duas etapas. Isso tem que estar na
                tela, não só no aviso que o próprio cookie carrega no texto.
              */}
              <p className="text-[12px] text-zinc-500 mb-1.5">
                {t("Paste one .ROBLOSECURITY cookie per line, or one username:password:cookie per line.")}
              </p>
              <p className="text-[12px] text-amber-300/80 leading-snug mb-1.5">
                {t(
                  "This cookie is the account's whole session: anyone holding it is signed in as that account, with no password and no 2-step verification. Treat it like the account itself."
                )}
              </p>
              {/*
                Aceitar a senha na mesma caixa aumenta o que está em jogo: o
                cookie entrega a sessão (e pode ser revogado saindo de todas as
                sessões), a senha entrega a conta — troca de e-mail, troca de
                senha, recuperação. E ela fica no `AccountData.json`, que só é
                encriptado quando o app tem senha (`data/crypto.rs`). O aviso
                de cima falava só do cookie.
              */}
              <p className="text-[12px] text-amber-300/80 leading-snug mb-1.5">
                {t(
                  "A username:password:cookie line also saves the password, which is more than the session: it can change the account's email and password, and signing out of every session does not revoke it. It is stored in AccountData.json, encrypted only if you set an app password."
                )}
              </p>
              <p className="text-[12px] text-zinc-500 leading-snug mb-2">
                {t(
                  "Where to find it: sign in to roblox.com in your browser, open DevTools (F12) › Application › Cookies › https://www.roblox.com and copy the .ROBLOSECURITY value."
                )}
              </p>
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onDrop={handleDrop}
                onDragOver={(e) => e.preventDefault()}
                disabled={importing}
                placeholder={t("_|WARNING:-DO-NOT-SHARE...")}
                className="flex-1 min-h-[100px] max-h-[140px] w-full p-3 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-xs text-zinc-300 font-mono placeholder-zinc-600 resize-none focus:outline-none focus:border-zinc-600 transition-colors disabled:opacity-50"
                spellCheck={false}
              />
            </>
          ) : tab === "userpass" ? (
            <>
              <p className="text-[12px] text-zinc-500 mb-2">{t("Paste one username:password per line. A secure browser opens for each to finish sign-in.")}</p>
              <p className="text-[12px] text-zinc-500 mb-2">{t("A line that already carries the cookie (username:password:cookie) is imported straight away, with no browser.")}</p>
              <textarea
                value={input}
                onChange={(e) => setInput(e.target.value)}
                disabled={importing}
                placeholder={t("username:password")}
                className="flex-1 min-h-[100px] max-h-[140px] w-full p-3 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-xs text-zinc-300 font-mono placeholder-zinc-600 resize-none focus:outline-none focus:border-zinc-600 transition-colors disabled:opacity-50"
                spellCheck={false}
              />
              <p className="text-[12px] text-zinc-600 mt-2">{t("Complete any CAPTCHA or 2-step verification in the browser window. This finishes automatically.")}</p>
            </>
          ) : (
            <>
              <p className="text-[12px] text-zinc-500 mb-3">{t("Select an old AccountData.json file to merge into your current accounts.")}</p>
              <input
                ref={fileInputRef}
                type="file"
                accept=".json,application/json"
                className="hidden"
                onChange={handleLegacyFileChange}
              />
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                onDrop={handleLegacyFileDrop}
                onDragOver={(e) => e.preventDefault()}
                disabled={importing}
                className="w-full px-4 py-3 bg-zinc-800/50 hover:bg-zinc-700/55 border border-dashed border-zinc-600/70 rounded-xl text-left transition-colors disabled:opacity-50"
              >
                <div className="flex items-center gap-3">
                  <div className="h-8 w-8 rounded-lg bg-zinc-900/70 border border-zinc-700/70 flex items-center justify-center text-zinc-400">
                    <FileIcon size={14} strokeWidth={1.75} />
                  </div>
                  <div>
                    <div className="text-xs text-zinc-200 font-medium">{t("Choose AccountData.json")}</div>
                    <div className="text-[12px] text-zinc-500">{t("Click to browse or drop file here")}</div>
                  </div>
                </div>
              </button>
              <div className="mt-2 text-[12px] text-zinc-500 min-h-[16px] truncate">
                {selectedFileName
                  ? t("Selected file: {{name}}", { name: selectedFileName })
                  : t("No file selected")}
              </div>
            </>
          )}

          {results.length > 0 && (
            <div className="mt-2 max-h-[80px] overflow-y-auto space-y-0.5">
              {results.map((r, i) => (
                <div key={i} className={`text-[12px] ${r.ok ? "text-emerald-400" : "text-red-400"}`}>
                  {r.text}
                </div>
              ))}
            </div>
          )}

          <div className={`flex items-center mt-3 ${tab === "legacy" ? "justify-start" : "justify-between"}`}>
            <span className="text-[12px] text-zinc-500">{progress}</span>
            {tab === "cookie" && (
              <button
                onClick={handleImportCookie}
                disabled={importing || !input.trim()}
                className="px-4 py-1.5 bg-sky-600 hover:bg-sky-500 disabled:bg-zinc-700 disabled:text-zinc-500 text-white text-xs font-medium rounded-lg transition-colors"
              >
                {importing ? t("Importing...") : t("Import")}
              </button>
            )}
            {tab === "userpass" && (
              <button
                onClick={handleImportUserPass}
                disabled={importing || !input.trim()}
                className="px-4 py-1.5 bg-sky-600 hover:bg-sky-500 disabled:bg-zinc-700 disabled:text-zinc-500 text-white text-xs font-medium rounded-lg transition-colors"
              >
                {importing ? t("Signing in...") : t("Log in & Add")}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
