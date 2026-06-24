import { useState, useRef, useLayoutEffect } from "react";
import type { TabId } from "./types";
import { useTr } from "../../i18n/text";
import { Server, Gamepad2, Star, Clock } from "lucide-react";

export interface TabBarProps {
  activeTab: TabId;
  onTabChange: (id: TabId) => void;
}

export function TabBar({
  activeTab,
  onTabChange,
}: TabBarProps) {
  const t = useTr();
  const tabs: { id: TabId; label: string; icon: React.ReactNode }[] = [
    {
      id: "servers",
      label: "Servers",
      icon: <Server size={14} strokeWidth={1.5} />,
    },
    {
      id: "games",
      label: "Games",
      icon: <Gamepad2 size={14} strokeWidth={1.5} />,
    },
    {
      id: "favorites",
      label: "Favorites",
      icon: <Star size={14} strokeWidth={1.5} />,
    },
    {
      id: "recent",
      label: "Recent",
      icon: <Clock size={14} strokeWidth={1.5} />,
    },
  ];

  const tabRefs = useRef<Map<TabId, HTMLButtonElement>>(new Map());
  const barRef = useRef<HTMLDivElement>(null);
  const [pillStyle, setPillStyle] = useState({ left: 0, width: 0 });
  const hasInitialized = useRef(false);

  useLayoutEffect(() => {
    const el = tabRefs.current.get(activeTab);
    const bar = barRef.current;
    if (!el || !bar) return;
    const barRect = bar.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    setPillStyle({
      left: elRect.left - barRect.left,
      width: elRect.width,
    });
    hasInitialized.current = true;
  }, [activeTab]);

  return (
    <div ref={barRef} className="relative flex gap-1 px-5 pb-0 shrink-0">
      <div
        className="absolute top-0 h-full rounded-lg bg-zinc-800 shadow-sm"
        style={{
          left: pillStyle.left,
          width: pillStyle.width,
          transition: hasInitialized.current
            ? "left 180ms cubic-bezier(0.22, 1, 0.36, 1), width 150ms cubic-bezier(0.22, 1, 0.36, 1)"
            : "none",
        }}
      />
      {tabs.map((tab) => (
        <button
          key={tab.id}
          ref={(el) => {
            if (el) tabRefs.current.set(tab.id, el);
          }}
          onClick={() => onTabChange(tab.id)}
          className={`relative z-[1] flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[12px] font-medium transition-colors duration-200 ${
            activeTab === tab.id
              ? "text-zinc-100"
              : "text-zinc-500 hover:text-zinc-300"
          }`}
        >
          <span
            className="transition-colors duration-200"
            style={{ color: activeTab === tab.id ? "rgb(56, 189, 248)" : undefined }}
          >
            {tab.icon}
          </span>
          {t(tab.label)}
        </button>
      ))}
    </div>
  );
}
