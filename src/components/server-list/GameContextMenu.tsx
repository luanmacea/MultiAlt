import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { GameEntry } from "./types";
import { useTr } from "../../i18n/text";

export function GameContextMenu({
  x,
  y,
  onClose,
  onJoin,
  onFavorite,
  onCopyPlaceId,
}: {
  x: number;
  y: number;
  game: GameEntry;
  onClose: () => void;
  onJoin: () => void;
  onFavorite: () => void;
  onCopyPlaceId: () => void;
}) {
  const t = useTr();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    }
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
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

  return createPortal(
    <div
      ref={ref}
      className="theme-modal-scope theme-panel theme-border fixed z-[60] bg-zinc-900/98 border border-zinc-700/60 rounded-xl shadow-2xl py-1 w-44 backdrop-blur-xl animate-scale-in"
      style={{ top: pos.top, left: pos.left }}
    >
      <button
        onClick={() => { onJoin(); onClose(); }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-800 text-left"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-emerald-400">
          <polygon points="5 3 19 12 5 21 5 3" />
        </svg>
        {t("Join Game")}
      </button>
      <button
        onClick={() => { onFavorite(); onClose(); }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-800 text-left"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-amber-400">
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
        {t("Favorite")}
      </button>
      <div className="h-px bg-zinc-800 my-0.5" />
      <button
        onClick={() => { onCopyPlaceId(); onClose(); }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-800 text-left"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-zinc-500">
          <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
        {t("Copy Place ID")}
      </button>
    </div>
    ,
    document.body
  );
}
