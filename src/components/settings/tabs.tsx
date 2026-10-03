import type { ReactNode } from "react";
import { Settings as SettingsIcon, Code, Gauge, Server, Eye, Sparkles, ShieldCheck, Package, MoreHorizontal, DatabaseBackup } from "lucide-react";
import { ENABLE_ACCOUNT_GENERATOR, ENABLE_WEBSERVER } from "../../featureFlags";

/** Seções da página Settings (eram as abas do antigo SettingsDialog). */
export type TabId =
  | "general"
  | "backups"
  | "developer"
  | "webserver"
  | "watcher"
  | "generator"
  | "isolation"
  | "versions"
  | "optimization"
  | "miscellaneous";

const ALL_TABS: TabId[] = [
  "general",
  "backups",
  "developer",
  "webserver",
  "watcher",
  "generator",
  "isolation",
  "versions",
  "optimization",
  "miscellaneous",
];

/**
 * Seções montadas pela página, na ordem. As desligadas por build ficam de fora
 * (nem montam escondidas): o WebServer sem `ENABLE_WEBSERVER` e o gerador pago
 * sem `ENABLE_ACCOUNT_GENERATOR`.
 */
export const TAB_ORDER: TabId[] = ALL_TABS.filter(
  (tab) =>
    (tab !== "webserver" || ENABLE_WEBSERVER) && (tab !== "generator" || ENABLE_ACCOUNT_GENERATOR)
);

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
  // Logo depois de General: backup é o que salva o usuário quando algo dá
  // errado, e como diálogo aberto por "Manage" em Misc ninguém o achava.
  { id: "backups", label: "Backups", icon: <DatabaseBackup {...ICON} /> },
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
    // Escondida com o gerador pago desligado — ver `ENABLE_ACCOUNT_GENERATOR`.
    hidden: !ENABLE_ACCOUNT_GENERATOR,
  },
  { id: "isolation", label: "Isolation", icon: <ShieldCheck {...ICON} /> },
  { id: "versions", label: "Versions", icon: <Package {...ICON} /> },
  { id: "optimization", label: "Optimization", icon: <Gauge {...ICON} /> },
  { id: "miscellaneous", label: "Misc", icon: <MoreHorizontal {...ICON} /> },
];
