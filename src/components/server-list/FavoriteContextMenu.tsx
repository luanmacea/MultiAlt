import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTr } from "../../i18n/text";

export function FavoriteContextMenu({
  x,
  y,
  onClose,
  onJoin,
  onSetPrivateServer,
  hasPrivateServer,
  onRename,
  onRemove,
  onCopyPlaceId,
}: {
  x: number;
  y: number;
  onClose: () => void;
  onJoin: () => void;
  onSetPrivateServer: () => void;
  hasPrivateServer: boolean;
  onRename: () => void;
  onRemove: () => void;
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
    const height = el.offsetHeight || 160;
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
        onClick={() => { onRename(); onClose(); }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-800 text-left"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-zinc-500">
          <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
          <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
        </svg>
        {t("Rename")}
      </button>
      <button
        onClick={() => { onSetPrivateServer(); onClose(); }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-800 text-left"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="text-amber-400">
          <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
          <line x1="8" y1="21" x2="16" y2="21" />
          <line x1="12" y1="17" x2="12" y2="21" />
        </svg>
        {hasPrivateServer ? t("Edit Private Server") : t("Add Private Server")}
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
      <button
        onClick={() => { onRemove(); onClose(); }}
        className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[12px] text-red-400 hover:bg-zinc-800 text-left"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="3 6 5 6 21 6" />
          <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
        </svg>
        {t("Remove")}
      </button>
    </div>
    ,
    document.body
  );
}
