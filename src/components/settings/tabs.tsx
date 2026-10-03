import type { ReactNode } from "react";
import { Settings as SettingsIcon, Code, Gauge, Server, Eye, Sparkles, ShieldCheck, Package, MoreHorizontal } from "lucide-react";
import { ENABLE_WEBSERVER } from "../../featureFlags";

/** Seções da página Settings (eram as abas do antigo SettingsDialog). */
export type TabId =
  | "general"
  | "developer"
  | "webserver"
  | "watcher"
  | "generator"
  | "isolation"
  | "versions"
  | "optimization"
  | "miscellaneous";

export const TAB_ORDER: TabId[] = ENABLE_WEBSERVER
  ? ["general", "developer", "webserver", "watcher", "generator", "isolation", "versions", "optimization", "miscellaneous"]
  : ["general", "developer", "watcher", "generator", "isolation", "versions", "optimization", "miscellaneous"];

export interface TabDef {
  id: TabId;
  /** Chave de tradução (inglês). */
  label: string;
  icon: ReactNode;
  hidden?: boolean;
}

const ICON = { size: 15, strokeWidth: 1.6 } as const;

export const SETTINGS_TABS: TabDef[] = [
  { id: "general", label: "General", icon: <SettingsIcon {...ICON} /> },
  { id: "developer", label: "Developer", icon: <Code {...ICON} /> },
  {
    id: "webserver",
    label: "WebServer",
    icon: <Server {...ICON} />,
    // A seção fica visível mesmo trancada: escondê-la fazia a API HTTP local
    // depender do usuário descobrir sozinho o Developer Mode. Quem guarda o
    // acesso é o conteúdo (WebServerTab), não a ausência da seção.
    hidden: !ENABLE_WEBSERVER,
  },
  { id: "watcher", label: "Watcher", icon: <Eye {...ICON} /> },
  {
    id: "generator",
    // `Account Generator` é o nome da função paga em toda a interface (menu
    // Add, AddAccountDialog, GeneratorDialog). "Generator" sozinho não dizia
    // qual das duas formas de criar conta era esta.
    label: "Account Generator",
    icon: <Sparkles {...ICON} />,
  },
  { id: "isolation", label: "Isolation", icon: <ShieldCheck {...ICON} /> },
  { id: "versions", label: "Versions", icon: <Package {...ICON} /> },
  { id: "optimization", label: "Optimization", icon: <Gauge {...ICON} /> },
  { id: "miscellaneous", label: "Misc", icon: <MoreHorizontal {...ICON} /> },
];
