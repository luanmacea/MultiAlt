# Documentação — Roblox Account Manager 4

Roblox Account Manager 4 (RAM4) é um gerenciador desktop de múltiplas contas Roblox escrito em **Tauri 2** (backend Rust + frontend React/TypeScript). Ele permite:

- guardar várias contas (cookie `.ROBLOSECURITY`, senha opcional, alias, grupo, campos livres) em um arquivo local, opcionalmente criptografado com senha;
- lançar um ou vários clientes Roblox ao mesmo tempo (multi-Roblox), em jogos públicos, Job IDs específicos ou servidores VIP/privados;
- navegar por jogos/servidores, manter favoritos (com vários links VIP por jogo) e jogos recentes;
- automatizar re-join de contas (Botting Mode), vigiar processos (Watcher), isolar sessões antes do launch e fixar versões específicas do cliente Roblox;
- expor uma API HTTP local (feature `webserver`) e um servidor WebSocket para scripts Lua (feature `nexus`);
- rodar scripts JavaScript do usuário em um sandbox (Web Worker) dentro do app.

Plataforma principal: **Windows** (APIs Win32 para processos, registro, janelas, DPAPI). macOS é parcial.

## Como navegar

| Se você quer... | Leia |
|---|---|
| Entender como as peças se encaixam (IPC, stores, arquivos, eventos, startup) | [architecture.md](architecture.md) |
| Rodar, buildar, adicionar um comando Tauri, extrair traduções | [development.md](development.md) |
| Entender uma funcionalidade específica | [features/](#funcionalidades) |
| Achar uma funcionalidade na tela (o que existe e onde fica) | [mapa-da-interface.md](mapa-da-interface.md) |
| Saber o que está confuso na interface e o que corrigir primeiro | [ux-checkup.md](ux-checkup.md) |

Cada documento de funcionalidade segue a mesma estrutura: **Objetivo**, **Onde fica o código**, **Fluxo**, **Regras de negócio**, **Configurações relacionadas**, **Armadilhas / cuidados**.

## Documentos gerais

- [architecture.md](architecture.md) — arquitetura geral, persistência, feature flags, eventos backend→frontend, fluxo de inicialização.
- [development.md](development.md) — setup, comandos, features do Cargo, i18n, convenções, passo a passo para novo comando Tauri.
- [mapa-da-interface.md](mapa-da-interface.md) — onde fica cada funcionalidade na tela e para que serve, em português.
- [ux-checkup.md](ux-checkup.md) — revisão de usabilidade de setembro/2026: 131 achados triados por prioridade.

## Funcionalidades

### Contas e sessão
- [features/backups.md](features/backups.md) — backup/restauração dos dados pelo app e onde a pasta de dados fica (o `.exe` é portátil).
- [features/accounts.md](features/accounts.md) — modelo de conta, adicionar/remover/importar, criptografia do `AccountData.json`, tela de senha, grupos (incl. `moderadas`), campos, comandos de API por conta.
- [features/account-creation.md](features/account-creation.md) — criar contas em série: formulário preenchido pelo app com o CAPTCHA resolvido pelo usuário, e o gerador por provedor (BloxGen).
- [features/authentication.md](features/authentication.md) — cookie, CSRF, auth ticket, retry de sessão (`run_with_session_retry`), refresh de cookie.

### Launch e automação
- [features/launch.md](features/launch.md) — lançamento de uma conta.
- [features/multi-launch.md](features/multi-launch.md) — lançamento de várias contas / multi-Roblox.
- [features/botting.md](features/botting.md) — Botting Mode (auto-rejoin cíclico).
- [features/isolation.md](features/isolation.md) — isolamento pré-launch (cache, registro, MachineGuid/MAC).
- [features/roblox-versions.md](features/roblox-versions.md) — instalação e seleção de versões do cliente Roblox.
- [features/watcher.md](features/watcher.md) — monitoramento de processos Roblox.

### Integrações
- [features/webserver.md](features/webserver.md) — API HTTP local (feature `webserver`).
- [features/chromium.md](features/chromium.md) — navegador Chromium via CDP: login por janela/senha, captura do cookie e o que foi mitigado na porta de debug.
- [features/nexus.md](features/nexus.md) — servidor WebSocket para Nexus.lua (feature `nexus`).
- [features/scripts.md](features/scripts.md) — scripts do usuário e sandbox do frontend.

### Interface
- [features/server-choice.md](features/server-choice.md) — preferência de servidor (aleatório/mais vazio/mais cheio), filtro por região e a aba Servers da Choose Game.
- [features/server-list.md](features/server-list.md) — navegador de servidores/jogos, favoritos, recentes, VIP.
- [features/friends.md](features/friends.md) — aba "Friends": amigos online por conta e entrada de todas as contas no servidor do amigo.
- [features/join-links.md](features/join-links.md) — campo único de "Join link": convites de experiência, links VIP/privados, links de jogo e deep links.
- [features/settings.md](features/settings.md) — abas de configuração e chaves do `RAMSettings.ini`.
- [features/ui-layout.md](features/ui-layout.md) — shell da UI, temas/fontes, diálogos, lista de contas, barra de ações, tela "Choose Game".

## Registro de mudanças (2026-09-25)

**Escolher onde o lote entra**

1. **Preferência de servidor por lote**: `Best fit` (padrão — o mais cheio que ainda caiba o lote com **uma vaga de folga**), `Fullest`, `Emptiest`, `Random` e `Let Roblox choose`. O servidor é resolvido uma vez para o lote inteiro — [server-choice.md](features/server-choice.md).
2. **Aba Servers** na Choose Game: lista com ocupação, região e ping, varredura assíncrona página a página (profundidade configurável), filtro por país e Join que manda todas as contas selecionadas.
3. **Região do servidor**: a API não devolve isso, então vem de `join-game-instance` → IP → geolocalização, sob demanda e com cache. Conserta de quebra o "Load Region" do navegador de servidores, que fazia `fetch` do frontend para um serviço que hoje exige desafio do Cloudflare.
4. **Servidor repetido entre páginas** quebrava a reordenação da lista (chave duplicada no React). A varredura passou a deduplicar por Job ID — [server-choice.md](features/server-choice.md#servidor-repetido-entre-páginas).

**Contas**

5. **Criação em série no navegador**: o app preenche o cadastro (usuário, senha, 18+, masculino), confere antes se o nome está livre, e o usuário só resolve o CAPTCHA. O laço de reparo preenche apenas campo vazio, para nunca brigar com o que está na tela — [account-creation.md](features/account-creation.md).
6. **Gerador BloxGen** deixou de tentar para sempre quando o erro nunca passa (estoque vazio, saldo zerado, chave vencida).
7. **Aba Friends**: amigos online de cada conta selecionada; clicar num amigo manda todas as contas para o servidor dele. A rota antiga do Roblox saiu do ar e respondia 404 — [friends.md](features/friends.md).

**Sessão e interface**

8. **Painel de Sessão**: fila de contas entrando (cancelar uma, parar a fila — sem fechar cliente nenhum) e lista de contas em jogo (focar/fechar) — [ui-layout.md](features/ui-layout.md#painel-de-sessão), [multi-launch.md](features/multi-launch.md#fila-observável-e-cancelamento).
9. **"Lembrar de mim" na tela de senha** (padrão 24 h), com a senha protegida pelo DPAPI do usuário do Windows e prazo dentro do blob cifrado — [authentication.md](features/authentication.md#lembrar-de-mim-na-tela-de-senha).
10. **Atalhos pedidos**: "x" nos chips de conta da Choose Game, botão de servidores em cada jogo da aba Games, e entrada **Create Accounts** no menu Add.

**Base**

11. **XSRF por serviço**: o token do `auth.roblox.com` é recusado pelo `apis.roblox.com` — toda chamada mutável repete uma vez com o token que o próprio serviço devolve no 403 — [authentication.md](features/authentication.md#o-token-é-por-serviço--send_with_csrf_retry).
12. **Multi-instância sem fechar o jogo aberto**: o app fecha o `ROBLOX_singletonEvent` dos clientes já rodando — [launch.md](features/launch.md#regras-de-negócio).
13. **Harness de UI** (`bun run dev:ui`): roda o frontend no navegador com o lado Tauri dublado, com cenários por URL (inclusive um com dados reais da API). Foi com ele que a lista fora de ordem foi reproduzida e a correção conferida — [development.md](development.md#validando-a-ui-no-navegador).

## Registro de mudanças (2026-09-24)

1. **Dados saíram da pasta do executável** para `%LOCALAPPDATA%\Roblox Account Manager`, com migração que copia (sem apagar a origem nem sobrescrever o destino), modo portátil por `portable.txt` e override por `RAM_DATA_DIR` — [architecture.md](architecture.md#arquivos-de-persistência).
2. **Backups dentro do app**: criar, listar, restaurar e apagar, com backup automático de segurança antes de restaurar — [backups.md](features/backups.md).
3. **Suítes de teste por funcionalidade** (`bun run t <suite>`) com auditoria no `bun run check` — [development.md](development.md#suítes-por-funcionalidade-o-dia-a-dia).

## Registro de mudanças (2026-09-22)

0. **O app segue o canal do Roblox em vez de fixá-lo.** Forçar `production` fazia o launch pelo *site* divergir e chamar o instalador do Roblox (que fecha os clientes abertos). Agora `launch_url`/`default_player_dir` leem o canal do registro, garantem que a build daquele canal esteja instalada (baixando-a em silêncio) e abrem ela direto — app e site concordam, e cada build é baixada uma única vez — [launch.md](features/launch.md#canal-do-roblox-e-a-tela-de-atualização-causa-raiz-e-fix).

1. **Atualização do Roblox não abre mais o instalador dele:** quando a build production não está instalada, o launcher baixa e instala essa build sozinho (`ensure_production_player_exe` + `install_build_to_dir`, progresso no evento `roblox-build-install`) — [launch.md](features/launch.md#canal-do-roblox-e-a-tela-de-atualização-causa-raiz-e-fix).
2. **Join links:** campo único na aba Follow aceita convite de experiência, link VIP/privado, `vip:<código>`, link de jogo, servidor específico, deep link e link curto — [join-links.md](features/join-links.md).
3. **Testes:** 160 testes Rust (incluindo API do Roblox com HTTP mockado via wiremock e a costura `api/endpoints.rs`) e 164 no frontend (vitest); portão único `bun run check` — [development.md](development.md#testes).

## Registro de correções (2026-09-21)

1. Canal fixado em `production` também no old join sem catálogo (`default_player_dir`), cache da build production de 60 s e ClientSettings gravados na pasta realmente lançada (`refresh_production_version` + `get_roblox_path`) — [launch.md](features/launch.md#canal-do-roblox-e-a-tela-de-atualização-causa-raiz-e-fix).
2. Mutex `ROBLOX_singletonMutex` adquirido, segurado e liberado numa thread dedicada (`multi-roblox-mutex`) — [launch.md](features/launch.md#regras-de-negócio).
3. Isolamento pré-launch não fecha mais clientes abertos: com Roblox rodando ele é pulado — [isolation.md](features/isolation.md).
4. `launch_multiple` checa o cancelamento logo antes do spawn de cada conta (Close All para a conta em andamento) — [multi-launch.md](features/multi-launch.md).
5. `kill_for_user` / `kill_for_user_graceful` só matam se o PID ainda for Roblox (proteção contra reuso de PID) — [launch.md](features/launch.md#regras-de-negócio), [watcher.md](features/watcher.md).
6. `stop_botting_mode(closeBotAccounts)` fecha só os bots da sessão, não mais todo cliente não-player — [botting.md](features/botting.md).
7. `AccountData.json`: save atômico, bloqueio de save após load com falha e após lock, `update_account` preserva cookie/senha do store — [accounts.md](features/accounts.md).
8. `make_selected_friends`: uma execução por vez, delay 500–60000 ms e nunca renova sessão (refresh desloga todas as sessões) — [accounts.md](features/accounts.md#regras-de-negócio), [authentication.md](features/authentication.md).
9. Web server: bloqueio de requisições de páginas web (Origin/Sec-Fetch-Site), senha obrigatória para cookies e `check_password` em ImportCookie e rotas de edição/ação — [webserver.md](features/webserver.md).
10. Nexus: handshake com `Origin` recusado, desconexão não apaga a conexão nova de um rejoin, auto-execute com prazo de 60 s — [nexus.md](features/nexus.md).
11. Scripts: settings secretas removidas do `window:update` e mais APIs de rede/armazenamento bloqueadas no Worker — [scripts.md](features/scripts.md).
12. Frontend: listener `account-moderated` sem nome velho, cleanup de `listen()` com flag `disposed`, `onDragEnd` na alça de arrasto, recentes gravados pela store só em sucesso e Gap da grade aplicado no blur/Enter — [ui-layout.md](features/ui-layout.md), [server-list.md](features/server-list.md), [development.md](development.md).

## Glossário rápido

| Termo | Significado |
|---|---|
| Cookie / `SecurityToken` | Valor do cookie `.ROBLOSECURITY` da conta, usado em todas as chamadas autenticadas. |
| Place ID | ID do "lugar" (jogo) no Roblox. |
| Job ID | ID de uma instância de servidor específica. |
| VIP / servidor privado | Servidor privado; no app é representado por `vip:<código>`, link com `privateServerLinkCode`, share link ou access code. |
| Grupo | String livre em `Account.Group`; prefixo numérico (`"01 Main"`) define a ordem. |
| Multi-Roblox | Rodar vários clientes simultâneos (controlado por `General.EnableMultiRbx`). |
