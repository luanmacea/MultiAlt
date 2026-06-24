import { useState, useEffect, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Minus, Square, Copy, X, Github } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";
import { useTr } from "../../i18n/text";

export function TitleBar({ controlsHidden = false }: { controlsHidden?: boolean }) {
  const t = useTr();
  const repoUrl = "https://github.com/niccsprojects/Roblox-Account-Manager";
  const [maximized, setMaximized] = useState(false);
  const [minimizeToTray, setMinimizeToTray] = useState(false);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    appWindow.isMaximized().then(setMaximized);
    const unlisten = appWindow.onResized(() => {
      appWindow.isMaximized().then(setMaximized);
    });
    invoke<string>("get_setting", { section: "General", key: "MinimizeToTray" })
      .then((v) => setMinimizeToTray(v === "true"))
      .catch(() => {});
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const handleClose = useCallback(() => {
    if (minimizeToTray) {
      appWindow.hide();
    } else {
      appWindow.close();
    }
  }, [minimizeToTray]);

  return (
    <div className="theme-titlebar theme-border flex items-center h-9 shrink-0 select-none border-b">
      <div className="flex items-center gap-2.5 pl-3.5 pr-3 shrink-0">
        <span className="text-[12px] font-medium tracking-tight">
          {t("Roblox Account Manager")}
        </span>
      </div>

      <div
        className="flex-1 h-full"
        onMouseDown={() => appWindow.startDragging()}
      />

      <div
        className={[
          "flex items-center shrink-0 overflow-hidden transition-all duration-250 ease-out",
          controlsHidden
            ? "max-w-0 opacity-0 scale-95 -translate-y-1 pointer-events-none"
            : "max-w-[176px] opacity-100 scale-100 translate-y-0",
        ].join(" ")}
      >
        <Tooltip content={t("Open GitHub repository")} side="bottom" delayMs={500}>
          <button
            onClick={() => {
              void invoke("open_repo_url").catch(() => {
                window.open(repoUrl, "_blank");
              });
            }}
            className="h-9 w-11 flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] transition-colors"
          >
            <Github size={12} strokeWidth={1.6} />
          </button>
        </Tooltip>
        <Tooltip content={t("Minimize")} side="bottom" delayMs={500}>
          <button
            onClick={() => appWindow.minimize()}
            className="h-9 w-11 flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] transition-colors"
          >
            <Minus size={12} strokeWidth={1} />
          </button>
        </Tooltip>
        <Tooltip content={maximized ? t("Restore") : t("Maximize")} side="bottom" delayMs={500}>
          <button
            onClick={() => appWindow.toggleMaximize()}
            className="h-9 w-11 flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] transition-colors"
          >
            {maximized ? (
              <Copy size={12} strokeWidth={1} />
            ) : (
              <Square size={12} strokeWidth={1} />
            )}
          </button>
        </Tooltip>
        <Tooltip content={minimizeToTray ? t("Minimize to tray") : t("Close")} side="bottom" delayMs={500}>
          <button
            onClick={handleClose}
            className="h-9 w-11 flex items-center justify-center theme-muted hover:text-white hover:bg-red-600/80 transition-colors rounded-tr-none"
          >
            <X size={12} strokeWidth={1.2} />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
