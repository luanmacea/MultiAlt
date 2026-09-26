import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { usePrompt } from "../../hooks/usePrompt";
import { Tooltip } from "../ui/Tooltip";
import { tr, useTr } from "../../i18n/text";
import { ENABLE_NEXUS } from "../../featureFlags";
import { SessionToolbarButton } from "../dialogs/SessionDialog";
import { Search, X, SquareX, SquareCheckBig, PanelRight, Plus, ChevronDown, Globe, KeyRound, File, FileText, Palette, Layers, Settings, TerminalSquare, Sparkles, Package, UserPlus, CircleHelp } from "lucide-react";

export function Toolbar() {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  const addRef = useRef<HTMLDivElement>(null);
  const activeToggleStyle = "theme-accent theme-accent-bg theme-accent-border";

  // O painel lateral (DetailSidebar) só existe para uma conta — ver App.tsx.
  // Com 0 ou 2+ selecionadas o botão ficava aceso e nada abria; aqui ele fica
  // desabilitado e o tooltip diz o que falta, em vez de fingir que ligou.
  const panelAvailable = store.selectedAccounts.length === 1;
  const panelTooltip = panelAvailable
    ? store.sidebarOpen
      ? t("Hide panel")
      : t("Show panel")
    : store.selectedAccounts.length === 0
      ? t("Select an account to show its panel")
      : t("The panel shows one account at a time");

  // Tooltip aparece só depois de 350 ms de mouse parado: leitor de tela e
  // teste ficavam sem nome nenhum nos botões de ícone. O mesmo texto vira
  // `aria-label`, então os dois caminhos dizem a mesma coisa.
  const selectAllLabel =
    store.selectedIds.size > 0
      ? t("Deselect all ({{count}})", { count: store.selectedIds.size })
      : t("Select all");

  // O rótulo `Names`/`Hidden` não dizia o que o botão faz. Agora o texto conta
  // o estado ("Names shown"/"Names hidden") e o tooltip conta a ação.
  const namesTooltip = store.hideUsernames
    ? t("Show the usernames in the list again")
    : t("Mask the usernames in the list (for screenshots)");

  useEffect(() => {
    if (!addMenuOpen) return;
    function handleClick(e: MouseEvent) {
      if (addRef.current && !addRef.current.contains(e.target as Node)) {
        setAddMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [addMenuOpen]);

  function handleBrowserLogin() {
    setAddMenuOpen(false);
    store.openLoginBrowser();
  }

  function handleUserPassLogin() {
    setAddMenuOpen(false);
    store.setImportDialogTab("userpass");
    store.setImportDialogOpen(true);
  }

  function handleImportCookie() {
    setAddMenuOpen(false);
    store.setImportDialogTab("cookie");
    store.setImportDialogOpen(true);
  }

  function handleImportOldAccountData() {
    setAddMenuOpen(false);
    store.setImportDialogTab("legacy");
    store.setImportDialogOpen(true);
  }

  function handleOpenGenerator() {
    setAddMenuOpen(false);
    store.openGeneratorDialog("provider");
  }

  function handleOpenSignup() {
    setAddMenuOpen(false);
    store.openGeneratorDialog("signup");
  }

  function handleOpenVersions() {
    setAddMenuOpen(false);
    store.setVersionsDialogOpen(true);
  }

  async function handleQuickAdd() {
    setAddMenuOpen(false);
    // "Cookie or username" não dizia que cookie é esse, onde ele está nem o que
    // ele entrega — e ele entra como a conta inteira (api/auth.rs:45). Mesmo
    // texto do Quick Add do AddAccountDialog.
    const input = await prompt(
      tr(
        "Paste a .ROBLOSECURITY cookie — it signs in as that account, and you find it in your browser's DevTools › Application › Cookies on roblox.com — or type a username to add it without a session."
      )
    );
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
      // Busca por nome de usuário não entrega cookie nenhum: a conta entra só
      // como registro, sem sessão, e não lança. Dizer só "Added" fazia parecer
      // que tinha dado certo — o aviso tem que nomear o que falta.
      store.addToast(
        tr("Added {{name}} with no session — paste its cookie or use Browser Login to sign in", {
          name: user.name,
        })
      );
    } catch (e) {
      store.addToast(tr("Add failed: {{error}}", { error: String(e) }));
    }
  }

  return (
    <div className="theme-panel theme-border flex items-center gap-3 px-4 py-2 border-b shrink-0">
      <div className="relative flex-1 max-w-xs">
        <Search size={15} strokeWidth={2} className="absolute left-3 top-1/2 -translate-y-1/2 theme-muted" />
        <input
          type="text"
          value={store.searchQuery}
          onChange={(e) => store.setSearchQuery(e.target.value)}
          onClick={(e) => e.stopPropagation()}
          placeholder={t("Filter accounts...")}
          autoComplete="off"
          spellCheck={false}
          className="theme-input w-full pl-9 pr-3 py-1.5 rounded-lg text-sm transition-colors"
        />
        {store.searchQuery && (
          <Tooltip content={t("Clear search")} side="bottom">
            <button
              onClick={() => store.setSearchQuery("")}
              aria-label={t("Clear search")}
              className="absolute right-2 top-1/2 -translate-y-1/2 theme-muted hover:opacity-100"
            >
              <X size={14} strokeWidth={2} />
            </button>
          </Tooltip>
        )}
      </div>

      <div className="flex items-center gap-1.5 ml-auto">
        <Tooltip content={selectAllLabel} side="bottom">
          <button
            onClick={() => store.toggleSelectAll()}
            aria-label={selectAllLabel}
            className={`px-2.5 py-1.5 text-xs rounded-lg border transition-colors ${
              store.selectedIds.size > 0
                ? activeToggleStyle
                : "theme-btn-ghost"
            }`}
          >
            {store.selectedIds.size > 0 ? (
              <SquareX size={14} strokeWidth={2} />
            ) : (
              <SquareCheckBig size={14} strokeWidth={2} />
            )}
          </button>
        </Tooltip>

        <Tooltip content={namesTooltip} side="bottom">
          <button
            onClick={() => store.setHideUsernames(!store.hideUsernames)}
            className={`px-2.5 py-1.5 text-xs rounded-lg border transition-colors ${
              store.hideUsernames
                ? activeToggleStyle
                : "theme-btn-ghost"
            }`}
          >
            {store.hideUsernames ? t("Names hidden") : t("Names shown")}
          </button>
        </Tooltip>

        <Tooltip content={panelTooltip} side="bottom">
          <button
            onClick={() => store.setSidebarOpen(!store.sidebarOpen)}
            disabled={!panelAvailable}
            aria-label={panelTooltip}
            className={`p-1.5 rounded-lg border transition-colors ${
              !panelAvailable
                ? "theme-btn-ghost opacity-40 cursor-not-allowed"
                : store.sidebarOpen
                ? activeToggleStyle
                : "theme-btn-ghost"
            }`}
          >
            <PanelRight size={16} strokeWidth={1.5} />
          </button>
        </Tooltip>

        <div className="w-px h-5 mx-1 bg-[var(--border-color)]" />

        <div ref={addRef} className="relative">
          <button
            onClick={() => setAddMenuOpen(!addMenuOpen)}
            data-tour="toolbar-add"
            className="theme-btn flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors"
          >
            <Plus size={14} strokeWidth={2.5} />
            {t("Add")}
            <ChevronDown size={10} strokeWidth={2.5} />
          </button>
          {addMenuOpen && (
            <div className="theme-panel theme-border absolute right-0 top-full mt-1.5 w-64 border rounded-xl shadow-2xl z-50 animate-scale-in py-1">
              <button
                onClick={handleQuickAdd}
                className="flex items-center gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <Plus size={14} strokeWidth={1.5} className="theme-muted" />
                {t("Quick Add")}
              </button>
              <button
                onClick={handleBrowserLogin}
                data-tour="add-browser-login"
                className="flex items-center gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <Globe size={14} strokeWidth={1.5} className="theme-muted" />
                {t("Browser Login")}
              </button>
              <button
                onClick={handleUserPassLogin}
                className="flex items-center gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <KeyRound size={14} strokeWidth={1.5} className="theme-muted" />
                {t("User:Pass Login")}
              </button>
              <div className="my-1 border-t theme-border" />
              <button
                onClick={handleImportCookie}
                className="flex items-center gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <File size={14} strokeWidth={1.5} className="theme-muted" />
                {t("Import Cookie")}
              </button>
              <button
                onClick={handleImportOldAccountData}
                className="flex items-center gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <FileText size={14} strokeWidth={1.5} className="theme-muted" />
                {t("Import Old Account Data")}
              </button>
              <div className="my-1 border-t theme-border" />
              {/*
                As duas entradas que trazem conta nova são bem diferentes e a
                tela não dizia nada: uma cria de graça no navegador embutido
                (a pessoa resolve o CAPTCHA), a outra compra conta pronta de um
                serviço pago de terceiro. Ver docs/features/account-creation.md.
                O mesmo texto aparece no AddAccountDialog — as duas portas têm
                que dizer a mesma coisa.
              */}
              <button
                onClick={handleOpenSignup}
                className="flex items-start gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <UserPlus size={14} strokeWidth={1.5} className="theme-muted mt-0.5 shrink-0" />
                <span className="min-w-0">
                  {t("Create Accounts")}
                  <span className="block text-[12px] theme-muted leading-snug">
                    {t("Free — the app fills Roblox's signup form; you solve the CAPTCHA")}
                  </span>
                </span>
              </button>
              <button
                onClick={handleOpenGenerator}
                className="flex items-start gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <Sparkles size={14} strokeWidth={1.5} className="theme-muted mt-0.5 shrink-0" />
                <span className="min-w-0">
                  {t("Account Generator")}
                  <span className="block text-[12px] theme-muted leading-snug">
                    {t("Paid — buys ready-made accounts from BloxGen (third party, API key)")}
                  </span>
                </span>
              </button>
              <button
                onClick={handleOpenVersions}
                className="flex items-center gap-2.5 w-full px-3.5 py-2 text-sm text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] text-left"
              >
                <Package size={14} strokeWidth={1.5} className="theme-muted" />
                {t("Roblox Versions")}
              </button>
            </div>
          )}
        </div>

        {/*
          O passo do walkthrough que destaca a Sessão achava este botão pelo
          `aria-label` traduzido — em outro idioma o tour perdia o alvo. O
          `data-tour` vai no invólucro porque o botão em si mora no
          SessionDialog; o span colado no botão dá o mesmo retângulo.
        */}
        <span data-tour="toolbar-session" className="inline-flex">
          <SessionToolbarButton />
        </span>

        <Tooltip content={t("Theme")} side="bottom">
          <button
            onClick={() => store.setThemeEditorOpen(true)}
            aria-label={t("Theme")}
            className="theme-btn-ghost p-1.5 rounded-lg transition-colors"
          >
            <Palette size={16} strokeWidth={1.5} />
          </button>
        </Tooltip>

        {ENABLE_NEXUS && (
          <Tooltip content="Nexus" side="bottom">
            <button
              onClick={() => store.setNexusOpen(true)}
              aria-label="Nexus"
              className="theme-btn-ghost p-1.5 rounded-lg transition-colors"
            >
              <Layers size={16} strokeWidth={1.5} />
            </button>
          </Tooltip>
        )}

        <Tooltip content={t("Scripts")} side="bottom">
          <button
            onClick={() => store.setScriptsOpen(true)}
            aria-label={t("Scripts")}
            className="theme-btn-ghost p-1.5 rounded-lg transition-colors"
          >
            <TerminalSquare size={16} strokeWidth={1.5} />
          </button>
        </Tooltip>

        <Tooltip content={t("Settings")} side="bottom">
          <button
            onClick={() => store.setSettingsOpen(true)}
            data-tour="toolbar-settings"
            aria-label={t("Settings")}
            className="theme-btn-ghost p-1.5 rounded-lg transition-colors"
          >
            <Settings size={16} strokeWidth={1.5} />
          </button>
        </Tooltip>

        {/*
          Ponto de ajuda. Escolhi reabrir o walkthrough de primeira execução em
          vez de abrir a documentação: é conteúdo que já existe, é mantido junto
          com a interface e aponta para os controles reais da tela — nada novo
          foi inventado aqui. A documentação já tem porta própria (o botão do
          GitHub na barra de título), e o backend só sabe abrir a raiz do
          repositório (`open_repo_url`, sem argumento), o que deixaria de fora o
          `docs/mapa-da-interface.md` que a pessoa está procurando.
        */}
        {/*
          Tooltip curto de propósito: encostado na borda direita da janela ele
          encolhe até a maior palavra e sai da tela. O nome completo fica no
          `aria-label`, que não tem esse limite.
        */}
        <Tooltip content={t("Help")} side="bottom">
          <button
            onClick={store.openFirstRunWalkthroughFromSettings}
            aria-label={t("Help — replay the walkthrough")}
            className="theme-btn-ghost p-1.5 rounded-lg transition-colors"
          >
            <CircleHelp size={16} strokeWidth={1.5} />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
