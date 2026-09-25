/**
 * Cenários do harness de UI.
 *
 * Cada cenário monta um backend de mentira com dados realistas e, quando faz
 * sentido, **manda os eventos aos poucos** — é justamente o que só aparece com
 * a tela montada: lista que chega em páginas, progresso, estado final.
 *
 * Escolha pela URL: `http://localhost:1420/?scenario=servers-big-game&accounts=6`.
 * Cada agente abre a sua própria URL e olha um cenário diferente. `&lang=pt`
 * (ou `de`) sobe a tela naquele idioma, sem passar pelo seletor de Settings.
 *
 * Um cenário **nunca** inventa o comportamento que está sendo testado: ele
 * entrega os mesmos dados que a API do Roblox entregaria (inclusive na ordem
 * ruim), para a UI ter que se virar.
 */
import realPlacePages from "./fixtures/place-15101393044.json";
import {
  harnessCalls,
  harnessEmit,
  resetHarnessCalls,
  setInvokeHandler,
  type InvokeHandler,
} from "./bus";
import { seedTourStorage, tourHandler } from "./tour";

const params = new URLSearchParams(window.location.search);
const scenarioName = params.get("scenario") || "default";
const accountCount = Math.max(1, Math.min(Number(params.get("accounts") ?? 6) || 6, 16));
const language = params.get("lang");

function account(index: number) {
  return {
    UserID: 1000 + index,
    Username: `TestAccount${index}`,
    Alias: "",
    Description: "",
    Group: "",
    SecurityToken: `cookie-${index}`,
    Password: "",
    Fields: {},
    Valid: true,
    LastUse: new Date().toISOString(),
    DateAdded: new Date().toISOString(),
    Region: "",
    Order: index,
    Moderated: false,
    Banned: false,
  };
}

const accounts = Array.from({ length: accountCount }, (_, i) => account(i + 1));

const settings: Record<string, Record<string, string>> = {
  General: {
    ...(language ? { Language: language } : {}),
    RestrictedBackgroundStyle: "waves",
    ServerPreference: "bestfit",
    ServerRegionFilter: "",
    ServerScanPages: "30",
    MaxRecentGames: "8",
  },
};

/** Base comum: o app sobe destrancado, com contas e settings. */
const baseHandler: InvokeHandler = (cmd) => {
  switch (cmd) {
    case "needs_password":
      return false;
    case "is_accounts_encrypted":
      return false;
    case "get_accounts":
      return accounts;
    case "get_all_settings":
      return settings;
    case "get_platform_capabilities":
      return { isWindows: true, isMacos: false, supportsIsolation: true, supportsMultiRoblox: true };
    case "remembered_unlock_state":
      return { supported: true, active: false, defaultHours: 24 };
    case "get_launch_queue":
      return { entries: [], active: false, placeId: 0, jobId: "" };
    case "get_running_instances":
      return [];
    case "batched_get_avatar_headshots":
      return [];
    case "update_setting":
      return null;
    // Sem atualização: o diálogo de update não pode tapar a tela em teste.
    case "check_for_updates_with_channels":
      return null;
    case "get_theme":
      return null;
    default:
      // Comando sem resposta no cenário: `[]` é o que a maioria das telas
      // espera, e o aviso no console diz ao agente o que ainda falta cobrir.
      console.warn("[harness] comando sem resposta:", cmd);
      return [];
  }
};

/**
 * Jogo grande, servidores de 13 lugares: exatamente a forma que a API devolve
 * com `sortOrder=Desc` — páginas e páginas de servidores quase cheios antes de
 * aparecer qualquer um que caiba um lote grande.
 */
function bigGamePages(): { id: string; playing: number; maxPlayers: number; ping: number }[][] {
  const pages: { id: string; playing: number; maxPlayers: number; ping: number }[][] = [];
  let serial = 0;
  const make = (playing: number) => ({
    id: `job-${String(++serial).padStart(4, "0")}`,
    playing,
    maxPlayers: 13,
    ping: 20 + (serial % 180),
  });

  // 3 páginas só de servidores cheios demais para um lote de 6.
  for (let page = 0; page < 3; page++) {
    pages.push(Array.from({ length: 100 }, (_, i) => make(12 - (i % 3))));
  }
  // A página em que finalmente aparecem servidores utilizáveis, misturados.
  pages.push([
    ...Array.from({ length: 60 }, () => make(9)),
    make(5), // folga 8 — cabe, mas quase vazio
    make(6), // folga 7 — cabe com folga 1: o ideal
    make(7), // folga 6 — cabe exatamente, sem folga
    ...Array.from({ length: 37 }, () => make(10)),
  ]);
  return pages;
}

/**
 * Emite a varredura do place real página a página, como o backend faz.
 *
 * Fica separado porque dois cenários usam a mesma entrega: o que existe para
 * conferir a ordem da lista e o `tour`, que só precisa da aba cheia.
 */
function emitRealPlaceScan(
  pages: { id: string; playing: number; maxPlayers: number; ping: number | null }[][],
  placeId: number
): void {
  const all: (typeof pages)[number] = [];
  pages.forEach((page, index) => {
    setTimeout(() => {
      all.push(...page);
      harnessEmit("server-scan", {
        scanId: 1,
        placeId,
        servers: [
          ...all.filter((s) => s.playing + accountCount <= s.maxPlayers),
          ...all.filter((s) => s.playing + accountCount > s.maxPlayers),
        ].slice(0, 150),
        scanned: all.length,
        fitting: all.filter((s) => s.playing + accountCount <= s.maxPlayers).length,
        done: index === pages.length - 1,
        stoppedAtLimit: false,
        error: null,
      });
    }, 250 * (index + 1));
  });
}

const SCENARIOS: Record<string, () => void> = {
  default() {
    setInvokeHandler(baseHandler);
  },

  /**
   * O app inteiro com conteúdo: favoritos, recentes, busca de jogos, scripts,
   * versões, backups, contas em jogo. É o cenário da revisão de usabilidade —
   * tela vazia esconde onde o botão está e se o rótulo explica o que ele faz.
   */
  tour() {
    seedTourStorage();
    const pages = realPlacePages as { id: string; playing: number; maxPlayers: number; ping: number | null }[][];
    const withTour = tourHandler(baseHandler, accounts.map((a) => a.UserID));
    setInvokeHandler((cmd, args) => {
      if (cmd === "start_server_scan") {
        emitRealPlaceScan(pages, Number(args.placeId) || 0);
        return 1;
      }
      if (cmd === "stop_server_scan") return null;
      return withTour(cmd, args);
    });
  },

  /**
   * Aba Servers de um jogo grande. Valida a pergunta que sempre volta: o topo
   * da lista é mesmo o melhor encaixe para o lote?
   */
  "servers-big-game"() {
    const pages = bigGamePages();
    setInvokeHandler((cmd, args) => {
      if (cmd === "start_server_scan") {
        const scanId = 1;
        const all: ReturnType<typeof bigGamePages>[number] = [];
        pages.forEach((page, index) => {
          setTimeout(() => {
            all.push(...page);
            const fitting = all.filter(
              (s) => s.playing + accountCount <= s.maxPlayers
            ).length;
            harnessEmit("server-scan", {
              scanId,
              placeId: Number(args.placeId) || 0,
              // Como o backend faz: quem cabe o lote vai na frente **antes**
              // do corte dos 150, senão o recorte esconde justamente os bons.
              servers: [
                ...all.filter((s) => s.playing + accountCount <= s.maxPlayers),
                ...all.filter((s) => s.playing + accountCount > s.maxPlayers),
              ].slice(0, 150),
              scanned: all.length,
              fitting,
              done: index === pages.length - 1,
              stoppedAtLimit: false,
              error: null,
            });
          }, 250 * (index + 1));
        });
        return scanId;
      }
      if (cmd === "stop_server_scan") return null;
      if (cmd === "get_server_regions") {
        const jobIds = (args.jobIds as string[]) || [];
        return jobIds.map((jobId, i) => ({
          jobId,
          region: {
            ip: `1.2.3.${i}`,
            city: i % 2 === 0 ? "São Paulo" : "Ashburn",
            region: "",
            country: i % 2 === 0 ? "Brazil" : "United States",
            countryCode: i % 2 === 0 ? "BR" : "US",
          },
          label: i % 2 === 0 ? "São Paulo, BR" : "Ashburn, US",
          error: null,
        }));
      }
      return baseHandler(cmd, args);
    });
  },

  /**
   * Réplica com **dados reais** do place 15101393044 (o do relato): 4 páginas
   * capturadas da API do Roblox, entregues página a página como a varredura
   * faz. É o cenário para conferir a ordem da lista contra o mundo real.
   */
  "servers-real-place"() {
    const pages = realPlacePages as { id: string; playing: number; maxPlayers: number; ping: number | null }[][];
    setInvokeHandler((cmd, args) => {
      if (cmd === "start_server_scan") {
        emitRealPlaceScan(pages, Number(args.placeId) || 0);
        return 1;
      }
      if (cmd === "stop_server_scan") return null;
      return baseHandler(cmd, args);
    });
  },

  /**
   * O caso ruim: o backend diz que existem servidores que cabem, mas a lista
   * enviada foi cortada antes deles. A tela não pode fingir que os primeiros
   * servem — tem que dizer que os bons ainda não chegaram.
   */
  "servers-truncated"() {
    setInvokeHandler((cmd, args) => {
      if (cmd === "start_server_scan") {
        setTimeout(() => {
          harnessEmit("server-scan", {
            scanId: 1,
            placeId: Number(args.placeId) || 0,
            servers: Array.from({ length: 20 }, (_, i) => ({
              id: `job-cheio-${i}`,
              playing: 10,
              maxPlayers: 13,
              ping: 40,
            })),
            scanned: 1700,
            fitting: 68,
            done: true,
            stoppedAtLimit: false,
            error: null,
          });
        }, 200);
        return 1;
      }
      if (cmd === "stop_server_scan") return null;
      return baseHandler(cmd, args);
    });
  },

  /** Nenhum servidor cabe o lote: a lista tem que abrir pelos que levam mais contas. */
  "servers-no-fit"() {
    setInvokeHandler((cmd, args) => {
      if (cmd === "start_server_scan") {
        setTimeout(() => {
          harnessEmit("server-scan", {
            scanId: 1,
            placeId: Number(args.placeId) || 0,
            servers: [
              { id: "uma-vaga", playing: 12, maxPlayers: 13, ping: 40 },
              { id: "tres-vagas", playing: 10, maxPlayers: 13, ping: 40 },
              { id: "duas-vagas", playing: 11, maxPlayers: 13, ping: 40 },
              { id: "quatro-vagas", playing: 9, maxPlayers: 13, ping: 40 },
            ],
            scanned: 4,
            fitting: 0,
            done: true,
            stoppedAtLimit: false,
            error: null,
          });
        }, 200);
        return 1;
      }
      if (cmd === "stop_server_scan") return null;
      return baseHandler(cmd, args);
    });
  },

  /** Varredura interrompida pelo limite de páginas. */
  "servers-page-limit"() {
    setInvokeHandler((cmd, args) => {
      if (cmd === "start_server_scan") {
        setTimeout(() => {
          harnessEmit("server-scan", {
            scanId: 1,
            placeId: Number(args.placeId) || 0,
            servers: [{ id: "job-1", playing: 11, maxPlayers: 13, ping: 30 }],
            scanned: 3000,
            fitting: 0,
            done: true,
            stoppedAtLimit: true,
            error: null,
          });
        }, 200);
        return 1;
      }
      if (cmd === "stop_server_scan") return null;
      return baseHandler(cmd, args);
    });
  },

  /** Amigos online de cada conta, com uma conta falhando. */
  "friends-online"() {
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_online_friends_for_accounts") {
        const ids = (args.userIds as number[]) || [];
        return ids.map((userId, index) => ({
          userId,
          error: index === 1 ? "Failed to get online friends (status 429)" : null,
          friends:
            index === 1
              ? []
              : [
                  {
                    userId: 7000 + index,
                    name: `friend${index}`,
                    displayName: `Friend ${index}`,
                    presenceType: 2,
                    lastLocation: "Some Game",
                    placeId: 606849621,
                    rootPlaceId: 606849621,
                    gameId: `job-friend-${index}`,
                  },
                  {
                    userId: 8000 + index,
                    name: `website${index}`,
                    displayName: `On Site ${index}`,
                    presenceType: 1,
                    lastLocation: "Website",
                    placeId: null,
                    rootPlaceId: null,
                    gameId: null,
                  },
                ],
        }));
      }
      return baseHandler(cmd, args);
    });
  },

  /** Fila de launch e contas em jogo (Painel de Sessão). */
  "launch-queue"() {
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_launch_queue") {
        return {
          entries: accounts.map((a, i) => ({
            userId: a.UserID,
            state: i === 0 ? "done" : i === 1 ? "launching" : "queued",
            error: null,
            updatedAtMs: Date.now(),
          })),
          active: true,
          placeId: 606849621,
          jobId: "",
        };
      }
      if (cmd === "get_running_instances") {
        return [{ userId: accounts[0].UserID, pid: 4242, placeId: 606849621, jobId: "job-1" }];
      }
      return baseHandler(cmd, args);
    });
  },
};

(SCENARIOS[scenarioName] ?? SCENARIOS.default)();

/** Deixa o agente inspecionar o cenário pelo console do navegador. */
(window as unknown as Record<string, unknown>).__harness = {
  scenario: scenarioName,
  accounts: accountCount,
  emit: harnessEmit,
  calls: harnessCalls,
  resetCalls: resetHarnessCalls,
  scenarios: Object.keys(SCENARIOS),
};
