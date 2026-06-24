import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Minus, Square, Copy, X } from "lucide-react";
import { useTr } from "../../i18n/text";

export function ModalWindowControls({ visible }: { visible: boolean }) {
  const t = useTr();
  const appWindow = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);
  const [minimizeToTray, setMinimizeToTray] = useState(false);

  useEffect(() => {
    if (!visible) return;
    appWindow.isMaximized().then(setMaximized).catch(() => {});
    const unlisten = appWindow.onResized(() => {
      appWindow.isMaximized().then(setMaximized).catch(() => {});
    });
    invoke<string>("get_setting", { section: "General", key: "MinimizeToTray" })
      .then((v) => setMinimizeToTray(v === "true"))
      .catch(() => {});
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [appWindow, visible]);

  const handleClose = useCallback(() => {
    if (minimizeToTray) {
      appWindow.hide().catch(() => {});
    } else {
      appWindow.close().catch(() => {});
    }
  }, [appWindow, minimizeToTray]);

  return (
    <div
      className={[
        "fixed top-2.5 right-3 z-[140] flex items-center gap-1 rounded-xl border theme-border",
        "bg-[color:var(--panel-bg)]/96 px-1.5 py-1 backdrop-blur-md",
        "transition-all duration-250 ease-out",
        visible
          ? "opacity-100 translate-y-0 scale-100 pointer-events-auto shadow-[0_12px_34px_rgba(0,0,0,0.45)]"
          : "opacity-0 -translate-y-2 scale-95 pointer-events-none shadow-none",
      ].join(" ")}
    >
      <button
        type="button"
        title={t("Minimize")}
        onClick={() => appWindow.minimize().catch(() => {})}
        className="h-7 w-7 rounded-md flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] transition-colors"
      >
        <Minus size={11} strokeWidth={1.4} />
      </button>
      <button
        type="button"
        title={maximized ? t("Restore") : t("Maximize")}
        onClick={() => appWindow.toggleMaximize().catch(() => {})}
        className="h-7 w-7 rounded-md flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] transition-colors"
      >
        {maximized ? <Copy size={10} strokeWidth={1.4} /> : <Square size={10} strokeWidth={1.4} />}
      </button>
      <button
        type="button"
        title={minimizeToTray ? t("Minimize to tray") : t("Close")}
        onClick={handleClose}
        className="h-7 w-7 rounded-md flex items-center justify-center theme-muted hover:text-white hover:bg-red-600/80 transition-colors"
      >
        <X size={11} strokeWidth={1.6} />
      </button>
    </div>
  );
}
