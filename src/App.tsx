import { useEffect, useRef, useState } from "react";
import { StoreProvider, useStore } from "./store";
import { PromptProvider } from "./hooks/usePrompt";
import { PasswordScreen } from "./components/layout/PasswordScreen";
import { EncryptionSetupScreen } from "./components/layout/EncryptionSetupScreen";
import { FirstRunWalkthrough } from "./components/layout/FirstRunWalkthrough";
import { TitleBar } from "./components/layout/TitleBar";
import { ModalWindowControls } from "./components/layout/ModalWindowControls";
import { UpdateBanner } from "./components/layout/UpdateBanner";
import { Toolbar } from "./components/layout/Toolbar";
import { AccountList } from "./components/accounts/AccountList";
import { ContextMenu } from "./components/menus/ContextMenu";
import { DetailSidebar } from "./components/accounts/DetailSidebar";
import { BottomActionBar } from "./components/layout/BottomActionBar";
import { ChooseGameScreen } from "./components/ChooseGameScreen";
import { StatusBar } from "./components/layout/StatusBar";
import { SettingsDialog } from "./components/settings/SettingsDialog";
import { ServerListDialog } from "./components/server-list/ServerListDialog";
import { ImportDialog } from "./components/dialogs/ImportDialog";
import { AccountFieldsDialog } from "./components/dialogs/AccountFieldsDialog";
import { AccountUtilsDialog } from "./components/dialogs/AccountUtilsDialog";
import { MissingAssetsDialog } from "./components/dialogs/MissingAssetsDialog";
import { ThemeEditorDialog } from "./components/dialogs/ThemeEditorDialog";
import { UpdateDialog } from "./components/dialogs/UpdateDialog";
import { NexusDialog } from "./components/dialogs/NexusDialog";
import { BottingDialog } from "./components/dialogs/BottingDialog";
import { GeneratorDialog } from "./components/dialogs/GeneratorDialog";
import { VersionsDialog } from "./components/dialogs/VersionsDialog";
import { BackupsDialog } from "./components/dialogs/BackupsDialog";
import { IsolationProgressOverlay } from "./components/IsolationProgressOverlay";
import { ScriptsDialog } from "./components/dialogs/ScriptsDialog";
import { SessionDialog } from "./components/dialogs/SessionDialog";
import { useTr } from "./i18n/text";
import { TONE_STYLES } from "./utils/toastTone";
import { ENABLE_NEXUS } from "./featureFlags";

function AppContent() {
  const t = useTr();
  const store = useStore();
  const hasCheckedForUpdatesRef = useRef(false);
  // O diálogo de backups é aberto pelas Settings; o estado mora aqui porque a
  // store não expõe um flag para ele.
  const [backupsOpen, setBackupsOpen] = useState(false);
  const errorLower = (store.error || "").toLowerCase();
  const showCloseRobloxAction =
    errorLower.includes("failed to enable multi roblox") ||
    (errorLower.includes("multi roblox") && errorLower.includes("close all roblox process"));
  // O log de lançamento é a única explicação passo a passo do que falhou, e ele
  // mora na aba Console da Choose Game. Só vale apontar para lá quando existe
  // log: fora do launch, a faixa mandaria o usuário para uma tela vazia.
  const hasLaunchLog = store.launchLogs.length > 0;
  const anyModalOpen =
    store.settingsOpen ||
    store.serverListOpen ||
    store.importDialogOpen ||
    store.accountFieldsOpen ||
    store.accountUtilsOpen ||
    !!store.missingAssets ||
    store.themeEditorOpen ||
    store.bottingDialogOpen ||
    store.generatorDialogOpen ||
    (ENABLE_NEXUS && store.nexusOpen) ||
    store.scriptsOpen ||
    store.updateDialogOpen ||
    store.sessionDialogOpen ||
    backupsOpen ||
    store.firstRunWalkthroughOpen ||
    !!store.modal;

  useEffect(() => {
    if (!store.initialized || store.needsPassword || store.firstRunWalkthroughOpen) return;
    if (hasCheckedForUpdatesRef.current) return;
    hasCheckedForUpdatesRef.current = true;
    store.checkForUpdates();
  }, [store.checkForUpdates, store.firstRunWalkthroughOpen, store.initialized, store.needsPassword]);

  if (!store.initialized) {
    return (
      <div className="theme-app flex h-screen items-center justify-center">
        <div className="text-sm theme-muted">{t("Loading...")}</div>
      </div>
    );
  }

  if (store.needsPassword) {
    return <PasswordScreen />;
  }

  if (store.encryptionSetupOpen) {
    return <EncryptionSetupScreen />;
  }

  return (
    <div className="theme-app flex h-screen flex-col">
      <ModalWindowControls visible={anyModalOpen} />
      <TitleBar controlsHidden={anyModalOpen} />
      <UpdateBanner />
      <Toolbar />

      {store.error && (
        <div className="mx-4 mt-2 rounded-lg bg-red-500/10 border border-red-500/20 px-4 py-2 text-sm text-red-400 flex flex-wrap items-start justify-between gap-y-1 animate-fade-in">
          {/* O `truncate` cortava justamente o fim da mensagem, que é onde o
              backend explica o erro. Agora ela quebra linha; o teto de altura
              com rolagem impede que um erro enorme vire painel. */}
          <span className="min-w-0 flex-1 whitespace-pre-wrap break-words max-h-24 overflow-y-auto">
            {store.error}
          </span>
          <div className="ml-2 flex items-center gap-2 shrink-0">
            {showCloseRobloxAction && (
              <button
                onClick={() => store.killAllRobloxProcesses()}
                className="px-2 py-1 rounded-md bg-red-500/20 border border-red-500/30 text-red-300 hover:bg-red-500/30 transition-colors animate-pulse"
              >
                {t("Close Roblox")}
              </button>
            )}
            {/* O detalhe do que falhou no launch só existe no log, em outra
                tela. Enquanto houver log, a faixa diz onde ele está e abre a
                Choose Game — a aba Console é escolhida lá dentro. */}
            {hasLaunchLog && !store.chooseGameOpen && (
              <button
                onClick={() => store.setChooseGameOpen(true)}
                className="px-2 py-1 rounded-md bg-red-500/10 border border-red-500/30 text-red-300 hover:bg-red-500/20 transition-colors"
              >
                {t("Open launch log")}
              </button>
            )}
            <button
              onClick={() => store.setError(null)}
              className="text-red-500/60 hover:text-red-400 transition-colors"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6 6 18M6 6l12 12" />
              </svg>
            </button>
          </div>
          {hasLaunchLog && (
            <p className="basis-full text-xs text-red-400/70">
              {t("Step-by-step details of the last launch are in Choose Game › Console.")}
            </p>
          )}
        </div>
      )}

      <div className="flex flex-col flex-1 min-h-0">
        {store.chooseGameOpen ? (
          <ChooseGameScreen />
        ) : (
          <div className="flex flex-1 min-h-0">
            <AccountList />
            {/* O painel é de uma conta só. Quem garante que o botão da Toolbar
                não promete um painel que não vem é o `disabled` de lá, que usa
                exatamente esta condição — mudou aqui, muda lá. */}
            {store.sidebarOpen && store.selectedAccounts.length === 1 && <DetailSidebar />}
          </div>
        )}
        {!store.chooseGameOpen && store.selectedIds.size > 0 && <BottomActionBar />}
      </div>

      <StatusBar />

      <ContextMenu />

      {/* O tom vem pronto da store (`addToast` calcula uma vez) e a cor sai do
          mesmo mapa do Console de launch — antes a fila era toda cinza e um
          erro tinha exatamente a cara de um sucesso. A chave é o `id`: com
          `key={i}` a saída do primeiro toast remontava os que sobravam. */}
      {store.toasts.length > 0 && (
        <div className="fixed bottom-10 right-4 z-[60] flex flex-col gap-1.5">
          {store.toasts.map((toast) => (
            <div
              key={toast.id}
              className={`theme-panel theme-border backdrop-blur-lg px-4 py-2 rounded-lg text-xs shadow-xl animate-toast flex items-center gap-2 ${TONE_STYLES[toast.tone].text}`}
            >
              <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${TONE_STYLES[toast.tone].dot}`} />
              <span>{toast.message}</span>
            </div>
          ))}
        </div>
      )}

      <SettingsDialog
        open={store.settingsOpen}
        onClose={() => store.setSettingsOpen(false)}
        onSettingsChanged={store.reloadSettings}
        onRequestEncryptionSetup={() => {
          store.setSettingsOpen(false);
          store.openEncryptionSetupFromSettings();
        }}
        onRequestBackups={() => {
          store.setSettingsOpen(false);
          setBackupsOpen(true);
        }}
      />

      <ServerListDialog
        open={store.serverListOpen}
        onClose={() => store.setServerListOpen(false)}
      />

      <ImportDialog
        open={store.importDialogOpen}
        onClose={() => store.setImportDialogOpen(false)}
        defaultTab={store.importDialogTab}
      />

      <AccountFieldsDialog
        open={store.accountFieldsOpen}
        onClose={() => store.setAccountFieldsOpen(false)}
      />

      <AccountUtilsDialog
        open={store.accountUtilsOpen}
        onClose={() => store.setAccountUtilsOpen(false)}
      />

      <MissingAssetsDialog />

      <ThemeEditorDialog
        open={store.themeEditorOpen}
        onClose={() => store.setThemeEditorOpen(false)}
      />

      <BottingDialog
        open={store.bottingDialogOpen}
        onClose={() => store.setBottingDialogOpen(false)}
      />

      <GeneratorDialog
        open={store.generatorDialogOpen}
        initialTab={store.generatorDialogTab}
        onClose={() => store.setGeneratorDialogOpen(false)}
      />

      <VersionsDialog
        open={store.versionsDialogOpen}
        onClose={() => store.setVersionsDialogOpen(false)}
      />

      <BackupsDialog open={backupsOpen} onClose={() => setBackupsOpen(false)} />

      <SessionDialog
        open={store.sessionDialogOpen}
        onClose={() => store.setSessionDialogOpen(false)}
      />

      <IsolationProgressOverlay />

      {ENABLE_NEXUS && (
        <NexusDialog
          open={store.nexusOpen}
          onClose={() => store.setNexusOpen(false)}
        />
      )}

      <ScriptsDialog
        open={store.scriptsOpen}
        onClose={() => store.setScriptsOpen(false)}
      />

      <UpdateDialog />

      {store.firstRunWalkthroughOpen && <FirstRunWalkthrough />}

      {store.modal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
          onClick={store.closeModal}
        >
          <div
            className="theme-panel theme-border rounded-xl p-5 max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col shadow-2xl animate-scale-in"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-semibold text-[var(--panel-fg)]">{store.modal.title}</h3>
              <button
                onClick={store.closeModal}
                className="theme-muted hover:opacity-100 transition-opacity"
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6 6 18M6 6l12 12" />
                </svg>
              </button>
            </div>
            <pre className="theme-input text-xs font-mono rounded-lg p-4 overflow-auto flex-1">
              {store.modal.content}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

function App() {
  return (
    <StoreProvider>
      <PromptProvider>
        <AppContent />
      </PromptProvider>
    </StoreProvider>
  );
}

export default App;
