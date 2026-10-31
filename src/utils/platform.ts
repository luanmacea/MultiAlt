import type { PlatformCapabilities } from "../types";

export function isWindowsPlatform(capabilities: PlatformCapabilities | null): boolean {
  const platform = capabilities?.os || "";
  return (
    platform === "windows" ||
    (platform === "" &&
      typeof navigator !== "undefined" &&
      navigator.userAgent.toLowerCase().includes("windows"))
  );
}

/** Linux de verdade (o backend reporta `os`); sem capacidades ainda, não é. */
export function isLinuxPlatform(capabilities: PlatformCapabilities | null): boolean {
  return capabilities?.os === "linux";
}

export interface PlatformFeatures {
  /** Página Live (miniaturas do DWM). */
  liveView: boolean;
  /** Tecla/clique periódico e gravações (SendInput). */
  afkInput: boolean;
  /** Página AFK Mode: existe se houver AFK por tecla/clique **ou** Auto Rejoin. */
  afkPage: boolean;
  /** Settings › Isolation. */
  isolation: boolean;
  /** Travas do Multi Roblox (mutex/evento) em Settings › Launch. */
  multiRobloxMutex: boolean;
  /** Settings › Versions. */
  versions: boolean;
  /** Settings › Optimization. */
  optimization: boolean;
  /** Patch de settings/FastFlags do cliente. */
  clientSettings: boolean;
  /** Clientes abertos pelo site, adotados pela varredura do log. */
  externalClients: boolean;
  /** Modo Python da página Scripts. */
  python: boolean;
  /** Settings › Watcher. */
  watcher: boolean;
  /** Settings › Linux (só lá). */
  linuxSection: boolean;
}

/**
 * O que a UI mostra neste SO. Regra única: **campo ausente conta como
 * suportado** (`!== false`), então Windows, macOS, backend antigo e o harness
 * antes de as capacidades chegarem seguem vendo tudo; só o Linux manda `false`
 * (ver `build_platform_capabilities` em commands/platform_info.rs).
 * O que vem `false` é **escondido**, não desabilitado.
 */
export function platformFeatures(capabilities: PlatformCapabilities | null): PlatformFeatures {
  const on = (value: boolean | undefined) => value !== false;
  const afkInput = on(capabilities?.supportsAfkInput);
  return {
    liveView: on(capabilities?.supportsLiveView),
    afkInput,
    afkPage: afkInput || on(capabilities?.supportsBotting),
    isolation: on(capabilities?.supportsIsolation),
    multiRobloxMutex: on(capabilities?.supportsMultiRobloxMutex),
    versions: on(capabilities?.supportsVersions),
    optimization: on(capabilities?.supportsOptimization),
    clientSettings: on(capabilities?.supportsClientSettings),
    externalClients: on(capabilities?.supportsExternalClients),
    python: on(capabilities?.supportsPython),
    watcher: on(capabilities?.supportsWatcher),
    linuxSection: isLinuxPlatform(capabilities),
  };
}
