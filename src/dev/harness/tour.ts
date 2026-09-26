/**
 * Dados do cenário `tour`: o app inteiro com conteúdo plausível em toda tela.
 *
 * Os outros cenários miram uma tela e um comportamento. Este mira o contrário:
 * **nada pode estar vazio**, porque tela vazia esconde justamente o que a
 * revisão de usabilidade procura — onde o botão está, se o rótulo explica o
 * que faz, se o campo pede número decorado. Favoritos sem nenhum jogo não
 * responde "dá para ver os servidores daqui?".
 *
 * Nenhum número aqui vem da rede: é fixture, escolhida para parecer o que o
 * Roblox devolveria.
 */
import type { InvokeHandler } from "./bus";
import { iconForGame } from "./games";

/** Jogos populares como a busca devolve (nomes e places reais, contagem inventada). */
const GAMES = [
  { rootPlaceId: 6516141723, universeId: 2680623874, name: "Blox Fruits", playerCount: 412_003, totalUpVotes: 9_100_000, totalDownVotes: 1_200_000 },
  { rootPlaceId: 4924922222, universeId: 1685831367, name: "Brookhaven RP", playerCount: 388_540, totalUpVotes: 7_400_000, totalDownVotes: 1_600_000 },
  { rootPlaceId: 920587237, universeId: 372226183, name: "Adopt Me!", playerCount: 122_870, totalUpVotes: 6_200_000, totalDownVotes: 1_900_000 },
  { rootPlaceId: 2753915549, universeId: 1537690962, name: "Blox Fruits Trading", playerCount: 88_210, totalUpVotes: 2_100_000, totalDownVotes: 300_000 },
  { rootPlaceId: 15101393044, universeId: 5272564064, name: "Steal a Brainrot", playerCount: 741_902, totalUpVotes: 3_300_000, totalDownVotes: 410_000 },
  { rootPlaceId: 189707, universeId: 66654135, name: "Natural Disaster Survival", playerCount: 12_430, totalUpVotes: 1_100_000, totalDownVotes: 120_000 },
  { rootPlaceId: 606849621, universeId: 245662005, name: "Jailbreak", playerCount: 41_205, totalUpVotes: 4_800_000, totalDownVotes: 700_000 },
  { rootPlaceId: 8737602449, universeId: 3244096465, name: "Pet Simulator 99", playerCount: 64_118, totalUpVotes: 2_900_000, totalDownVotes: 330_000 },
  { rootPlaceId: 142823291, universeId: 51103135, name: "Murder Mystery 2", playerCount: 55_040, totalUpVotes: 3_700_000, totalDownVotes: 500_000 },
  { rootPlaceId: 286090429, universeId: 111958650, name: "Arsenal", playerCount: 18_902, totalUpVotes: 2_400_000, totalDownVotes: 260_000 },
];

/** Favoritos e recentes moram no localStorage do frontend — o harness os semeia. */
export function seedTourStorage(): void {
  const favorites = [
    {
      placeId: 15101393044,
      name: "Steal a Brainrot",
      iconUrl: null,
      addedAt: Date.now() - 86_400_000 * 3,
      vipServers: [
        { id: "vip-1", name: "Servidor da galera", link: "https://www.roblox.com/share?code=abc123&type=Server" },
      ],
    },
    { placeId: 606849621, name: "Jailbreak", iconUrl: null, addedAt: Date.now() - 86_400_000 * 9 },
    { placeId: 6516141723, name: "Blox Fruits", iconUrl: null, addedAt: Date.now() - 86_400_000 * 21 },
  ];
  const recent = [
    { placeId: 15101393044, name: "Steal a Brainrot", iconUrl: null, lastPlayed: Date.now() - 3_600_000 },
    { placeId: 189707, name: "Natural Disaster Survival", iconUrl: null, lastPlayed: Date.now() - 86_400_000 },
    { placeId: 606849621, name: "Jailbreak", iconUrl: null, lastPlayed: Date.now() - 86_400_000 * 4 },
  ];
  try {
    localStorage.setItem("ram_favorite_games", JSON.stringify(favorites));
    localStorage.setItem("ram_recent_games", JSON.stringify(recent));
  } catch {
    // Navegador sem storage: o resto do cenário continua valendo.
  }
}

const SCRIPT_PERMISSIONS = {
  allowInvoke: false,
  allowHttp: true,
  allowWebSocket: false,
  allowWindow: false,
  allowModal: true,
  allowSettings: false,
  allowUi: true,
};

/**
 * Respostas de leitura para as telas que o cenário `default` deixaria vazias.
 *
 * Só leitura: comando que muda alguma coisa continua caindo no `base`, e o
 * agente confere o que a tela tentou fazer em `window.__harness.calls()`.
 */
export function tourHandler(base: InvokeHandler, userIds: number[]): InvokeHandler {
  return (cmd, args) => {
    switch (cmd) {
      case "search_games": {
        const keyword = String(args.keyword || "");
        if (!keyword) return { sorts: [{ games: GAMES }] };
        const hits = GAMES.filter((g) => g.name.toLowerCase().includes(keyword.toLowerCase()));
        return {
          searchResults: [
            { contents: hits.map((g) => ({ ...g, contentType: "Game" })) },
          ],
        };
      }
      case "batched_get_game_icon": {
        const placeId = Number(args.placeId ?? 0);
        const game = GAMES.find((g) => g.rootPlaceId === placeId);
        // Antes isto devolvia sempre `null` e nenhuma tela era vista com ícone.
        return game ? iconForGame(placeId, game.name) : null;
      }
      case "get_place_details": {
        const ids = (args.placeIds as number[]) || [];
        return ids.map((placeId) => {
          const game = GAMES.find((g) => g.rootPlaceId === placeId);
          return {
            placeId,
            universeId: game?.universeId ?? 0,
            name: game?.name ?? `Place ${placeId}`,
            description: "Fixture do harness.",
            sourceName: game?.name ?? `Place ${placeId}`,
            sourceDescription: "Fixture do harness.",
            url: `https://www.roblox.com/games/${placeId}`,
          };
        });
      }

      case "get_scripts":
        return [
          {
            id: "script-auto-rejoin",
            name: "Auto rejoin no servidor favorito",
            description: "Entra de novo quando o cliente cai.",
            language: "javascript",
            source: "ram.log('exemplo')\n",
            enabled: true,
            trusted: false,
            autoStart: false,
            permissions: SCRIPT_PERMISSIONS,
            createdAtMs: Date.now() - 86_400_000 * 12,
            updatedAtMs: Date.now() - 86_400_000,
          },
          {
            id: "script-robux",
            name: "Relatório de Robux",
            description: "",
            language: "javascript",
            source: "ram.log('robux')\n",
            enabled: false,
            trusted: true,
            autoStart: true,
            permissions: { ...SCRIPT_PERMISSIONS, allowInvoke: true },
            createdAtMs: Date.now() - 86_400_000 * 40,
            updatedAtMs: Date.now() - 86_400_000 * 2,
          },
        ];

      case "versions_list_installed":
        return [
          {
            channel: "production",
            versionHash: "version-8f2c1d9a44e24b10",
            binaryType: "WindowsPlayer",
            displayVersion: "0.712.1.7120586",
            installPath: "C:\\Users\\voce\\AppData\\Local\\Roblox\\Versions\\version-8f2c1d9a44e24b10",
            installSizeBytes: 412_398_112,
            installedAt: new Date(Date.now() - 86_400_000 * 6).toISOString(),
            lastLaunchedAt: new Date(Date.now() - 3_600_000).toISOString(),
            userLabel: null,
          },
          {
            channel: "zintegration",
            versionHash: "version-1a7b3c5d9e0f4a21",
            binaryType: "WindowsPlayer",
            displayVersion: "0.713.0.7130042",
            installPath: "C:\\Users\\voce\\AppData\\Local\\Roblox\\Versions\\version-1a7b3c5d9e0f4a21",
            installSizeBytes: 418_002_944,
            installedAt: new Date(Date.now() - 86_400_000 * 2).toISOString(),
            lastLaunchedAt: null,
            userLabel: "Teste",
          },
        ];
      case "versions_list_remote":
        return {
          current: [
            { channel: "production", versionHash: "version-8f2c1d9a44e24b10", displayVersion: "0.712.1.7120586", binaryType: "WindowsPlayer" },
          ],
          past: [
            { channel: "production", versionHash: "version-6d4e2f8a11c34b55", displayVersion: "0.711.0.7110331", binaryType: "WindowsPlayer" },
          ],
          pastError: null,
        };

      case "list_backups":
        return [
          {
            id: "bkp-2026-09-20",
            fileName: "RAMBackup-2026-09-20-0312.zip",
            createdAt: new Date(Date.now() - 86_400_000 * 5).toISOString(),
            label: null,
            sizeBytes: 18_204,
            files: ["AccountData.json", "RAMSettings.ini"],
            valid: true,
            automatic: true,
          },
          {
            id: "bkp-antes-da-limpeza",
            fileName: "RAMBackup-antes-da-limpeza.zip",
            createdAt: new Date(Date.now() - 86_400_000 * 18).toISOString(),
            label: "antes da limpeza",
            sizeBytes: 17_880,
            files: ["AccountData.json"],
            valid: true,
            automatic: false,
          },
        ];
      case "backups_info":
        return { dir: "C:\\Users\\voce\\AppData\\Roaming\\RAM4\\Backups", portable: false, totalBytes: 36_084, count: 2 };

      case "get_botting_mode_status":
        return {
          active: false,
          startedAtMs: null,
          placeId: 0,
          jobId: "",
          launchData: "",
          intervalMinutes: 30,
          launchDelaySeconds: 10,
          playerGraceMinutes: 5,
          playerUserIds: [],
          userIds: [],
          accounts: [],
        };
      case "get_generator_status":
        return {
          active: false,
          startedAtMs: null,
          provider: "",
          endpoint: "",
          accountType: "",
          extraDelaySeconds: 0,
          targetGroup: "",
          maxAccounts: 0,
          phase: "idle",
          nextAttemptAtMs: null,
          totalGenerated: 0,
          lastUsername: null,
        };
      case "get_signup_status":
        return {
          active: false,
          current: 0,
          total: 0,
          created: 0,
          phase: "idle",
          identity: null,
          lastError: null,
          createdUsernames: [],
        };

      case "get_web_server_status":
        return { running: false, port: 7963 };
      case "get_nexus_status":
        return { running: false, port: null, connected_count: 0 };
      case "get_nexus_accounts":
        return [];
      case "get_nexus_elements":
        return [];
      case "get_nexus_log":
        return [];

      case "get_theme_presets":
        return [];
      case "get_blocked_users":
        return [
          { userId: 991_001, name: "AlguemChato" },
          { userId: 991_002, name: "Spammer123" },
        ];
      case "get_robux":
        return 4_235;
      case "get_presence":
        return { userPresenceType: 0, lastLocation: "Website" };
      case "get_outfits":
        return [];
      case "get_universe_places":
        return [{ id: 15101393044, name: "Steal a Brainrot" }];

      case "isolation_list_adapters":
        return [
          { subkey: "0012", description: "Intel(R) Wi-Fi 6 AX201", currentMac: null, netCfgInstanceId: "{9A1B-...}" },
          { subkey: "0015", description: "Realtek PCIe GbE Family Controller", currentMac: "02:1A:44:9C:31:7E", netCfgInstanceId: "{4C7D-...}" },
        ];
      case "list_display_monitors":
        return [
          { index: 0, width: 2560, height: 1440, primary: true },
          { index: 1, width: 1920, height: 1080, primary: false },
        ];

      case "get_running_instances":
        return userIds.slice(0, 2).map((userId, i) => ({
          userId,
          pid: 4200 + i,
          placeId: 15101393044,
          jobId: `job-em-jogo-${i}`,
        }));

      default:
        return base(cmd, args);
    }
  };
}
