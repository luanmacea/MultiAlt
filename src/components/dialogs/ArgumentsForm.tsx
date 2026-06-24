import { useState, useEffect, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";

export function ArgumentsForm({
  open,
  onClose,
  anchorRef,
}: {
  open: boolean;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLButtonElement | null>;
}) {
  const t = useTr();
  const store = useStore();
  const [isTeleport, setIsTeleport] = useState(false);
  const [useOldJoin, setUseOldJoin] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{
    top: number;
    left: number;
    maxHeight: number;
    transformOrigin: string;
  } | null>(null);

  useEffect(() => {
    if (!open) return;
    const s = store.settings;
    if (s?.Developer) {
      setIsTeleport(s.Developer.IsTeleport === "true");
      setUseOldJoin(s.Developer.UseOldJoin === "true");
    }
    function handleClick(e: MouseEvent) {
      if (
        panelRef.current &&
        !panelRef.current.contains(e.target as Node) &&
        anchorRef.current &&
        !anchorRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
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
  }, [open]);

  useLayoutEffect(() => {
    if (!open) return;

    let raf = 0;
    function compute() {
      const anchor = anchorRef.current;
      const panel = panelRef.current;
      if (!anchor || !panel) return;

      const gap = 8;
      const margin = 10;

      const a = anchor.getBoundingClientRect();
      const p = panel.getBoundingClientRect();
      const panelW = p.width || 280;
      const panelH = p.height || 0;

      // Align the panel to the right edge of the anchor button by default.
      const preferredLeft = a.right - panelW;
      const left = Math.min(window.innerWidth - margin - panelW, Math.max(margin, preferredLeft));

      const belowSpace = window.innerHeight - margin - (a.bottom + gap);
      const aboveSpace = (a.top - gap) - margin;

      const openBelow = belowSpace >= panelH || belowSpace >= aboveSpace;
      const top = openBelow
        ? Math.min(window.innerHeight - margin - panelH, a.bottom + gap)
        : Math.max(margin, a.top - gap - panelH);

      const maxHeight = openBelow
        ? Math.max(160, window.innerHeight - margin - top)
        : Math.max(160, a.top - gap - margin);

      setPos({
        top,
        left,
        maxHeight,
        transformOrigin: openBelow ? "top right" : "bottom right",
      });
    }

    function schedule() {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(compute);
    }

    // Initial placement, then again next frame once layout settles.
    schedule();
    const t1 = window.setTimeout(schedule, 0);

    window.addEventListener("resize", schedule);
    // Capture scrolls from any scroll container so the fixed panel tracks the anchor.
    window.addEventListener("scroll", schedule, true);

    return () => {
      window.clearTimeout(t1);
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", schedule);
      window.removeEventListener("scroll", schedule, true);
    };
  }, [open, anchorRef]);

  if (!open) return null;

  function toggle(key: string, val: boolean, setter: (v: boolean) => void) {
    setter(val);
    invoke("update_setting", { section: "Developer", key, value: String(val) }).catch(() => {});
  }

  const panel = (
    <div
      ref={panelRef}
      className="theme-modal-scope theme-panel theme-border fixed w-[280px] bg-zinc-900 border border-zinc-700/60 rounded-xl shadow-2xl z-[999] animate-scale-in p-3 space-y-2.5 overflow-y-auto"
      style={
        pos
          ? { top: pos.top, left: pos.left, maxHeight: pos.maxHeight, transformOrigin: pos.transformOrigin }
          : { top: 0, left: 0, transformOrigin: "top right" }
      }
    >
      <div className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
        {t("Launch Arguments")}
      </div>

      <label className="flex items-center gap-2 cursor-pointer">
        <input
          type="checkbox"
          checked={isTeleport}
          onChange={(e) => toggle("IsTeleport", e.target.checked, setIsTeleport)}
          className="w-3.5 h-3.5 rounded border-zinc-600 bg-zinc-800 text-sky-500 focus:ring-0 focus:ring-offset-0"
        />
        <span className="text-xs text-zinc-300">{t("Is Teleport")}</span>
      </label>

      <label className="flex items-center gap-2 cursor-pointer">
        <input
          type="checkbox"
          checked={useOldJoin}
          onChange={(e) => toggle("UseOldJoin", e.target.checked, setUseOldJoin)}
          className="w-3.5 h-3.5 rounded border-zinc-600 bg-zinc-800 text-sky-500 focus:ring-0 focus:ring-offset-0"
        />
        <span className="text-xs text-zinc-300">{t("Use Old Join Method")}</span>
      </label>

    </div>
  );

  // Render as a portal to avoid clipping/stacking-context issues inside the scrollable sidebar.
  return createPortal(panel, document.body);
}
