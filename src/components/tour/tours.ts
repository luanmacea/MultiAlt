/**
 * Os tutoriais de cada tela, como dados.
 *
 * Regras (ver docs/features/ui-layout.md, "Tutoriais"):
 * - Frases curtas e palavras simples: quem lê quase nunca é técnico.
 * - `title`/`description` são chaves de tradução em inglês (o extrator de
 *   `scripts/i18n/extract-keys.ts` lê os campos com esses nomes).
 * - `targets`: seletores em ordem de preferência — vale o primeiro que estiver
 *   na tela. Nenhum achado: o passo aparece no centro, sem destaque. O tutorial
 *   nunca quebra por falta de alvo.
 * - `reveal`: antes do passo, clica neste controle para mostrar o alvo. **Só
 *   aba ou seção** (navegação dentro da tela). Nenhum passo lança conta, salva,
 *   apaga ou muda dado — os testes de cada página conferem.
 * - `root`: o que prova que a tela está aberta. Some (a pessoa saiu da tela), o
 *   tutorial fecha.
 */
export type TourId =
  | "accounts"
  | "choose-game"
  | "session"
  | "afk"
  | "avatars"
  | "groups"
  | "scripts"
  | "theme"
  | "settings"
  | "changelog";

export interface TourStep {
  id: string;
  title: string;
  description: string;
  targets?: string[];
  reveal?: string;
}

export interface TourDefinition {
  id: TourId;
  /** Nome da tela, no selo do painel (chave de tradução). */
  label: string;
  root: string;
  steps: TourStep[];
}

const tour = (name: string) => `[data-tour='${name}']`;
/** Alvo dentro da página: o mesmo componente pode existir num modal ao lado. */
const inside = (root: string, name: string) => `${tour(root)} ${tour(name)}`;

export const TOURS: Record<TourId, TourDefinition> = {
  accounts: {
    id: "accounts",
    label: "Accounts",
    root: tour("accounts-list"),
    steps: [
      {
        id: "list",
        title: "Your accounts",
        description: "Each row is one Roblox account. The colored dot shows if it is online or in a game.",
        targets: [tour("accounts-list")],
      },
      {
        id: "select",
        title: "Select accounts",
        description: "Click an account to select it. Hold Ctrl and click to pick more than one.",
        targets: ["[data-account-row='true']", tour("accounts-list")],
      },
      {
        id: "add",
        title: "Add an account",
        description: "Click Add to bring in a new account. Browser Login is the easiest way.",
        targets: [tour("toolbar-add"), tour("empty-add")],
      },
      {
        id: "panel",
        title: "Account panel",
        description: "With one account selected, the panel on the right shows its details and options. This button shows or hides it.",
        targets: [tour("launch-sidebar"), tour("toolbar-panel")],
      },
      {
        id: "choose-game",
        title: "Play",
        description: "With accounts selected, click Choose Game at the bottom to pick a game and open Roblox.",
        targets: [tour("choose-game-button")],
      },
      {
        id: "names",
        title: "Hide names",
        description: "Hide the account names before you share a screenshot.",
        targets: [tour("toolbar-names")],
      },
    ],
  },

  "choose-game": {
    id: "choose-game",
    label: "Choose Game",
    root: tour("choose-game"),
    steps: [
      {
        id: "accounts",
        title: "Who will play",
        description: "These accounts will join the game. Click the × on one to leave it out.",
        targets: [tour("cg-accounts")],
      },
      {
        id: "tabs",
        title: "Ways to pick a game",
        description: "Each tab is a different way to choose where to play.",
        targets: [tour("cg-tabs")],
      },
      {
        id: "games",
        title: "Games",
        description: "Search for a game. Click it to see its servers, or click Join Game to play now.",
        reveal: tour("cg-tab-games"),
        targets: [tour("cg-panel")],
      },
      {
        id: "servers",
        title: "Servers",
        description: "Pick one server and every account joins it together.",
        reveal: tour("cg-tab-servers"),
        targets: [tour("cg-panel")],
      },
      {
        id: "friends",
        title: "Friends",
        description: "See which friends are online and join the server they are in.",
        reveal: tour("cg-tab-friends"),
        targets: [tour("cg-panel")],
      },
      {
        id: "windows",
        title: "Windows",
        description: "Put all open Roblox windows side by side in a grid.",
        reveal: tour("cg-tab-windows"),
        targets: [tour("cg-panel")],
      },
    ],
  },

  session: {
    id: "session",
    label: "Session",
    root: tour("session-page"),
    steps: [
      {
        id: "joining",
        title: "Joining",
        description: "Accounts waiting to open show here. Click the × to take one out of the line.",
        targets: [inside("session-page", "session-joining")],
      },
      {
        id: "in-game",
        title: "In game",
        description: "Accounts with Roblox open show here. Focus brings a window to the front.",
        targets: [inside("session-page", "session-in-game")],
      },
      {
        id: "actions",
        title: "Keep or close",
        description: "AFK Mode keeps these accounts in the game. Close accounts closes their Roblox windows.",
        targets: [inside("session-page", "session-in-game-actions"), inside("session-page", "session-in-game")],
      },
      {
        id: "unidentified",
        title: "Unidentified clients",
        description: "A Roblox window the app could not match to an account. Tell the app which account it is.",
        targets: [inside("session-page", "session-unidentified"), inside("session-page", "session-in-game")],
      },
      {
        id: "summary",
        title: "Right now",
        description: "A quick count of what is open and running.",
        targets: [tour("session-summary")],
      },
    ],
  },

  afk: {
    id: "afk",
    label: "AFK Mode",
    root: tour("afk-page"),
    steps: [
      {
        id: "tabs",
        title: "Two ways to stay in game",
        description: "AFK clicks press a key so Roblox does not kick you. Auto Rejoin reopens the game on a timer.",
        reveal: inside("afk-page", "afk-tab-clicks"),
        targets: [inside("afk-page", "afk-tabs")],
      },
      {
        id: "clicks-settings",
        title: "What to send",
        description: "Choose how often to send, and what: a key or a click.",
        targets: [inside("afk-page", "afk-clicks-settings")],
      },
      {
        id: "clicks-accounts",
        title: "Which accounts",
        description: "Tick the accounts that get the clicks. Only games opened by this app show here.",
        targets: [inside("afk-page", "afk-clicks-accounts")],
      },
      {
        id: "clicks-start",
        title: "Start and stop",
        description: "Click Start AFK Mode. The bar turns green while it runs. Stopping never closes a game.",
        targets: [inside("afk-page", "afk-clicks-start")],
      },
      {
        id: "rejoin-server",
        title: "Auto Rejoin: where",
        description: "Choose where to rejoin: where the accounts are now, or a favorite game.",
        reveal: inside("afk-page", "afk-tab-rejoin"),
        targets: [inside("afk-page", "afk-rejoin-server"), inside("afk-page", "afk-rejoin-start")],
      },
      {
        id: "rejoin-accounts",
        title: "Auto Rejoin: who",
        description: "Tick the accounts, then click Start Auto Rejoin at the top. Stop Auto Rejoin keeps the games open.",
        targets: [inside("afk-page", "afk-rejoin-accounts"), inside("afk-page", "afk-rejoin-start")],
      },
    ],
  },

  avatars: {
    id: "avatars",
    label: "Avatars",
    root: tour("avatars-page"),
    steps: [
      {
        id: "build",
        title: "Build",
        description: "Make a look from free Roblox items. Nothing here costs Robux.",
        reveal: tour("avatars-tab-build"),
        targets: [tour("avatars-tab-build")],
      },
      {
        id: "parts",
        title: "Pick the parts",
        description: "Choose a part on the left, then click an item in the middle.",
        targets: [tour("avatars-categories")],
      },
      {
        id: "preview",
        title: "Preview",
        description: "See your look here. Randomize all picks a full look for you.",
        targets: [tour("avatars-preview")],
      },
      {
        id: "save",
        title: "Save",
        description: "Give it a name and click Save.",
        targets: [tour("avatars-save")],
      },
      {
        id: "distribute",
        title: "Distribute",
        description: "Tick the saved looks you want to hand out.",
        reveal: tour("avatars-tab-distribute"),
        targets: [tour("avatars-handout"), tour("avatars-tab-distribute")],
      },
      {
        id: "apply",
        title: "Apply",
        description: "Tick the accounts and click Apply avatars. Each account gets one look.",
        targets: [tour("avatars-accounts")],
      },
    ],
  },

  groups: {
    id: "groups",
    label: "Groups",
    root: tour("groups-page"),
    steps: [
      {
        id: "search",
        title: "Find a group",
        description: "Type a group name, or paste its link or ID, and click Search.",
        targets: [tour("groups-search")],
      },
      {
        id: "pick",
        title: "Pick the group",
        description: "Click a card to choose it. The tag says if anyone can join or if the owner approves each request.",
        targets: [tour("groups-results"), tour("groups-search")],
      },
      {
        id: "accounts",
        title: "Pick the accounts",
        description: "Tick the accounts that should join. The ones selected in your list come ticked.",
        targets: [tour("groups-accounts")],
      },
      {
        id: "join",
        title: "Join",
        description: "Click Join group. Accounts join one at a time. If Roblox asks for a captcha, use Solve in browser on that account.",
        targets: [tour("groups-join"), tour("groups-accounts")],
      },
    ],
  },

  scripts: {
    id: "scripts",
    label: "Scripts",
    root: tour("scripts-page"),
    steps: [
      {
        id: "list",
        title: "Your scripts",
        description: "Scripts are for advanced users. They automate this app, not the Roblox game.",
        targets: [tour("scripts-list")],
      },
      {
        id: "new",
        title: "New script",
        description: "New starts a blank script or a ready template.",
        targets: [tour("scripts-new")],
      },
      {
        id: "editor",
        title: "Edit and run",
        description: "Pick a script to see its code and run it. Only run scripts you trust.",
        targets: [tour("scripts-editor")],
      },
    ],
  },

  theme: {
    id: "theme",
    label: "Theme",
    root: tour("theme-page"),
    steps: [
      {
        id: "presets",
        title: "Presets",
        description: "Start from a ready-made look. Pick one from the list.",
        targets: [tour("theme-presets")],
      },
      {
        id: "colors",
        title: "Colors and fonts",
        description: "Change any color or font. You see the change right away.",
        targets: [tour("theme-colors")],
      },
      {
        id: "save",
        title: "Save",
        description: "Click Save to keep it. Leave without saving and the old look comes back.",
        targets: [tour("theme-save")],
      },
    ],
  },

  settings: {
    id: "settings",
    label: "Settings",
    root: tour("settings-page"),
    steps: [
      {
        id: "sections",
        title: "Sections",
        description: "Settings are split into sections. Click one on the left.",
        targets: [tour("settings-sections")],
      },
      {
        id: "autosave",
        title: "Saved for you",
        description: "Changes save on their own. There is no Save button.",
        targets: [tour("settings-content")],
      },
      {
        id: "backups",
        title: "Backups",
        description: "Backups keep a copy of your accounts and settings. Make one before big changes.",
        reveal: tour("settings-section-backups"),
        targets: [tour("settings-content")],
      },
    ],
  },

  changelog: {
    id: "changelog",
    label: "What's new",
    root: tour("changelog-page"),
    steps: [
      {
        id: "list",
        title: "Updates",
        description: "Each update is listed here, newest at the top.",
        targets: [tour("changelog-list"), tour("changelog-page")],
      },
      {
        id: "current",
        title: "Your version",
        description: "The colored dot marks the version you have now.",
        targets: [tour("changelog-current"), tour("changelog-list")],
      },
      {
        id: "update",
        title: "Get the update",
        description: "When a newer version exists, a button here installs it.",
        targets: [tour("changelog-update"), tour("changelog-list")],
      },
    ],
  },
};
