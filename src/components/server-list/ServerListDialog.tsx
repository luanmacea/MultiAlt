import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { usePrompt } from "../../hooks/usePrompt";
import { useJoinOnlineWarning } from "../../hooks/useJoinOnlineWarning";
import { useModalClose } from "../../hooks/useModalClose";
import { useTr } from "../../i18n/text";
import type { TabId, GameEntry } from "./types";
import { recordRecentGame, loadFavorites, saveFavorites } from "./types";
import { ServersTab } from "./ServersTab";
import { GamesTab } from "./GamesTab";
import { FavoritesTab } from "./FavoritesTab";
import { RecentTab } from "./RecentTab";
import { TabBar } from "./TabBar";
import { X } from "lucide-react";

interface ServerListDialogProps {
  open: boolean;
  onClose: () => void;
}

export function ServerListDialog({ open, onClose }: ServerListDialogProps) {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const confirmJoinOnline = useJoinOnlineWarning();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const [activeTab, setActiveTab] = useState<TabId>("servers");
  const [localPlaceId, setLocalPlaceId] = useState(store.placeId);
  const [refreshOnOpenSignal, setRefreshOnOpenSignal] = useState(0);
  const [jobIdPrefill, setJobIdPrefill] = useState("");
  const [jobIdPrefillNonce, setJobIdPrefillNonce] = useState(0);

  const maxRecent = parseInt(store.settings?.General?.MaxRecentGames || "8") || 8;
  const maxRecentJobs = parseInt(store.settings?.General?.MaxRecentJobs || "12") || 12;
  const userId = store.selectedAccount?.UserID || null;

  useEffect(() => {
    if (open) {
      setLocalPlaceId(store.placeId);
      setRefreshOnOpenSignal((v) => v + 1);
    }
  }, [open, store.placeId]);

  async function handleJoinServer(jobId: string) {
    store.setJobId(jobId);
    store.setPlaceId(localPlaceId);
    if (userId) {
      if (!(await confirmJoinOnline([userId]))) return;
      // O resultado (recusa/falha) já é reportado pelo próprio `joinServer`.
      void store.joinServer(userId);
    }
  }

  function handleSelectGame(placeId: number, name?: string, iconUrl?: string | null, privateServer?: string) {
    setLocalPlaceId(String(placeId));
    store.setPlaceId(String(placeId));
    setJobIdPrefill(privateServer || "");
    setJobIdPrefillNonce((v) => v + 1);
    setActiveTab("servers");
    recordRecentGame(placeId, userId, maxRecent, { name, iconUrl });
  }

  /**
   * Clique num servidor recente: **preenche** o Job ID e vai para a aba
   * Servers, não entra. Entrar é o gesto seguinte, com o aviso de conta online
   * que `handleJoinServer` já faz.
   */
  function handleSelectJob(placeId: number | null, raw: string) {
    if (placeId) {
      setLocalPlaceId(String(placeId));
      store.setPlaceId(String(placeId));
    }
    setJobIdPrefill(raw);
    setJobIdPrefillNonce((v) => v + 1);
    setActiveTab("servers");
  }

  /**
   * "Browse servers" (menu do jogo ou botão da linha): o mesmo destino do
   * clique no card — a aba Servers com o place, Job ID limpo —, sem gravar o
   * jogo nos recentes, como a Choose Game faz.
   */
  function handleBrowseServers(placeId: number) {
    setLocalPlaceId(String(placeId));
    store.setPlaceId(String(placeId));
    setJobIdPrefill("");
    setJobIdPrefillNonce((v) => v + 1);
    setActiveTab("servers");
  }

  /**
   * Auto Rejoin e Scripts **com este jogo**, as mesmas ações do menu na Choose
   * Game. Os dois abrem por cima deste diálogo (z-[70]); fechar volta aqui. O
   * place vai explícito ao Auto Rejoin porque o rascunho salvo vence a store.
   */
  function handleBottingForGame(placeId: number) {
    store.setPlaceId(String(placeId));
    store.openBottingDialog(String(placeId));
  }

  function handleScriptsForGame(placeId: number) {
    store.setPlaceId(String(placeId));
    store.setScriptsOpen(true);
  }

  async function handleJoinGame(placeId: number) {
    if (!userId) {
      store.addToast(t("No account selected"));
      return;
    }
    if (!(await confirmJoinOnline([userId]))) return;
    try {
      await invoke("launch_roblox", {
        userId,
        placeId,
        jobId: "",
        launchData: "",
        followUser: false,
        joinVip: false,
        linkCode: "",
        shuffleJob: false,
      });
      store.addToast(t("Launching game..."));
    } catch (e) {
      store.addToast(t("Failed to join: {{error}}", { error: String(e) }));
    }
  }

  async function handleAddFavorite(game: GameEntry) {
    const existing = loadFavorites();
    if (existing.some((f) => f.placeId === game.placeId)) {
      store.addToast(t("Already in favorites"));
      return;
    }
    const customName = await prompt(t("Favorite name:"), game.name);
    if (!customName?.trim()) return;
    existing.push({
      placeId: game.placeId,
      name: customName.trim(),
      iconUrl: game.iconUrl,
      addedAt: Date.now(),
    });
    saveFavorites(existing);
    store.addToast(t("Added to favorites"));
  }

  if (!visible) return null;

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      onClick={handleClose}
    >
      {/* O teto é o mesmo dos outros diálogos grandes. Sem ele, na janela
          mínima do app (750x450) os 560 px fixos transbordavam 55 px para cada
          lado: título e X ficavam fora da tela, e na base os campos Teleport e
          Find player — `body` não rola. Com o teto, quem cede altura é a lista
          de servidores, que já rola por dentro. */}
      <div
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[680px] h-[560px] max-w-[calc(100vw-24px)] max-h-[calc(100vh-24px)] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <div className="flex items-center gap-2.5">
            <h2 className="text-[15px] font-semibold text-zinc-100 tracking-tight">{t("Server List")}</h2>
            {userId && (
              <span className="text-[11px] text-zinc-600 bg-zinc-800/60 px-2 py-0.5 rounded">
                {store.selectedAccount?.Alias || store.selectedAccount?.Username}
              </span>
            )}
          </div>
          <button
            onClick={handleClose}
            className="p-1 rounded-md text-zinc-600 hover:text-zinc-400 hover:bg-zinc-800 transition-colors"
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <TabBar activeTab={activeTab} onTabChange={setActiveTab} />

        <div className="h-px bg-zinc-800/60 mx-5 mt-2.5" />

        <div className="flex-1 overflow-hidden px-5 py-3 min-h-0">
          {activeTab === "servers" && (
            <ServersTab
              placeId={localPlaceId}
              setPlaceId={setLocalPlaceId}
              onJoinServer={handleJoinServer}
              addToast={store.addToast}
              userId={userId}
              refreshOnOpenSignal={refreshOnOpenSignal}
              prefillJobId={jobIdPrefill}
              prefillNonce={jobIdPrefillNonce}
            />
          )}
          {/* O menu do jogo é o mesmo da Choose Game (Browse servers, Auto
              Rejoin, Scripts): as duas telas ficaram espelhadas pela metade —
              aqui havia a coluna de servidores recentes e não o menu
              completo; lá, o contrário. */}
          {activeTab === "games" && (
            <GamesTab
              onSelectGame={handleSelectGame}
              onJoinGame={handleJoinGame}
              addToast={store.addToast}
              onAddFavorite={handleAddFavorite}
              onBrowseServers={(placeId) => handleBrowseServers(placeId)}
              onBotting={handleBottingForGame}
              onScripts={handleScriptsForGame}
            />
          )}
          {activeTab === "favorites" && (
            <FavoritesTab
              onSelectGame={(placeId, privateServer) => handleSelectGame(placeId, undefined, undefined, privateServer)}
              addToast={store.addToast}
              onBrowseServers={handleBrowseServers}
              onBotting={handleBottingForGame}
              onScripts={handleScriptsForGame}
            />
          )}
          {activeTab === "recent" && (
            <RecentTab
              onSelectGame={(placeId, name, iconUrl) => handleSelectGame(placeId, name, iconUrl)}
              maxRecent={maxRecent}
              maxRecentJobs={maxRecentJobs}
              userId={userId}
              onSelectJob={handleSelectJob}
              onBrowseServers={handleBrowseServers}
              onAddFavorite={handleAddFavorite}
              onBotting={handleBottingForGame}
              onScripts={handleScriptsForGame}
            />
          )}
        </div>
      </div>
    </div>
  );
}
