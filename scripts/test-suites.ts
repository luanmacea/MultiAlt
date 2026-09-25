/**
 * Suítes de teste por funcionalidade.
 *
 * Ao mexer numa funcionalidade, rode só a suíte dela (`bun run t <suite>`) —
 * segundos em vez de minutos. A suíte completa (`bun run check`) continua
 * obrigatória antes do commit, porque só ela pega quebra cruzada.
 *
 * - `rust`: filtros passados ao `cargo test` (casam com o caminho do teste,
 *   ou seja, o nome do `mod <nome>_tests` e o nome de cada teste).
 * - `front`: caminhos/globs passados ao `vitest run`.
 *
 * Ao criar um `mod ..._tests` novo ou um `*.test.ts(x)` novo, encaixe-o numa
 * suíte aqui — `bun run t --audit` falha se algo ficar de fora.
 */
export interface TestSuite {
  /** Uma linha explicando o que a suíte cobre. */
  description: string;
  rust: string[];
  front: string[];
  /** Alvos de `src-tauri/tests/*.rs` (rodados com `cargo test --test <nome>`). */
  rustIntegration?: string[];
}

export const SUITES: Record<string, TestSuite> = {
  launch: {
    description: "Launch de uma conta, canal/build do Roblox, URL de launch, old join",
    rust: [
      "launch_url_tests",
      "launch_command_tests",
      "launch_queue_tests",
      "singleton_event_tests",
      "launch_resolve_tests",
      "launch_shared_helper_tests",
      "channel_follow_tests",
      "channel_build_pairing_tests",
      "browser_tracker_tests",
      "win_process_tests",
      "win_tracker_tests",
      "win_client_settings_tests",
    ],
    front: [
      "src/store.test.ts",
      "src/components/ChooseGameScreen.test.tsx",
      "src/components/session",
      "src/components/dialogs/SessionDialog.test.tsx",
    ],
  },
  "join-links": {
    description: "Links de convite, VIP/privado, deep links e share links",
    rust: [
      "join_link_tests",
      "join_link_helper_tests",
      "join_link_http_tests",
      "join_link_resolve_tests",
      "share_link_csrf_tests",
      "private_link_parsing_tests",
      "private_link_http_tests",
      "private_link_extra_http_tests",
    ],
    front: ["src/components/ChooseGameScreen.test.tsx"],
  },
  accounts: {
    description: "Store de contas, criptografia, modelo e comandos de conta",
    rust: [
      "account_store_tests",
      "account_helpers_tests",
      "account_path_tests",
      "account_api_tests",
      "account_api_http_tests",
      "remember_unlock_tests",
      "crypto_tests",
      "model_tests",
      "data::accounts::tests",
    ],
    front: ["src/components/accounts", "src/components/dialogs/AddAccountDialog.test.tsx"],
  },
  botting: {
    description: "Modo botting e watcher de processos",
    rust: ["botting_command_tests", "watcher_tests"],
    front: ["src/components/dialogs/BottingDialog.test.tsx"],
  },
  isolation: {
    description: "Isolamento pré-launch (cache, registro, MachineGuid/MAC)",
    rust: ["win_isolation_tests", "isolation_command_tests"],
    front: ["src/components/settings"],
  },
  versions: {
    description: "Catálogo e instalação de versões do Roblox",
    rust: [
      "win_versions_tests",
      "versions_catalog_tests",
      "versions_command_tests",
      "versions_atomic_tests",
      "updater_tests",
    ],
    front: ["src/components/dialogs/VersionsDialog.test.tsx", "src/updaterChannels.test.ts"],
  },
  api: {
    description: "Cliente da API do Roblox (auth, users, thumbnails, economy, presence, batch)",
    rust: [
      "auth_http_tests",
      "auth_extra_tests",
      "csrf_retry_tests",
      "user_api_tests",
      "user_http_tests",
      "avatar_games_http_tests",
      "avatar_games_extra_tests",
      "thumbnail_http_tests",
      "thumbnail_batch_tests",
      "economy_http_tests",
      "economy_extra_tests",
      "social_presence_http_tests",
      "social_presence_extra_tests",
      "http_retry_tests",
      "http_client_tests",
      "endpoint_host_tests",
      "image_cache_tests",
      "image_cache_command_tests",
    ],
    front: [],
  },
  webserver: {
    description: "Servidor HTTP local: rotas, senha e bloqueio anti-CSRF",
    rust: [
      "handlers_basic_tests",
      "handlers_launch_tests",
      "middleware_tests",
      "password_tests",
      "server_helpers_tests",
      "server_query_tests",
      "server_state_tests",
      "route_wrapper_tests",
      "services_command_tests",
    ],
    front: ["src/components/settings/settingsTabs.test.tsx"],
  },
  nexus: {
    description: "Servidor WebSocket do Nexus",
    rust: ["nexus_query_tests"],
    front: [],
  },
  scripts: {
    description: "Scripts do usuário: store, sandbox e redação de segredos",
    rust: ["scripts_store_tests"],
    rustIntegration: ["security_regression_scripts_store"],
    front: [
      "src/scripting",
      "src/components/dialogs/scriptsRedact.test.ts",
      "src/components/dialogs/scriptsInvokeSecurity.test.tsx",
      "src/components/dialogs/ScriptsDialog.test.tsx",
    ],
  },
  backups: {
    description: "Backup e restauração dos dados, e onde a pasta de dados fica",
    rust: ["backups_tests", "settings_paths_tests"],
    front: ["src/components/dialogs/BackupsDialog.test.tsx"],
  },
  friends: {
    description: "Amigos online por conta e entrada no servidor do amigo",
    rust: [
      "friends_online_tests",
      "friends_online_http_tests",
      "presence_cookie_tests",
      "online_friends_batch_tests",
    ],
    front: ["src/components/friends"],
  },
  servers: {
    description: "Escolha de servidor (aleatório/vazio/cheio), região e navegador de servidores",
    rust: [
      "server_preference_tests",
      "server_pick_http_tests",
      "server_region_format_tests",
      "server_region_payload_tests",
      "server_region_http_tests",
      "server_scan_budget_tests",
      "server_scan_dedupe_tests",
    ],
    front: ["src/components/servers", "src/components/server-list"],
  },
  settings: {
    description: "RAMSettings.ini, defaults, temas e presets",
    rust: [
      "ini_tests",
      "ini_structure_tests",
      "settings_store_tests",
      "settings_paths_tests",
      "theme_store_tests",
      "theme_preset_tests",
      "theme_font_command_tests",
    ],
    front: [
      "src/components/settings",
      "src/theme.test.ts",
      "src/themeFonts.test.ts",
      "src/fontPresets.test.ts",
      "src/hooks/useSettings.test.ts",
    ],
  },
  chromium: {
    description: "Chromium via CDP: download, argumentos, sessão de login",
    rust: [
      "chromium_cdp_tests",
      "chromium_manager_tests",
      "chromium_commands_tests",
      "chromium_download_tests",
    ],
    front: [],
  },
  signup: {
    description: "Criação de contas no formulário do Roblox (CAPTCHA pelo usuário)",
    rust: [
      "signup_identity_tests",
      "signup_script_tests",
      "signup_session_tests",
      "username_check_tests",
      "username_check_http_tests",
      "generator_failure_budget_tests",
    ],
    front: ["src/components/signup"],
  },
  ui: {
    description: "Casca da UI: layout, menus, componentes genéricos, i18n, hooks",
    rust: [],
    front: [
      "src/components/layout",
      "src/components/menus",
      "src/components/ui",
      "src/components/server-list",
      "src/App.test.tsx",
      "src/hooks",
      "src/i18n",
      "src/types.test.ts",
      "src/utils",
      "src/featureFlags.test.ts",
    ],
  },
  diagnostics: {
    description: "Diagnóstico de mutex, janelas, otimização de processo e capacidades da plataforma",
    rust: [
      "diagnostics_tests",
      "win_windowing_tests",
      "win_optimization_tests",
      "generator_command_tests",
      "generator_http_tests",
      "platform_info_tests",
    ],
    front: ["src/utils/platform.test.ts"],
  },
};

export const SUITE_NAMES = Object.keys(SUITES).sort();
