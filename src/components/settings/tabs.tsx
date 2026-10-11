import type { ReactNode } from "react";
import {
  AppWindow,
  Code,
  DatabaseBackup,
  Download,
  Eye,
  Gauge,
  Lock,
  Package,
  Repeat,
  Rocket,
  Server,
  ShieldCheck,
  Sparkles,
  Users,
} from "lucide-react";
import { ENABLE_ACCOUNT_GENERATOR, ENABLE_WEBSERVER } from "../../featureFlags";

/**
 * Seções da página Settings (eram as abas do antigo SettingsDialog).
 *
 * General e Misc viraram as seis primeiras em 11/10/2026: General era um
 * paredão de ~40 opções sem ordem. Cada controle mudou só de lugar — a chave
 * do INI é a mesma (ver docs/features/settings.md).
 */
export type TabId =
  | "launch"
  | "keep-in-game"
  | "accounts"
  | "privacy"
  | "updates"
  | "app"
  | "backups"
  | "developer"
  | "webserver"
  | "watcher"
  | "generator"
  | "isolation"
  | "versions"
  | "optimization";

const ALL_TABS: TabId[] = [
  "launch",
  "keep-in-game",
  "accounts",
  "privacy",
  "updates",
  "app",
  "backups",
  "developer",
  "webserver",
  "watcher",
  "generator",
  "isolation",
  "versions",
  "optimization",
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
  /**
   * As seções técnicas ficam num grupo "Advanced" abaixo das do dia a dia,
   * separadas na barra — a lista tem 13 itens e sem grupo nada se destacava.
   */
  advanced?: boolean;
}

const ICON = { size: 15, strokeWidth: 1.6 } as const;

export const SETTINGS_TABS: TabDef[] = [
  // Nomes em sentence case, como no resto do app.
  { id: "launch", label: "Launch", icon: <Rocket {...ICON} /> },
  { id: "keep-in-game", label: "Keep in game", icon: <Repeat {...ICON} /> },
  { id: "accounts", label: "Accounts", icon: <Users {...ICON} /> },
  { id: "privacy", label: "Privacy & security", icon: <Lock {...ICON} /> },
  { id: "updates", label: "Updates", icon: <Download {...ICON} /> },
  { id: "app", label: "App", icon: <AppWindow {...ICON} /> },
  // Fecha o grupo do dia a dia: backup é o que salva o usuário quando algo dá
  // errado, e como diálogo aberto por "Manage" na antiga Misc ninguém o achava.
  { id: "backups", label: "Backups", icon: <DatabaseBackup {...ICON} /> },
  { id: "developer", label: "Developer", icon: <Code {...ICON} />, advanced: true },
  {
    id: "webserver",
    label: "WebServer",
    icon: <Server {...ICON} />,
    advanced: true,
    // A seção fica visível mesmo trancada: escondê-la fazia a API HTTP local
    // depender do usuário descobrir sozinho o Developer Mode. Quem guarda o
    // acesso é o conteúdo (WebServerTab), não a ausência da seção.
    hidden: !ENABLE_WEBSERVER,
  },
  { id: "watcher", label: "Watcher", icon: <Eye {...ICON} />, advanced: true },
  {
    id: "generator",
    // `Account Generator` é o nome da função paga em toda a interface (menu
    // Add, AddAccountDialog, GeneratorDialog). "Generator" sozinho não dizia
    // qual das duas formas de criar conta era esta.
    label: "Account Generator",
    icon: <Sparkles {...ICON} />,
    advanced: true,
    // Escondida com o gerador pago desligado — ver `ENABLE_ACCOUNT_GENERATOR`.
    hidden: !ENABLE_ACCOUNT_GENERATOR,
  },
  { id: "isolation", label: "Isolation", icon: <ShieldCheck {...ICON} />, advanced: true },
  { id: "versions", label: "Versions", icon: <Package {...ICON} />, advanced: true },
  { id: "optimization", label: "Optimization", icon: <Gauge {...ICON} />, advanced: true },
];
