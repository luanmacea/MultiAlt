import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useEscapeStack } from "../../hooks/useEscapeStack";
import { createPortal } from "react-dom";
import { Code2, Repeat, Server } from "lucide-react";
import type { GameEntry } from "./types";
import { useTr } from "../../i18n/text";

/**
 * Um item do menu. O `title` leva o nome do jogo: a ação age sobre ele.
 *
 * Fica **fora** de `GameContextMenu` de propósito. Declarado dentro do corpo,
 * era um tipo novo a cada render, e o React desmontava e remontava os botões —
 * as telas donas do menu leem a store inteira, que se atualiza sozinha a cada
 * poucos segundos, então com o menu aberto e parado os botões eram recriados
 * (24 remoções em 8 s no harness): clique perdido na troca, hover piscando.
 */
function MenuItem({
  icon,
  label,
  gameName,
  onPick,
  onClose,
}: {
  icon: ReactNode;
  label: string;
  gameName: string;
  onPick: () => void;
  onClose: () => void;
}) {
  return (
    <button
      onClick={() => {
        onPick();
        onClose();
      }}
      title={gameName ? `${label} — ${gameName}` : label}
      className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-800 text-left"
    >
      {icon}
      {label}
    </button>
  );
}

/**
 * Menu do jogo: tudo que se pode fazer **com aquele jogo**, a partir da lista
 * onde ele aparece.
 *
 * O motivo de existir as ações novas: funcionalidades como Auto Rejoin só
 * podiam ser usadas abrindo a tela delas e colando o Place ID à mão. Aqui a
 * ação já sabe de que jogo se trata.
 *
 * Ação sem callback **não aparece**: o diálogo antigo (Server List) não tem
 * para onde abrir o Auto Rejoin, e item morto é pior que item ausente.
 */
export function GameContextMenu({
  x,
  y,
  game,
  onClose,
  onJoin,
  onFavorite,
  onCopyPlaceId,
  onBrowseServers,
  onBotting,
  onScripts,
}: {
  x: number;
  y: number;
  game: GameEntry;
  onClose: () => void;
  onJoin: () => void;
  onFavorite: () => void;
  onCopyPlaceId: () => void;
  onBrowseServers?: () => void;
  onBotting?: () => void;
  onScripts?: () => void;
}) {
  const t = useTr();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  // O Escape vem pela pilha: este popover monta depois do diálogo que o contém,
  // então fica no topo e o Escape fecha só ele.
  useEscapeStack(true, onClose);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    }
    document.addEventListener("mousedown", handleClick);
    return () => {
      document.removeEventListener("mousedown", handleClick);
    };
  }, [onClose]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const pad = 8;
    const width = el.offsetWidth || 176;
    const height = el.offsetHeight || 132;
    const left = Math.max(pad, Math.min(x, window.innerWidth - width - pad));
    const top = Math.max(pad, Math.min(y, window.innerHeight - height - pad));
    setPos({ left, top });
  }, [x, y]);

  /** Liga um item ao jogo e ao fechamento do menu. */
  const itemProps = { gameName: game.name, onClose };

  return createPortal(
    <div
      ref={ref}
      data-testid="game-context-menu"
      className="theme-modal-scope theme-panel theme-border fixed z-[60] bg-zinc-900/98 border border-zinc-700/60 rounded-xl shadow-2xl py-1 w-48 backdrop-blur-xl animate-scale-in"
      style={{ top: pos.top, left: pos.left }}
    >
      <MenuItem
        {...itemProps}
        onPick={onJoin}
        label={t("Join Game")}
        icon={
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-emerald-400">
            <polygon points="5 3 19 12 5 21 5 3" />
          </svg>
        }
      />
      {onBrowseServers && (
        <MenuItem
          {...itemProps}
          onPick={onBrowseServers}
          label={t("Browse servers")}
          icon={<Server size={12} strokeWidth={2} className="text-sky-400" />}
        />
      )}
      <MenuItem
        {...itemProps}
        onPick={onFavorite}
        label={t("Favorite")}
        icon={
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-amber-400">
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
        }
      />
      {(onBotting || onScripts) && <div className="h-px bg-zinc-800 my-0.5" />}
      {onBotting && (
        <MenuItem
          {...itemProps}
          onPick={onBotting}
          label={t("Auto Rejoin")}
          icon={<Repeat size={12} strokeWidth={2} className="text-violet-400" />}
        />
      )}
      {onScripts && (
        <MenuItem
          {...itemProps}
          onPick={onScripts}
          label={t("Scripts")}
          icon={<Code2 size={12} strokeWidth={2} className="text-zinc-400" />}
        />
      )}
      <div className="h-px bg-zinc-800 my-0.5" />
      <MenuItem
        {...itemProps}
        onPick={onCopyPlaceId}
        label={t("Copy Place ID")}
        icon={
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-zinc-500">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        }
      />
    </div>,
    document.body
  );
}
