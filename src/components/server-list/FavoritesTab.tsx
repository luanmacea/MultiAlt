import { useEffect, useRef, useState } from "react";
import { useConfirm, usePrompt } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import type { FavoriteGame, VipServer } from "./types";
import { loadFavorites, saveFavorites, makeVipId } from "./types";
import { FavoriteContextMenu } from "./FavoriteContextMenu";
import { GameRowActions, browseServersIcon } from "./GamesTab";
import { loadGameIdentity } from "../../hooks/useGameIdentity";

/**
 * O texto colado em "Private server link or VIP code" parece plausível?
 *
 * O Rust (`extract_private_server_link_code` em
 * `src-tauri/src/commands/launch_shared.rs`, e `normalize_private_server_link_code`
 * em `api/roblox/private_links.rs`) aceita quase qualquer string não-vazia como
 * código literal — inclusive um código puro sem prefixo `vip:` e sem formato de
 * URL (ver o teste `normalize_private_server_link_code("  plain-code  ")`).
 * Então não dá para inventar uma regra de formato nova aqui sem brigar com o
 * que o Rust já aceita: a validação do front só rejeita o que é claramente
 * OUTRA coisa — vazio, texto com espaço no meio (link/código nunca tem) ou sem
 * nenhum caractere alfanumérico.
 *
 * O prefixo `vip:` (convenção de Job ID VIP do projeto) é descontado antes do
 * teste de espaço, porque o próprio Rust aceita espaço logo depois dele
 * (`extract_private_server_link_code("VIP:  ABC%20123 ")`).
 */
export function isPlausibleVipLink(text: string): boolean {
  let value = text.trim();
  if (!value) return false;

  const vipPrefix = value.match(/^vip:\s*/i);
  if (vipPrefix) value = value.slice(vipPrefix[0].length);
  if (!value) return false;

  if (/\s/.test(value)) return false;
  if (!/[a-z0-9]/i.test(value)) return false;
  return true;
}

export interface FavoritesTabProps {
  onSelectGame: (placeId: number, privateServer?: string) => void;
  addToast: (msg: string) => void;
  /**
   * Abre a lista de servidores daquele jogo. Opcional porque o diálogo de
   * servidores (fora da Choose Game) não tem para onde levar.
   */
  onBrowseServers?: (placeId: number) => void;
  /** Abre o Botting Mode / os Scripts **com este jogo**, sem copiar Place ID. */
  onBotting?: (placeId: number) => void;
  onScripts?: (placeId: number) => void;
}

export function FavoritesTab({
  onSelectGame,
  addToast,
  onBrowseServers,
  onBotting,
  onScripts,
}: FavoritesTabProps) {
  const t = useTr();
  const prompt = usePrompt();
  const confirm = useConfirm();
  const [favorites, setFavorites] = useState<FavoriteGame[]>(loadFavorites);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; game: FavoriteGame } | null>(null);
  const [addingVipFor, setAddingVipFor] = useState<number | null>(null);
  const backfilledRef = useRef(false);

  /**
   * Completa o ícone dos favoritos salvos sem ele.
   *
   * O favorito guarda o ícone no `localStorage` na hora em que é salvo: quem
   * entrou antes de o ícone existir (ou por um caminho que não o tinha em mãos)
   * ficava com o quadrado vazio **para sempre**, porque ninguém tentava de
   * novo — enquanto a aba Games, ao lado, mostra o ícone de todos. Os Recentes
   * já se completavam assim.
   *
   * Uma passada só por montagem (`backfilledRef`), e o resultado é gravado para
   * a próxima abertura não custar rede nenhuma.
   */
  useEffect(() => {
    if (backfilledRef.current) return;
    backfilledRef.current = true;

    let cancelled = false;
    void (async () => {
      const pendentes = loadFavorites().filter((f) => !f.iconUrl);
      if (pendentes.length === 0) return;

      let mudou = false;
      const achados = new Map<number, string>();
      for (const favorito of pendentes) {
        const { iconUrl } = await loadGameIdentity(favorito.placeId, null);
        if (cancelled) return;
        if (iconUrl) {
          achados.set(favorito.placeId, iconUrl);
          mudou = true;
        }
      }
      if (!mudou) return;

      // Relê na hora de gravar: o usuário pode ter renomeado ou removido
      // favorito enquanto os ícones vinham.
      const atuais = loadFavorites().map((f) =>
        !f.iconUrl && achados.has(f.placeId) ? { ...f, iconUrl: achados.get(f.placeId)! } : f
      );
      saveFavorites(atuais);
      setFavorites(atuais);
    })();

    return () => {
      cancelled = true;
    };
  }, []);
  const [vipDraftLink, setVipDraftLink] = useState("");
  const [vipDraftName, setVipDraftName] = useState("");
  const [vipDraftError, setVipDraftError] = useState("");

  function persist(updated: FavoriteGame[]) {
    setFavorites(updated);
    saveFavorites(updated);
  }

  function getVips(game: FavoriteGame): VipServer[] {
    return game.vipServers ?? [];
  }

  async function handleRename(game: FavoriteGame) {
    const newName = await prompt(t("Rename favorite:"), game.name);
    if (!newName?.trim()) return;
    persist(favorites.map((f) => (f.placeId === game.placeId ? { ...f, name: newName.trim() } : f)));
    addToast(t("Renamed"));
  }

  /**
   * Favoritos e seus VIPs só existem neste computador (`localStorage`): a
   * frase diz o que desaparece, incluindo os VIPs que vão junto.
   */
  async function handleRemove(game: FavoriteGame) {
    const vipCount = getVips(game).length;
    const ok = await confirm(
      vipCount === 0
        ? t(
            'Remove "{{name}}" from favorites? It is deleted from this computer and cannot be recovered.',
            { name: game.name }
          )
        : t(
            'Remove "{{name}}" from favorites? The favorite and its saved VIP servers ({{count}}) are deleted from this computer and cannot be recovered.',
            { name: game.name, count: vipCount }
          ),
      true
    );
    if (!ok) return;
    persist(favorites.filter((f) => f.placeId !== game.placeId));
    addToast(t("Removed from favorites"));
  }

  function startAddVip(placeId: number) {
    setAddingVipFor(placeId);
    setVipDraftLink("");
    setVipDraftName("");
    setVipDraftError("");
  }

  function cancelAddVip() {
    setAddingVipFor(null);
    setVipDraftLink("");
    setVipDraftName("");
    setVipDraftError("");
  }

  /**
   * Era `prompt()` do link seguido de `prompt()` do nome: cancelar o segundo
   * (o nome, opcional) jogava fora o link que a pessoa acabou de digitar no
   * primeiro. Agora é um formulário inline na própria linha do favorito — os
   * dois campos juntos, sem diálogo nenhum no meio — e o link é validado
   * (`isPlausibleVipLink`) antes de salvar, não só lá no launch.
   */
  function handleSaveVip(game: FavoriteGame) {
    const link = vipDraftLink.trim();
    if (!isPlausibleVipLink(link)) {
      setVipDraftError(t("That doesn't look like a private server link or VIP code."));
      return;
    }
    const vips = getVips(game);
    const name = vipDraftName.trim() || `${t("VIP")} ${vips.length + 1}`;
    const newVip: VipServer = { id: makeVipId(), name, link };
    persist(
      favorites.map((f) =>
        f.placeId === game.placeId
          ? { ...f, vipServers: [...getVips(f), newVip], privateServer: undefined }
          : f
      )
    );
    addToast(t("VIP server added"));
    cancelAddVip();
  }

  async function handleRemoveVip(game: FavoriteGame, vipId: string) {
    const vip = getVips(game).find((v) => v.id === vipId);
    const ok = await confirm(
      t(
        'Remove the VIP server "{{name}}" from "{{game}}"? Its link is deleted from this computer and cannot be recovered.',
        { name: vip?.name || t("VIP"), game: game.name }
      ),
      true
    );
    if (!ok) return;
    persist(
      favorites.map((f) =>
        f.placeId === game.placeId ? { ...f, vipServers: getVips(f).filter((v) => v.id !== vipId) } : f
      )
    );
    addToast(t("VIP server removed"));
  }

  if (favorites.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" className="text-zinc-800 mb-3">
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
        <p className="text-xs text-zinc-700 mb-1">{t("No favorites yet")}</p>
        <p className="text-[10px] text-zinc-700">{t("Use the star on a game in the Games or Recent tab to add one")}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="flex flex-col gap-1 px-1">
          {favorites.map((game) => {
            const vips = getVips(game);
            const expanded = expandedId === game.placeId;
            return (
              <div key={game.placeId} className="rounded-lg">
                <div
                  className="flex items-center gap-3 p-2 rounded-lg hover:bg-zinc-800/40 transition-colors cursor-pointer group"
                  onClick={() => setExpandedId(expanded ? null : game.placeId)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setContextMenu({ x: e.clientX, y: e.clientY, game });
                  }}
                >
                  <div className="w-9 h-9 rounded-md bg-zinc-800 shrink-0 overflow-hidden">
                    {game.iconUrl ? (
                      <img src={game.iconUrl} alt="" className="w-full h-full object-cover" />
                    ) : (
                      <div className="w-full h-full flex items-center justify-center">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-zinc-700">
                          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                        </svg>
                      </div>
                    )}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="text-[12px] text-zinc-200 truncate">{game.name}</div>
                    <div className="text-[10px] text-zinc-600 font-mono">{t("ID: {{id}}", { id: game.placeId })}</div>
                  </div>
                  {vips.length > 0 && (
                    <span className="text-[9px] text-amber-400/70 bg-amber-500/10 px-1.5 py-0.5 rounded shrink-0">
                      {vips.length} {t("VIP")}
                    </span>
                  )}
                  {/* Entrar continua na gaveta (é lá que estão os VIPs); o que
                      faltava na linha era o caminho para os servidores. */}
                  <GameRowActions
                    placeId={game.placeId}
                    actions={
                      onBrowseServers
                        ? [
                            {
                              key: "servers",
                              label: t("Browse servers"),
                              icon: browseServersIcon,
                              onClick: () => onBrowseServers(game.placeId),
                            },
                          ]
                        : []
                    }
                  />
                  <svg
                    width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
                    className={`text-zinc-600 shrink-0 transition-transform ${expanded ? "rotate-180" : ""}`}
                  >
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                </div>

                {expanded && (
                  <div className="ml-12 mr-1 mb-2 mt-1 flex flex-col gap-1.5">
                    <button
                      onClick={() => onSelectGame(game.placeId)}
                      className="flex items-center justify-center gap-2 w-full px-3 py-1.5 rounded-lg text-[12px] font-medium bg-sky-600 hover:bg-sky-500 text-white transition-colors"
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <polygon points="5 3 19 12 5 21 5 3" />
                      </svg>
                      {t("Join Game")}
                    </button>

                    {vips.length > 0 && (
                      <div className="flex flex-col gap-1 mt-1">
                        {vips.map((vip) => (
                          <div key={vip.id} className="flex items-center gap-2 px-2 py-1.5 rounded-lg bg-zinc-800/40">
                            <div className="flex-1 min-w-0">
                              <div className="text-[11px] text-amber-300/90 truncate">{vip.name}</div>
                              <div className="text-[9px] text-zinc-600 font-mono truncate">{vip.link}</div>
                            </div>
                            <button
                              onClick={() => onSelectGame(game.placeId, vip.link)}
                              className="px-3 py-1 rounded-md text-[11px] font-medium bg-emerald-600 hover:bg-emerald-500 text-white transition-colors shrink-0"
                            >
                              {t("Join")}
                            </button>
                            <button
                              onClick={() => {
                                void handleRemoveVip(game, vip.id);
                              }}
                              title={t("Remove")}
                              className="p-1 rounded-md text-zinc-500 hover:text-red-400 hover:bg-zinc-800 transition-colors shrink-0"
                            >
                              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                                <line x1="18" y1="6" x2="6" y2="18" />
                                <line x1="6" y1="6" x2="18" y2="18" />
                              </svg>
                            </button>
                          </div>
                        ))}
                      </div>
                    )}

                    {addingVipFor === game.placeId ? (
                      <div className="flex flex-col gap-1.5 mt-1 p-2 rounded-lg border border-dashed border-zinc-700">
                        <input
                          autoFocus
                          value={vipDraftLink}
                          onChange={(e) => {
                            setVipDraftLink(e.target.value);
                            if (vipDraftError) setVipDraftError("");
                          }}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleSaveVip(game);
                            if (e.key === "Escape") cancelAddVip();
                          }}
                          placeholder={t("Private server link or VIP code")}
                          className="sidebar-input font-mono text-xs w-full"
                        />
                        <input
                          value={vipDraftName}
                          onChange={(e) => setVipDraftName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleSaveVip(game);
                            if (e.key === "Escape") cancelAddVip();
                          }}
                          placeholder={t("Name for this server (optional)")}
                          className="sidebar-input text-xs w-full"
                        />
                        {vipDraftError && (
                          <p className="text-[10px] text-red-400">{vipDraftError}</p>
                        )}
                        <div className="flex items-center gap-2">
                          <button
                            onClick={() => handleSaveVip(game)}
                            className="flex-1 px-3 py-1 rounded-md text-[11px] font-medium bg-emerald-600 hover:bg-emerald-500 text-white transition-colors"
                          >
                            {t("Save")}
                          </button>
                          <button
                            onClick={cancelAddVip}
                            className="flex-1 px-3 py-1 rounded-md text-[11px] font-medium bg-zinc-800 hover:bg-zinc-700 text-zinc-300 transition-colors"
                          >
                            {t("Cancel")}
                          </button>
                        </div>
                      </div>
                    ) : (
                      <button
                        onClick={() => startAddVip(game.placeId)}
                        className="flex items-center justify-center gap-1.5 w-full px-3 py-1.5 rounded-lg text-[11px] font-medium text-zinc-400 border border-dashed border-zinc-700 hover:border-zinc-600 hover:text-zinc-300 transition-colors mt-1"
                      >
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <line x1="12" y1="5" x2="12" y2="19" />
                          <line x1="5" y1="12" x2="19" y2="12" />
                        </svg>
                        {t("Add VIP Server")}
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {contextMenu && (
        <FavoriteContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onJoin={() => onSelectGame(contextMenu.game.placeId)}
          onRename={() => handleRename(contextMenu.game)}
          onRemove={() => {
            void handleRemove(contextMenu.game);
          }}
          onCopyPlaceId={() => {
            navigator.clipboard.writeText(String(contextMenu.game.placeId));
            addToast(t("Copied Place ID"));
          }}
          onBrowseServers={
            onBrowseServers ? () => onBrowseServers(contextMenu.game.placeId) : undefined
          }
          onBotting={onBotting ? () => onBotting(contextMenu.game.placeId) : undefined}
          onScripts={onScripts ? () => onScripts(contextMenu.game.placeId) : undefined}
        />
      )}
    </div>
  );
}
