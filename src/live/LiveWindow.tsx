import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTr } from "../i18n/text";
import { LIVE_GRID_DEFAULTS, liveGrid, toPhysical } from "./liveGrid";

/**
 * A janela "MultiAlt Live" (`?view=live`): as contas marcadas na página Live,
 * numa grade que sempre usa o espaço todo. Esta tela só desenha a moldura e o
 * nome; a imagem de cada conta é uma miniatura ao vivo do Windows (DWM), posta
 * por cima de cada quadro pelo backend (`live_set_layout`). Ver
 * docs/features/live.md.
 */

export interface LiveAccount {
  userId: number;
  label: string;
  inGame: boolean;
  selected: boolean;
}

const POLL_MS = 2000;

const BACKGROUNDS: Record<string, string> = {
  dark: "#0b0b0d",
  black: "#000000",
  green: "#00b140",
};

function useWindowSize() {
  const [size, setSize] = useState({ width: window.innerWidth, height: window.innerHeight });
  useEffect(() => {
    const onResize = () => setSize({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return size;
}

export function LiveWindow() {
  const t = useTr();
  const [tiles, setTiles] = useState<LiveAccount[]>([]);
  const [settings, setSettings] = useState<Record<string, string>>({});
  const size = useWindowSize();
  const sent = useRef("");

  const refresh = useCallback(async () => {
    try {
      const [next, all] = await Promise.all([
        invoke<LiveAccount[]>("live_tiles"),
        invoke<Record<string, Record<string, string>>>("get_all_settings"),
      ]);
      setTiles(next);
      setSettings(all?.Live ?? {});
    } catch {
      // A janela fecha junto com o app; uma passada perdida não importa.
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_MS);
    const unlisten = listen("live-changed", () => void refresh());
    return () => {
      window.clearInterval(timer);
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const showNames = settings.ShowNames !== "false";
  const background = BACKGROUNDS[settings.Background ?? "dark"] ?? BACKGROUNDS.dark;
  const cells = useMemo(
    () =>
      liveGrid(tiles.length, size.width, size.height, {
        ...LIVE_GRID_DEFAULTS,
        labelHeight: showNames ? LIVE_GRID_DEFAULTS.labelHeight : 0,
      }),
    [tiles.length, size.width, size.height, showNames]
  );

  // A grade em pixels físicos vai para o backend, que põe cada miniatura no
  // quadro dela. Só manda quando mudou.
  useEffect(() => {
    const scale = window.devicePixelRatio || 1;
    const payload = tiles.map((tile, i) => ({ userId: tile.userId, ...toPhysical(cells[i].frame, scale) }));
    const key = JSON.stringify(payload);
    if (key === sent.current) return;
    sent.current = key;
    void invoke("live_set_layout", { cells: payload }).catch(() => {
      sent.current = "";
    });
  }, [tiles, cells]);

  return (
    <div
      className="fixed inset-0 overflow-hidden select-none"
      style={{ background, fontFamily: "Outfit, system-ui, sans-serif" }}
    >
      {tiles.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center px-8 text-center text-[14px] text-zinc-500">
          {t("No account to show. Open the accounts in MultiAlt and pick them in the Live page.")}
        </div>
      )}
      {tiles.map((tile, i) => {
        const cell = cells[i];
        if (!cell) return null;
        return (
          <div key={tile.userId}>
            {showNames && (
              <div
                className="absolute flex items-center gap-1.5 truncate px-1 text-[12px] text-zinc-300"
                style={{ left: cell.label.x, top: cell.label.y, width: cell.label.width, height: cell.label.height }}
              >
                <span
                  className={`inline-block h-2 w-2 shrink-0 rounded-full ${tile.inGame ? "bg-emerald-400" : "bg-amber-400"}`}
                  aria-hidden="true"
                />
                <span className="truncate">{tile.label}</span>
              </div>
            )}
            <button
              type="button"
              title={t("Click to play on this account")}
              aria-label={tile.label ? `${t("Play on")} ${tile.label}` : t("Click to play on this account")}
              onClick={() => void invoke("live_focus_account", { userId: tile.userId }).catch(() => {})}
              className="absolute rounded-sm bg-black/40 outline-none"
              style={{ left: cell.frame.x, top: cell.frame.y, width: cell.frame.width, height: cell.frame.height }}
            />
          </div>
        );
      })}
    </div>
  );
}
